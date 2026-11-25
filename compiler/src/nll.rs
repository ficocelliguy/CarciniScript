//! The borrow checker: rustc's NLL over the IR of `nll_lower.rs`. Per function:
//!
//! 1. Liveness of every local at every point (backward dataflow over the CFG).
//! 2. Region inference: a region's value is the set of points where a local whose type mentions
//!    it is live, closed under the outlives constraints the lowering collected. The signature's
//!    lifetimes are universal regions holding every point plus an "end" bit each.
//! 3. Loans: each `&place` is in scope from its creation until the borrowed place is overwritten
//!    or goes out of scope, and only at points inside its region. A two-phase borrow (a `&mut`
//!    method receiver) is shared until its call activates it.
//! 4. Every access is checked against the loans in scope (E0499, E0502, E0503, E0505, E0506);
//!    a loan still in scope when its place's storage ends is E0597 / E0716; a loan whose region
//!    reaches a universal region's end escapes the function (E0515, or E0597 through an
//!    argument); a universal region made to outlive one it isn't declared to is "lifetime may
//!    not live long enough".
//!
//! These are errors, as in rustc. Lifetimes are erased in the emitted JS, so with strict borrows
//! off (`set_strict(false)`) they're warnings and the program still runs.

use std::collections::HashMap;

use ra_ap_syntax::ast;


use crate::adt::Adts;
use crate::nll_bir::*;
use crate::nll_rty::Env;
use crate::ty::Ty;
use crate::typeck::RawDiag;

static STRICT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Report borrow conflicts as errors (the program doesn't compile, as with rustc; the default)
/// or, with `false`, as warnings.
pub fn set_strict(on: bool) {
    STRICT.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn strict() -> bool {
    STRICT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Functions with more points than this are skipped (the editor must never stutter).
const MAX_POINTS: usize = 30_000;

/// Borrow-check every function of `file`.
pub fn check(
    file: &ast::SourceFile,
    src: &str,
    types: &HashMap<(usize, usize), Ty>,
    adts: &Adts,
    origin: &crate::typeck::Origin,
) -> Vec<RawDiag> {
    #[cfg(not(target_arch = "wasm32"))]
    let t0 = std::time::Instant::now();
    let env = Env::new(file, src, types, adts);
    // (Generated code, bundled preludes: not the user's to fix.)
    let bodies = crate::nll_lower::lower_file(&env, file, &|off| origin.generated(off));
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var_os("NLL_DUMP").is_some() {
        for b in &bodies {
            eprintln!("{}", crate::nll_bir::dump(b));
        }
    }
    let strict = strict();
    let mut out: Vec<RawDiag> = Vec::new();
    for body in &bodies {
        if body.point_count() > MAX_POINTS {
            continue;
        }
        for (span, msg, spans) in check_body(&env, body) {
            if out
                .iter()
                .any(|d| d.start == span.0 && d.end == span.1 && d.message.starts_with(&msg))
            {
                continue;
            }
            out.push(RawDiag {
                start: span.0,
                end: span.1,
                message: msg,
                severity: if strict { "error" } else { "warning" },
                // (The help says it runs anyway when borrow conflicts are warnings.)
                help: None,
                spans: spans.into_iter().map(|((s, e), l)| (s, e, l)).collect(),
            });
        }
    }
    out.sort_by_key(|d| (d.start, d.end));
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::var_os("NLL_TIME").is_some() {
        let points: usize = bodies.iter().map(Body::point_count).sum();
        eprintln!(
            "nll: {} bodies, {points} points, {:.2} ms",
            bodies.len(),
            t0.elapsed().as_secs_f64() * 1e3
        );
    }
    out
}

struct Loan {
    point: usize,
    region: Region,
    mutable: bool,
    two_phase: bool,
    place: Place,
    /// Per projection of `place`: whether it derefs a shared reference.
    shared_derefs: Vec<bool>,
    span: Span,
}

/// What a statement does to the loans, computed once: the loan it creates, the loans it
/// kills (the overwritten or dead place is a prefix of theirs), the two-phase loans it
/// activates.
#[derive(Default, Clone)]
struct Effects {
    creates: Option<usize>,
    kills: Vec<usize>,
    activations: Vec<(Place, Option<usize>)>,
}

struct Analysis<'b, 'e, 'a> {
    env: &'e Env<'a>,
    body: &'b Body,
    /// The first point of each block.
    block_start: Vec<usize>,
    n_points: usize,
    preds: Vec<Vec<Block>>,
    loans: Vec<Loan>,
    /// Per point: the statement's effects on the loans.
    effects: Vec<Effects>,
    /// Region values: points, then an end bit per region (used for universal ones).
    values: Vec<BitSet>,
    /// Loans reported as escaping or outliving their place (not reported again).
    reported: Vec<bool>,
    /// (where, message, rustc's secondary spans: the conflicting loan's)
    diags: Vec<(Span, String, Vec<(Span, String)>)>,
}

/// The loans in scope at a point: in scope, and activated (two-phase).
#[derive(Clone, PartialEq, Default)]
struct LoanState {
    scope: BitSet,
    active: BitSet,
}

fn check_body(env: &Env, body: &Body) -> Vec<(Span, String, Vec<(Span, String)>)> {
    let mut block_start = Vec::with_capacity(body.blocks.len());
    let mut n = 0;
    for b in &body.blocks {
        block_start.push(n);
        n += b.stmts.len() + 1;
    }
    let mut preds: Vec<Vec<Block>> = vec![Vec::new(); body.blocks.len()];
    for (i, b) in body.blocks.iter().enumerate() {
        for s in b.term.successors() {
            preds[s.idx()].push(Block(i as u32));
        }
    }
    let mut a = Analysis {
        env,
        body,
        block_start,
        n_points: n,
        preds,
        loans: Vec::new(),
        effects: Vec::new(),
        values: Vec::new(),
        reported: Vec::new(),
        diags: Vec::new(),
    };
    a.collect_loans();
    a.collect_effects();
    let live = a.liveness();
    a.infer_regions(&live);
    a.check_universals();
    a.check_returns();
    a.check_loans();
    a.diags
}

impl Analysis<'_, '_, '_> {
    fn point(&self, b: Block, i: usize) -> usize {
        self.block_start[b.idx()] + i
    }

    fn field(&self, v: &str, f: &str, adt: &RTy) -> Option<RTy> {
        let mut scratch = Body::default();
        self.env.field_rty(&mut scratch, v, f, adt)
    }

    /// Which projections of `place` deref a shared reference.
    fn shared_derefs(&self, place: &Place) -> Vec<bool> {
        let mut out = Vec::with_capacity(place.proj.len());
        for i in 0..place.proj.len() {
            let prefix = Place {
                local: place.local,
                proj: place.proj[..i].to_vec(),
            };
            let shared = place.proj[i] == Proj::Deref
                && matches!(
                    self.body.place_rty(&prefix, &|v, f, t| self.field(v, f, t)),
                    RTy::Ref(_, false, _)
                );
            out.push(shared);
        }
        out
    }

    fn collect_loans(&mut self) {
        for (bi, b) in self.body.blocks.iter().enumerate() {
            for (si, s) in b.stmts.iter().enumerate() {
                if let StmtKind::Assign(
                    _,
                    Rvalue::Ref {
                        region,
                        mutable,
                        place,
                        two_phase,
                    },
                ) = &s.kind
                {
                    // (A literal, a `static` or a promoted constant: `'static`, nothing to
                    // check.)
                    if place.local == Local(0) {
                        continue;
                    }
                    let shared_derefs = self.shared_derefs(place);
                    self.loans.push(Loan {
                        point: self.point(Block(bi as u32), si),
                        region: *region,
                        mutable: *mutable,
                        two_phase: *two_phase,
                        place: place.clone(),
                        shared_derefs,
                        span: s.span,
                    });
                }
            }
        }
        self.reported = vec![false; self.loans.len()];
    }

    fn collect_effects(&mut self) {
        let mut effects = vec![Effects::default(); self.n_points];
        // Loans by base local, for the kills.
        let mut by_local: HashMap<Local, Vec<usize>> = HashMap::new();
        for (li, l) in self.loans.iter().enumerate() {
            by_local.entry(l.place.local).or_default().push(li);
        }
        for (bi, b) in self.body.blocks.iter().enumerate() {
            for (si, s) in b.stmts.iter().enumerate() {
                let p = self.point(Block(bi as u32), si);
                let e = &mut effects[p];
                match &s.kind {
                    StmtKind::Assign(dest, rv) => {
                        if let Rvalue::Ref { .. } = rv {
                            e.creates = self.loans.iter().position(|l| l.point == p);
                        }
                        if let Rvalue::Ops { activate, .. } = rv {
                            for (place, region) in activate {
                                let li = self.loans.iter().position(|l| {
                                    l.region == *region && l.two_phase && l.place == *place
                                });
                                e.activations.push((place.clone(), li));
                            }
                        }
                        // Overwriting a place ends the loans of it and of what's inside it.
                        for li in by_local.get(&dest.local).into_iter().flatten() {
                            let loan = &self.loans[*li];
                            if loan.point != p && dest.is_prefix_of(&loan.place) {
                                e.kills.push(*li);
                            }
                        }
                    }
                    StmtKind::StorageDead(l) => {
                        e.kills.extend(by_local.get(l).into_iter().flatten().copied());
                    }
                }
            }
        }
        self.effects = effects;
    }

    // ------------------------------------------------------------ liveness

    /// The locals live on entry to each point.
    fn liveness(&self) -> Vec<BitSet> {
        let nb = self.body.blocks.len();
        let nl = self.body.locals.len();
        // Per block: the locals used before being defined, and the ones defined.
        let mut gens = vec![BitSet::new(nl); nb];
        let mut kill = vec![BitSet::new(nl); nb];
        for (bi, b) in self.body.blocks.iter().enumerate() {
            let (g, k) = (&mut gens[bi], &mut kill[bi]);
            let use_ = |l: Local, k: &BitSet, g: &mut BitSet| {
                if !k.contains(l.idx()) {
                    g.insert(l.idx());
                }
            };
            for s in &b.stmts {
                let (uses, def) = stmt_uses(s, self.body);
                for l in uses {
                    use_(l, k, g);
                }
                if let Some(d) = def {
                    k.insert(d.idx());
                }
            }
            if let Term::Switch(Some(op), _) = &b.term {
                use_(op.place.local, k, g);
            }
        }
        let mut live_in = vec![BitSet::new(nl); nb];
        let mut live_out = vec![BitSet::new(nl); nb];
        let mut changed = true;
        while changed {
            changed = false;
            for bi in (0..nb).rev() {
                let mut out = BitSet::new(nl);
                for s in self.body.blocks[bi].term.successors() {
                    out.union_with(&live_in[s.idx()]);
                }
                let mut inn = out.clone();
                for d in kill[bi].iter() {
                    inn.remove(d);
                }
                inn.union_with(&gens[bi]);
                if out != live_out[bi] || inn != live_in[bi] {
                    changed = true;
                    live_out[bi] = out;
                    live_in[bi] = inn;
                }
            }
        }
        // Per point, walking each block backward.
        let mut at = vec![BitSet::new(nl); self.n_points];
        for (bi, b) in self.body.blocks.iter().enumerate() {
            let mut cur = live_out[bi].clone();
            let tp = self.point(Block(bi as u32), b.stmts.len());
            if let Term::Switch(Some(op), _) = &b.term {
                cur.insert(op.place.local.idx());
            }
            at[tp] = cur.clone();
            for (si, s) in b.stmts.iter().enumerate().rev() {
                let (uses, def) = stmt_uses(s, self.body);
                if let Some(d) = def {
                    cur.remove(d.idx());
                }
                for l in uses {
                    cur.insert(l.idx());
                }
                at[self.point(Block(bi as u32), si)] = cur.clone();
            }
        }
        at
    }

    // ------------------------------------------------------------ regions

    fn infer_regions(&mut self, live: &[BitSet]) {
        let nr = self.body.regions.len();
        let total = self.n_points + nr;
        let mut values = vec![BitSet::new(total); nr];
        // Universal regions: every point, their own end, and the ends they're declared to
        // outlive (`'static`: every end).
        for r in 0..nr {
            if let Some(u) = &self.body.regions[r].universal {
                for p in 0..self.n_points {
                    values[r].insert(p);
                }
                values[r].insert(self.n_points + r);
                if r == 0 {
                    for x in 0..nr {
                        if self.body.regions[x].universal.is_some() {
                            values[r].insert(self.n_points + x);
                        }
                    }
                } else {
                    let mut stack: Vec<Region> = u.outlives.clone();
                    while let Some(v) = stack.pop() {
                        if values[r].insert(self.n_points + v.idx())
                            && let Some(vu) = &self.body.regions[v.idx()].universal
                        {
                            stack.extend(vu.outlives.iter().copied());
                        }
                    }
                }
            }
        }
        // Liveness: a region is live where a local mentioning it is.
        let mut local_regions: Vec<Vec<Region>> = Vec::with_capacity(self.body.locals.len());
        for l in &self.body.locals {
            let mut rs = Vec::new();
            l.rty.regions(&mut rs);
            rs.sort();
            rs.dedup();
            local_regions.push(rs);
        }
        for (p, set) in live.iter().enumerate() {
            for l in set.iter() {
                for r in &local_regions[l] {
                    values[r.idx()].insert(p);
                }
            }
        }
        // A loan's region holds the point of the borrow.
        for l in &self.loans {
            values[l.region.idx()].insert(l.point);
        }
        // Constraints to a fixpoint.
        let mut changed = true;
        while changed {
            changed = false;
            for c in &self.body.constraints {
                let (li, si) = (c.longer.idx(), c.shorter.idx());
                if li == si {
                    continue;
                }
                let (longer, shorter) = if li < si {
                    let (a, b) = values.split_at_mut(si);
                    (&mut a[li], &b[0])
                } else {
                    let (a, b) = values.split_at_mut(li);
                    (&mut b[0], &a[si])
                };
                if longer.union_with(shorter) {
                    changed = true;
                }
            }
        }
        self.values = values;
    }

    fn region_contains(&self, r: Region, p: usize) -> bool {
        self.values[r.idx()].contains(p)
    }

    /// The universal regions whose end a region reaches.
    fn escapes_to(&self, r: Region) -> Vec<Region> {
        let mut out = Vec::new();
        for x in 0..self.body.regions.len() {
            if self.body.regions[x].universal.is_some()
                && self.values[r.idx()].contains(self.n_points + x)
            {
                out.push(Region(x as u32));
            }
        }
        out
    }

    /// A universal region made to outlive one it isn't declared to.
    fn check_universals(&mut self) {
        let nr = self.body.regions.len();
        for r in 1..nr {
            let Some(u) = &self.body.regions[r].universal else {
                continue;
            };
            let mut declared = vec![r];
            let mut stack: Vec<Region> = u.outlives.clone();
            while let Some(v) = stack.pop() {
                if !declared.contains(&v.idx()) {
                    declared.push(v.idx());
                    if let Some(vu) = &self.body.regions[v.idx()].universal {
                        stack.extend(vu.outlives.iter().copied());
                    }
                }
            }
            let extra: Vec<usize> = (0..nr)
                .filter(|x| {
                    self.body.regions[*x].universal.is_some()
                        && self.values[r].contains(self.n_points + x)
                        && !declared.contains(x)
                })
                .collect();
            if extra.is_empty() {
                continue;
            }
            // Where it came from: the first constraint into `r` whose source carries the end.
            let span = self
                .body
                .constraints
                .iter()
                .find(|c| {
                    c.longer.idx() == r
                        && extra
                            .iter()
                            .any(|x| self.values[c.shorter.idx()].contains(self.n_points + x))
                })
                .map(|c| c.span)
                .or_else(|| self.body.returns.first().map(|s| s.span));
            if let Some(span) = span {
                self.diags
                    .push((span, "lifetime may not live long enough".to_string(), Vec::new()));
            }
        }
    }

    /// References to locals returned from the function.
    fn check_returns(&mut self) {
        let ret_regions = {
            let mut rs = Vec::new();
            self.body.locals[1].rty.regions(&mut rs);
            rs
        };
        let sites = self.body.returns.clone();
        for site in sites {
            for li in 0..self.loans.len() {
                let loan = &self.loans[li];
                if self.reported[li] || loan.place.has_deref() {
                    continue;
                }
                let ends = self.escapes_to(loan.region);
                if !ends.iter().any(|e| ret_regions.contains(e)) {
                    continue;
                }
                let local = &self.body.locals[loan.place.local.idx()];
                let msg = match (local.kind, site.direct_ref) {
                    (LocalKind::Temp, true) => "cannot return reference to temporary value".to_string(),
                    (LocalKind::Temp, false) => "cannot return value referencing temporary value".to_string(),
                    (_, true) => format!("cannot return reference to local variable `{}`", local.name),
                    (_, false) => format!("cannot return value referencing local variable `{}`", local.name),
                };
                self.reported[li] = true;
                self.diags.push((site.span, msg, Vec::new()));
            }
        }
    }

    // ------------------------------------------------------------ loans

    fn check_loans(&mut self) {
        let nb = self.body.blocks.len();
        let nl = self.loans.len();
        let empty = LoanState {
            scope: BitSet::new(nl),
            active: BitSet::new(nl),
        };
        let mut entry: Vec<LoanState> = vec![empty.clone(); nb];
        let mut exit: Vec<LoanState> = vec![empty.clone(); nb];
        // The dataflow: each block's exit state from its entry, successors re-run when their
        // entry grows, to a fixpoint (no reports).
        let mut queued = vec![true; nb];
        let mut worklist: std::collections::VecDeque<usize> = (0..nb).collect();
        while let Some(bi) = worklist.pop_front() {
            queued[bi] = false;
            let out = self.transfer_block(bi, &entry[bi], false);
            if out == exit[bi] && bi != 0 && !self.preds[bi].is_empty() {
                continue;
            }
            exit[bi] = out;
            for s in self.body.blocks[bi].term.successors() {
                let si = s.idx();
                let grew = entry[si].scope.union_with(&exit[bi].scope)
                    | entry[si].active.union_with(&exit[bi].active);
                if grew && !queued[si] {
                    queued[si] = true;
                    worklist.push_back(si);
                }
            }
        }
        // The reporting pass.
        for bi in 0..nb {
            let st = entry[bi].clone();
            self.transfer_block(bi, &st, true);
        }
    }

    /// Run a block from `entry`, returning the state at its end; with `report`, check every
    /// access on the way.
    fn transfer_block(&mut self, bi: usize, entry: &LoanState, report: bool) -> LoanState {
        let mut st = entry.clone();
        let block = Block(bi as u32);
        let stmts = self.body.blocks[bi].stmts.len();
        for si in 0..stmts {
            let p = self.point(block, si);
            self.prune(&mut st, p);
            let stmt = &self.body.blocks[bi].stmts[si];
            match &stmt.kind {
                StmtKind::Assign(dest, rv) => {
                    match rv {
                        Rvalue::Use(op) => {
                            if report {
                                self.check_access(&st, p, &op.place, op.access(), op.span);
                            }
                        }
                        Rvalue::Ref {
                            mutable,
                            place,
                            two_phase,
                            ..
                        } => {
                            // (A two-phase borrow is only reserved here; its call activates
                            // it, and reports.)
                            if report && !*two_phase {
                                let access = if *mutable { Access::MutBorrow } else { Access::Borrow };
                                self.check_access(&st, p, place, access, stmt.span);
                            }
                        }
                        Rvalue::Ops { args, .. } => {
                            if report {
                                for op in args {
                                    self.check_access(&st, p, &op.place, op.access(), op.span);
                                }
                            }
                            let activations = self.effects[p].activations.clone();
                            for (place, li) in &activations {
                                if report {
                                    self.check_activation(&st, p, place, *li, stmt.span);
                                }
                                if let Some(li) = li {
                                    st.active.insert(*li);
                                }
                            }
                        }
                    }
                    if report {
                        self.check_access(&st, p, dest, Access::Write, stmt.span);
                    }
                    // The new loan; the loans this overwrite ends.
                    if let Some(li) = self.effects[p].creates {
                        st.scope.insert(li);
                    }
                    for li in &self.effects[p].kills {
                        st.scope.remove(*li);
                        st.active.remove(*li);
                    }
                }
                StmtKind::StorageDead(l) => {
                    if report {
                        self.check_storage_dead(&st, p, *l);
                    }
                    for li in &self.effects[p].kills {
                        st.scope.remove(*li);
                        st.active.remove(*li);
                    }
                }
            }
        }
        let p = self.point(block, stmts);
        self.prune(&mut st, p);
        if report && let Term::Switch(Some(op), _) = &self.body.blocks[bi].term {
            let span = self.body.blocks[bi].term_span;
            self.check_access(&st, p, &op.place, Access::ShallowRead, span);
        }
        st
    }

    /// Drop the loans whose region doesn't reach `p`.
    fn prune(&self, st: &mut LoanState, p: usize) {
        let dead: Vec<usize> = st
            .scope
            .iter()
            .filter(|li| !self.region_contains(self.loans[*li].region, p))
            .collect();
        for li in dead {
            st.scope.remove(li);
            st.active.remove(li);
        }
    }

    fn loan_mutable(&self, st: &LoanState, li: usize) -> bool {
        let l = &self.loans[li];
        l.mutable && (!l.two_phase || st.active.contains(li))
    }

    fn check_access(&mut self, st: &LoanState, p: usize, place: &Place, access: Access, span: Span) {
        // (The constant local stands in for literals: nothing borrows it.)
        if place.local == Local(0) {
            return;
        }
        let deep = access.is_deep();
        let mut hit: Option<(usize, bool)> = None;
        for li in st.scope.iter() {
            let loan = &self.loans[li];
            if loan.point == p {
                continue;
            }
            let sd = &loan.shared_derefs;
            if !places_conflict(&loan.place, place, deep, &|i| sd.get(i).copied().unwrap_or(false)) {
                continue;
            }
            let lm = self.loan_mutable(st, li);
            let conflict = match access {
                Access::Read | Access::ShallowRead | Access::Borrow => lm,
                Access::Move | Access::Write | Access::MutBorrow => true,
            };
            if conflict && (hit.is_none() || (lm && !hit.unwrap().1)) {
                hit = Some((li, lm));
            }
        }
        let Some((li, lm)) = hit else { return };
        let at = p;
        let p = self.body.describe(place);
        let loan_label = match (access, lm) {
            (Access::Read | Access::ShallowRead | Access::Write, _) => format!("`{p}` is borrowed here"),
            (Access::Borrow, _) => "mutable borrow occurs here".to_string(),
            (Access::Move, _) => format!("borrow of `{p}` occurs here"),
            (Access::MutBorrow, true) => "first mutable borrow occurs here".to_string(),
            (Access::MutBorrow, false) => "immutable borrow occurs here".to_string(),
        };
        let loan_span = self.loans[li].span;
        let msg = match (access, lm) {
            (Access::Read | Access::ShallowRead, _) => {
                format!("cannot use `{p}` because it was mutably borrowed")
            }
            (Access::Borrow, _) => {
                format!("cannot borrow `{p}` as immutable because it is also borrowed as mutable")
            }
            (Access::Move, _) => format!("cannot move out of `{p}` because it is borrowed"),
            (Access::Write, _) => format!("cannot assign to `{p}` because it is borrowed"),
            (Access::MutBorrow, true) => {
                format!("cannot borrow `{p}` as mutable more than once at a time")
            }
            (Access::MutBorrow, false) => {
                format!("cannot borrow `{p}` as mutable because it is also borrowed as immutable")
            }
        };
        let later = match (access, lm) {
            (Access::MutBorrow, false) => "immutable borrow later used here",
            (Access::Borrow, _) => "mutable borrow later used here",
            (Access::MutBorrow, true) => "first borrow later used here",
            _ => "borrow later used here",
        };
        let mut spans = vec![(loan_span, loan_label)];
        spans.extend(self.later_use(li, at).map(|s| (s, later.to_string())));
        self.diags.push((span, msg, spans));
    }

    /// Where loan `li` is used after the conflict at `p`, for rustc's "borrow later used here":
    /// the nearest statement after `p` (following the control flow, around loops too) that uses
    /// a local whose type holds the loan's region or one it flows into, where the loan is live.
    fn later_use(&self, li: usize, p: usize) -> Option<Span> {
        let loan = &self.loans[li];
        // The regions the loan's flows into (`'loan: 'r`).
        let mut flows = vec![false; self.body.regions.len()];
        flows[loan.region.idx()] = true;
        let mut changed = true;
        while changed {
            changed = false;
            for c in &self.body.constraints {
                if flows[c.longer.idx()] && !flows[c.shorter.idx()] {
                    flows[c.shorter.idx()] = true;
                    changed = true;
                }
            }
        }
        let holds = |l: Local| {
            let mut rs = Vec::new();
            self.body.locals[l.idx()].rty.regions(&mut rs);
            rs.iter().any(|r| flows[r.idx()])
        };
        // (Point `q` as a block and an index in it.)
        let locate = |q: usize| {
            let bi = self.block_start.partition_point(|&s| s <= q) - 1;
            (bi, q - self.block_start[bi])
        };
        let next = |q: usize| -> Vec<usize> {
            let (bi, si) = locate(q);
            let b = &self.body.blocks[bi];
            if si < b.stmts.len() {
                vec![q + 1]
            } else {
                b.term.successors().iter().map(|s| self.block_start[s.idx()]).collect()
            }
        };
        let mut seen = vec![false; self.n_points];
        let mut queue: std::collections::VecDeque<usize> = next(p).into();
        while let Some(q) = queue.pop_front() {
            if std::mem::replace(&mut seen[q], true) {
                continue;
            }
            if self.region_contains(loan.region, q) {
                let (bi, si) = locate(q);
                let b = &self.body.blocks[bi];
                let mut uses: Vec<(Local, Span)> = Vec::new();
                match b.stmts.get(si).map(|s| (&s.kind, s.span)) {
                    Some((StmtKind::Assign(dest, rv), span)) => {
                        // (A write through it, `*r += y`: the statement.)
                        if !dest.is_local() {
                            uses.push((dest.local, span));
                        }
                        match rv {
                            Rvalue::Use(op) => uses.push((op.place.local, op.span)),
                            Rvalue::Ref { place, .. } => uses.push((place.local, span)),
                            Rvalue::Ops { args, activate } => {
                                uses.extend(args.iter().map(|o| (o.place.local, o.span)));
                                uses.extend(activate.iter().map(|(pl, _)| (pl.local, span)));
                            }
                        }
                    }
                    Some((StmtKind::StorageDead(_), _)) => {}
                    None => {
                        if let Term::Switch(Some(op), _) = &b.term {
                            uses.push((op.place.local, op.span));
                        }
                    }
                }
                if let Some(&(_, span)) = uses.iter().find(|(l, _)| *l != Local(0) && holds(*l)) {
                    return Some(span);
                }
            }
            queue.extend(next(q));
        }
        None
    }

    /// A two-phase borrow becomes a `&mut` at its call: like taking it there, against every
    /// other loan.
    fn check_activation(&mut self, st: &LoanState, p: usize, place: &Place, this: Option<usize>, span: Span) {
        let mut hit: Option<(usize, bool)> = None;
        for li in st.scope.iter() {
            if Some(li) == this {
                continue;
            }
            let loan = &self.loans[li];
            // (The call's arguments are read first: a loan whose holder's last use is one of
            // them has ended by the time the `&mut` is taken.)
            if loan.point == p || !self.region_contains(loan.region, p + 1) {
                continue;
            }
            let sd = &loan.shared_derefs;
            if !places_conflict(&loan.place, place, true, &|i| sd.get(i).copied().unwrap_or(false)) {
                continue;
            }
            let lm = self.loan_mutable(st, li);
            if hit.is_none() || (lm && hit.is_some_and(|h| !h.1)) {
                hit = Some((li, lm));
            }
        }
        let Some((li, lm)) = hit else { return };
        let at = p;
        let p = self.body.describe(place);
        let (msg, label) = if lm {
            (format!("cannot borrow `{p}` as mutable more than once at a time"), "first mutable borrow occurs here")
        } else {
            (format!("cannot borrow `{p}` as mutable because it is also borrowed as immutable"), "immutable borrow occurs here")
        };
        let later = if lm { "first borrow later used here" } else { "immutable borrow later used here" };
        let mut spans = vec![(self.loans[li].span, label.to_string())];
        spans.extend(self.later_use(li, at).map(|s| (s, later.to_string())));
        self.diags.push((span, msg, spans));
    }

    /// The end of a local's storage: a loan of it still in scope outlives it.
    fn check_storage_dead(&mut self, st: &LoanState, p: usize, l: Local) {
        let _ = p;
        for li in st.scope.iter() {
            let loan = &self.loans[li];
            if loan.place.local != l || loan.place.has_deref() || self.reported[li] {
                continue;
            }
            let local = &self.body.locals[l.idx()];
            let (span, msg) = match local.kind {
                LocalKind::Temp => (
                    local.span,
                    "temporary value dropped while borrowed".to_string(),
                ),
                _ => (loan.span, format!("`{}` does not live long enough", local.name)),
            };
            self.reported[li] = true;
            self.diags.push((span, msg, Vec::new()));
        }
    }
}

/// The locals a statement reads, and the one it defines (assigns whole).
fn stmt_uses(s: &Stmt, body: &Body) -> (Vec<Local>, Option<Local>) {
    let mut uses = Vec::new();
    let mut def = None;
    match &s.kind {
        StmtKind::Assign(dest, rv) => {
            match rv {
                Rvalue::Use(op) => uses.push(op.place.local),
                Rvalue::Ref { place, .. } => uses.push(place.local),
                Rvalue::Ops { args, activate } => {
                    uses.extend(args.iter().map(|o| o.place.local));
                    uses.extend(activate.iter().map(|(p, _)| p.local));
                }
            }
            if dest.is_local() {
                def = Some(dest.local);
            } else {
                uses.push(dest.local);
            }
        }
        StmtKind::StorageDead(l) => {
            if body.locals[l.idx()].drops {
                uses.push(*l);
            } else {
                def = Some(*l);
            }
        }
    }
    (uses, def)
}
