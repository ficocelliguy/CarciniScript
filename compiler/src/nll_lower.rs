//! Lowering a function body from syntax to the borrow checker's IR (`nll_bir`): every
//! expression becomes statements over places and temporaries in basic blocks, with each local's
//! reference-aware type and the outlives constraints the types imply. Patterns become downcast
//! projections and binding assignments (with rustc's default binding modes), `?` a branch and a
//! return, loops back edges, method calls an auto-ref'd receiver (two-phase for `&mut`),
//! closures separate bodies over captured places.
//!
//! The lowering errs toward silence: an expression whose borrows it can't follow produces an
//! owned temporary holding no references, so no later check can fire on it.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SyntaxNode, T, ast,
    ast::{HasArgList, HasGenericParams, HasLoopBody, HasName, RangeItem},
};

use crate::macros;
use crate::nll_bir::*;
use crate::nll_rty::{Env, FnSyn, Recv, TyEnv, generics_of};
use crate::nll_std;
use crate::ty::Ty;

/// The result of lowering an expression: where its value sits and what type it has.
#[derive(Clone, Debug)]
struct Ex {
    place: Place,
    rty: RTy,
    span: Span,
    /// A place expression (a local, a field, `*r`, `v[i]`), not a temporary holding a value.
    is_place: bool,
}

#[derive(Clone, Copy, Default)]
struct Ctx {
    /// A `let` initializer position where temporaries live to the end of the block
    /// (`let r = &make();`).
    extend: bool,
}

struct Scope {
    names: HashMap<String, Local>,
    locals: Vec<Local>,
}

struct LoopCtx {
    label: Option<String>,
    brk: Block,
    /// None for a labeled block.
    cont: Option<Block>,
    scope_depth: usize,
    /// `break value`: where the value goes.
    result: Option<Local>,
}

/// A variable captured by the closure being lowered.
#[derive(Clone)]
struct Upvar {
    local: Local,
    /// `Some(mutable)` for a capture by reference (the local holds `&var`), None by value.
    by_ref: Option<bool>,
}

/// How a closure uses a captured place.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CapMode {
    Shared,
    Mut,
    Value,
}

pub struct Lower<'e, 'a> {
    env: &'e Env<'a>,
    body: Body,
    cur: Block,
    reachable: bool,
    scopes: Vec<Scope>,
    loops: Vec<LoopCtx>,
    temps: Vec<Vec<Local>>,
    ret_local: Local,
    ret_rty: RTy,
    /// A local standing in for constants and unit values.
    unit: Local,
    /// Offset of the syntax being lowered (non-zero inside a macro argument).
    base: usize,
    upvars: HashMap<String, Upvar>,
    tenv: TyEnv,
    self_adt: Option<String>,
    /// Closure bodies found inside, lowered on their own.
    pub closures: Vec<Body>,
    /// For the closure argument about to be lowered: its parameters' types, from the method it
    /// is passed to (`v.iter().map(|x| ..)`: `x` is the iterator's item).
    pending_hints: Vec<(usize, RTy)>,
    /// In a closure body: the enclosing body's regions each universal region stands for.
    imported: HashMap<Region, Region>,
    /// Expression nesting, bounded so a pathological file can't overflow the stack.
    depth: usize,
    /// The function is past the block budget: its check is skipped.
    too_big: bool,
}

const MAX_BLOCKS: usize = 4000;

impl<'e, 'a> Lower<'e, 'a> {
    fn new(env: &'e Env<'a>, name: &str) -> Self {
        let mut body = Body {
            name: name.to_string(),
            ..Default::default()
        };
        let static_ = body.new_universal("'static");
        debug_assert_eq!(static_, Region(0));
        let unit = body.new_local("()", RTy::opaque(), LocalKind::Temp, (0, 0));
        let ret_local = body.new_local("return", RTy::opaque(), LocalKind::Return, (0, 0));
        let entry = body.new_block();
        Lower {
            env,
            body,
            cur: entry,
            reachable: true,
            scopes: vec![Scope {
                names: HashMap::new(),
                locals: Vec::new(),
            }],
            loops: Vec::new(),
            temps: vec![Vec::new()],
            ret_local,
            ret_rty: RTy::opaque(),
            unit,
            base: 0,
            upvars: HashMap::new(),
            tenv: TyEnv::default(),
            self_adt: None,
            closures: Vec::new(),
            pending_hints: Vec::new(),
            imported: HashMap::new(),
            depth: 0,
            too_big: false,
        }
    }

    // ------------------------------------------------------------ plumbing

    fn span(&self, n: &SyntaxNode) -> Span {
        let r = n.text_range();
        (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        )
    }

    fn ty_at(&self, n: &SyntaxNode) -> Option<&'e Ty> {
        self.env.ty_at(self.base, n)
    }

    fn rty_at(&mut self, n: &SyntaxNode) -> RTy {
        match self.ty_at(n).cloned() {
            Some(t) => self.env.rty_of_ty(&mut self.body, &t),
            None => RTy::opaque(),
        }
    }

    fn fresh(&mut self) -> Region {
        self.body.new_region()
    }

    fn emit(&mut self, kind: StmtKind, span: Span) {
        if self.reachable {
            self.body.blocks[self.cur.idx()].stmts.push(Stmt { kind, span });
        }
    }

    fn terminate(&mut self, term: Term, span: Span) {
        if self.reachable {
            let b = &mut self.body.blocks[self.cur.idx()];
            b.term = term;
            b.term_span = span;
            self.reachable = false;
        }
    }

    fn new_block(&mut self) -> Block {
        if self.body.blocks.len() > MAX_BLOCKS {
            self.too_big = true;
        }
        self.body.new_block()
    }

    /// Continue in `b` (reachable).
    fn switch_to(&mut self, b: Block) {
        self.cur = b;
        self.reachable = true;
    }

    fn goto(&mut self, b: Block, span: Span) {
        self.terminate(Term::Goto(b), span);
    }

    fn temp(&mut self, rty: RTy, span: Span) -> Local {
        let drops = self.drops(&rty);
        let l = self.body.new_local("", rty, LocalKind::Temp, span);
        self.body.locals[l.idx()].drops = drops;
        self.temps.last_mut().unwrap().push(l);
        l
    }

    /// A temporary that lives to the end of the enclosing block (`let r = &temp();`).
    fn extended_temp(&mut self, rty: RTy, span: Span) -> Local {
        let drops = self.drops(&rty);
        let l = self.body.new_local("", rty, LocalKind::Temp, span);
        self.body.locals[l.idx()].drops = drops;
        self.scopes.last_mut().unwrap().locals.push(l);
        l
    }

    fn drops(&self, rty: &RTy) -> bool {
        match rty {
            RTy::Adt(n, args, _) => {
                self.env
                    .adts
                    .defs
                    .get(n)
                    .is_some_and(|d| d.implements("Drop"))
                    || args.iter().any(|a| self.drops(a))
            }
            RTy::Vec(e) | RTy::Option(e) | RTy::Set(e) | RTy::Cell(e) => self.drops(e),
            RTy::Tuple(es) => es.iter().any(|e| self.drops(e)),
            RTy::Result(a, b) | RTy::Map(a, b) => self.drops(a) || self.drops(b),
            _ => false,
        }
    }

    fn declare(&mut self, name: &str, rty: RTy, span: Span) -> Local {
        let drops = self.drops(&rty);
        let l = self.body.new_local(name, rty, LocalKind::User, span);
        self.body.locals[l.idx()].drops = drops;
        let s = self.scopes.last_mut().unwrap();
        s.names.insert(name.to_string(), l);
        s.locals.push(l);
        l
    }

    fn lookup(&self, name: &str) -> Option<Local> {
        self.scopes.iter().rev().find_map(|s| s.names.get(name).copied())
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope {
            names: HashMap::new(),
            locals: Vec::new(),
        });
    }

    fn pop_scope(&mut self, span: Span) {
        let s = self.scopes.pop().unwrap();
        for l in s.locals.into_iter().rev() {
            self.emit(StmtKind::StorageDead(l), span);
        }
    }

    /// `StorageDead` for every local of the scopes above `depth` (leaving them by `break`,
    /// `continue` or `return`), without popping.
    fn unwind_to(&mut self, depth: usize, span: Span) {
        for i in (depth..self.scopes.len()).rev() {
            let locals = self.scopes[i].locals.clone();
            for l in locals.into_iter().rev() {
                self.emit(StmtKind::StorageDead(l), span);
            }
        }
    }

    fn with_temps<R>(&mut self, span: Span, f: impl FnOnce(&mut Self) -> R) -> R {
        self.temps.push(Vec::new());
        let r = f(self);
        let ts = self.temps.pop().unwrap();
        for t in ts.into_iter().rev() {
            self.emit(StmtKind::StorageDead(t), span);
        }
        r
    }

    fn unit_ex(&self, span: Span) -> Ex {
        Ex {
            place: Place::local(self.unit),
            rty: RTy::opaque(),
            span,
            is_place: false,
        }
    }

    fn const_ex(&self, rty: RTy, span: Span) -> Ex {
        Ex {
            place: Place::local(self.unit),
            rty,
            span,
            is_place: false,
        }
    }

    /// The type of a place. (A field type with an elided lifetime would need a fresh region;
    /// accepted Rust has none, so the lookup runs on a scratch body.)
    fn place_rty(&mut self, p: &Place) -> RTy {
        let env = self.env;
        let fields = |v: &str, f: &str, adt: &RTy| -> Option<RTy> {
            let mut scratch = Body::default();
            env.field_rty(&mut scratch, v, f, adt)
        };
        self.body.place_rty(p, &fields)
    }

    /// The field type of an ADT instance, allocating regions in the real body (for a field
    /// whose written type has an elided lifetime).
    fn field_rty(&mut self, variant: &str, f: &str, adt: &RTy) -> RTy {
        self.env
            .field_rty(&mut self.body, variant, f, adt)
            .unwrap_or(RTy::opaque())
    }

    /// An operand reading `ex`: a move when its type is owned and not `Copy`; a `&mut` place
    /// is reborrowed (as rustc does at every coercion site) rather than moved.
    fn operand(&mut self, ex: &Ex) -> Operand {
        if ex.is_place
            && let RTy::Ref(_, true, inner) = &ex.rty
        {
            let r = self.fresh();
            let inner = (**inner).clone();
            let place = ex.place.clone().deref();
            let t = self.borrow(r, true, place, inner, ex.span, false);
            return Operand {
                place: Place::local(t),
                moves: false,
                span: ex.span,
            };
        }
        let moves = self.env.moves(&ex.rty) && !(ex.is_place && ex.place.has_deref());
        Operand {
            place: ex.place.clone(),
            moves,
            span: ex.span,
        }
    }

    /// `t = &place` / `&mut place` into a fresh temporary of type `&'r T`, with the reborrow
    /// constraints of every reference the place goes through.
    fn borrow(
        &mut self,
        region: Region,
        mutable: bool,
        place: Place,
        inner: RTy,
        span: Span,
        two_phase: bool,
    ) -> Local {
        self.reborrow_constraints(&place, region, span);
        let t = self.temp(RTy::Ref(region, mutable, Box::new(inner)), span);
        self.emit(
            StmtKind::Assign(
                Place::local(t),
                Rvalue::Ref {
                    region,
                    mutable,
                    place,
                    two_phase,
                },
            ),
            span,
        );
        t
    }

    /// Borrowing `place` for `region` reborrows the references it derefs: walking in from the
    /// place, each dereferenced reference's region outlives `region`, up to the first shared
    /// one (its referent can't change, so what it was reached through doesn't matter: `&'a`
    /// data behind a `&mut self` can be handed out for `'a`).
    fn reborrow_constraints(&mut self, place: &Place, region: Region, span: Span) {
        let mut t = self.body.locals[place.local.idx()].rty.clone();
        let mut variant: Option<String> = None;
        // The references dereferenced, outermost first: (region, mutable).
        let mut derefs: Vec<(Region, bool)> = Vec::new();
        for p in &place.proj {
            match p {
                Proj::Deref => {
                    match t {
                        RTy::Ref(r, m, inner) => {
                            derefs.push((r, m));
                            t = *inner;
                        }
                        RTy::Cell(inner) => t = *inner,
                        _ => break,
                    }
                    variant = None;
                }
                Proj::Index => {
                    t = match t {
                        RTy::Vec(e) => *e,
                        _ => break,
                    };
                }
                Proj::Downcast(v) => variant = Some(v.clone()),
                Proj::Field(f) => {
                    t = match &t {
                        RTy::Tuple(es) => match f.parse::<usize>().ok().and_then(|i| es.get(i)) {
                            Some(x) => x.clone(),
                            None => break,
                        },
                        RTy::Option(inner) => (**inner).clone(),
                        RTy::Result(a, b) => match variant.as_deref() {
                            Some("Err") => (**b).clone(),
                            _ => (**a).clone(),
                        },
                        RTy::Adt(..) => {
                            let v = variant.take().unwrap_or_default();
                            match self.env.field_rty(&mut self.body, &v, f, &t) {
                                Some(x) => x,
                                None => break,
                            }
                        }
                        _ => break,
                    };
                    variant = None;
                }
            }
        }
        for (r, m) in derefs.into_iter().rev() {
            self.body.outlives(r, region, span);
            if !m {
                break;
            }
        }
    }

    /// `dest = rvalue` into a fresh temporary.
    fn assign_temp(&mut self, rty: RTy, rv: Rvalue, span: Span, ctx: Ctx) -> Ex {
        let t = if ctx.extend {
            self.extended_temp(rty.clone(), span)
        } else {
            self.temp(rty.clone(), span)
        };
        self.emit(StmtKind::Assign(Place::local(t), rv), span);
        Ex {
            place: Place::local(t),
            rty,
            span,
            is_place: false,
        }
    }

    /// An owned value computed from `args`.
    fn ops(&mut self, rty: RTy, args: Vec<Operand>, span: Span, ctx: Ctx) -> Ex {
        self.assign_temp(
            rty,
            Rvalue::Ops {
                args,
                activate: Vec::new(),
            },
            span,
            ctx,
        )
    }

    /// Copy `ex` into a fresh temporary of the same type (so a block's tail outlives its
    /// locals).
    fn materialize(&mut self, ex: Ex, ctx: Ctx) -> Ex {
        if !ex.is_place {
            return ex;
        }
        let op = self.operand(&ex);
        self.assign_temp(ex.rty.clone(), Rvalue::Use(op), ex.span, ctx)
    }

    // ------------------------------------------------------------ functions

    /// Lower a function or method of this file: its body and the bodies of its closures.
    pub fn function(env: &'e Env<'a>, fs: &FnSyn) -> Option<Vec<Body>> {
        let f = &fs.f;
        let name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
        let block = f.body()?;
        let mut lw = Lower::new(env, &name);
        lw.self_adt = fs.imp.as_ref().and_then(|i| {
            i.self_ty
                .as_ref()
                .and_then(|t| match t {
                    ast::Type::PathType(p) => p.path()?.segment()?.name_ref().map(|n| n.text().to_string()),
                    _ => None,
                })
        });
        let span = lw.span(f.syntax());
        let self_adt = lw.self_adt.clone().map(|n| {
            let def = env.adt_syn.get(&n);
            let args = def
                .map(|d| d.type_params.iter().map(|_| RTy::Opaque(true)).collect())
                .unwrap_or_default();
            RTy::Adt(n, args, Vec::new())
        });
        let sig = env.signature(&mut lw.body, fs, self_adt.as_ref(), true, span);
        // The names the signature's lifetimes got, for `let x: &'a T` inside the body.
        {
            let (lts, tps) = generics_of(f.generic_param_list());
            let mut names: Vec<String> = fs.imp.as_ref().map(|i| i.lifetimes.clone()).unwrap_or_default();
            names.extend(lts);
            for r in 0..lw.body.regions.len() {
                if let Some(u) = &lw.body.regions[r].universal
                    && names.contains(&u.name)
                {
                    lw.tenv.lifetimes.insert(u.name.clone(), Region(r as u32));
                }
            }
            for p in fs.imp.iter().flat_map(|i| i.type_params.iter()).chain(tps.iter()) {
                lw.tenv.params.insert(p.clone(), RTy::Opaque(true));
            }
            lw.tenv.self_ty = match &sig.self_ty {
                Some(t) => Some(t.strip_refs().clone()),
                None => self_adt.clone(),
            };
        }
        lw.ret_rty = sig.ret.clone();
        lw.body.locals[lw.ret_local.idx()].rty = sig.ret.clone();
        // Parameters.
        if let Some(list) = f.param_list() {
            if let (Some(sp), Some(t)) = (list.self_param(), &sig.self_ty) {
                let span = lw.span(sp.syntax());
                lw.declare("self", t.clone(), span);
            }
            for (i, p) in list.params().enumerate() {
                let rty = sig.params.get(i).cloned().unwrap_or(RTy::opaque());
                let Some(pat) = p.pat() else { continue };
                let span = lw.span(pat.syntax());
                match &pat {
                    ast::Pat::IdentPat(ip) if ip.at_token().is_none() => {
                        let name = ip.name().map(|n| n.text().to_string()).unwrap_or_default();
                        lw.declare(&name, rty, span);
                    }
                    _ => {
                        let l = lw.declare("", rty.clone(), span);
                        lw.pattern(&pat, Place::local(l), rty, None, span);
                    }
                }
            }
        }
        let direct = sig.ret_direct_ref;
        lw.fn_body(&block, direct);
        if lw.too_big {
            return None;
        }
        let mut out = vec![lw.body];
        out.extend(lw.closures);
        Some(out)
    }

    fn fn_body(&mut self, block: &ast::BlockExpr, direct_ref: bool) {
        let ex = self.block_inner(block, Ctx::default());
        if self.reachable {
            // (Reported at the tail expression, where rustc points.)
            let span = ex.span;
            self.ret_value(ex, span, direct_ref);
        }
    }

    fn ret_value(&mut self, ex: Ex, span: Span, direct_ref: bool) {
        let op = self.operand(&ex);
        let ret = self.ret_rty.clone();
        self.env.relate(&mut self.body, &ex.rty, &ret, span);
        self.emit(
            StmtKind::Assign(Place::local(self.ret_local), Rvalue::Use(op)),
            span,
        );
        self.body.returns.push(ReturnSite {
            span,
            direct_ref: direct_ref && ex.rty.is_ref(),
        });
        self.terminate(Term::Return, span);
    }

    // ------------------------------------------------------------ blocks and statements

    fn block(&mut self, b: &ast::BlockExpr, ctx: Ctx) -> Ex {
        let span = self.span(b.syntax());
        // `'a: { .. break 'a v; .. }`
        let label = b.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
        if label.is_some() {
            let rty = self.rty_at(b.syntax());
            let result = self.temp(rty, span);
            let brk = self.new_block();
            self.loops.push(LoopCtx {
                label,
                brk,
                cont: None,
                scope_depth: self.scopes.len(),
                result: Some(result),
            });
            let ex = self.block_inner(b, ctx);
            self.loops.pop();
            if self.reachable {
                let op = self.operand(&ex);
                self.emit(StmtKind::Assign(Place::local(result), Rvalue::Use(op)), span);
                self.goto(brk, span);
            }
            self.switch_to(brk);
            let rty = self.body.locals[result.idx()].rty.clone();
            return Ex {
                place: Place::local(result),
                rty,
                span,
                is_place: false,
            };
        }
        if b.async_token().is_some() {
            return self.async_block(b, ctx);
        }
        self.block_inner(b, ctx)
    }

    fn block_inner(&mut self, b: &ast::BlockExpr, ctx: Ctx) -> Ex {
        let span = self.span(b.syntax());
        let Some(list) = b.stmt_list() else {
            return self.unit_ex(span);
        };
        self.push_scope();
        for s in list.statements() {
            if self.too_big {
                break;
            }
            self.stmt(&s);
        }
        let ex = match list.tail_expr() {
            Some(t) => {
                let ex = self.expr(&t, ctx);
                self.materialize(ex, ctx)
            }
            None => self.unit_ex(span),
        };
        self.pop_scope(span);
        ex
    }

    fn stmt(&mut self, s: &ast::Stmt) {
        let span = self.span(s.syntax());
        match s {
            ast::Stmt::LetStmt(l) => self.with_temps(span, |lw| lw.let_stmt(l)),
            ast::Stmt::ExprStmt(es) => {
                if let Some(e) = es.expr() {
                    self.with_temps(span, |lw| {
                        lw.expr(&e, Ctx::default());
                    });
                }
            }
            ast::Stmt::Item(_) => {}
        }
    }

    fn let_stmt(&mut self, l: &ast::LetStmt) {
        let span = self.span(l.syntax());
        let Some(pat) = l.pat() else { return };
        let declared = l.ty().map(|t| {
            let tenv = self.tenv.clone();
            self.env
                .rty_from_syntax(&mut self.body, &tenv, &t, &mut |b| b.new_region())
        });
        let init = l.initializer().map(|e| self.expr(&e, Ctx { extend: true }));
        if let (Some(init), Some(d)) = (&init, &declared) {
            self.env.relate(&mut self.body, &init.rty, d, span);
        }
        let rty = match (&declared, &init) {
            (Some(d), _) => d.clone(),
            (None, Some(i)) => i.rty.clone(),
            (None, None) => self.rty_at(pat.syntax()),
        };
        // `let x = ..;`: the common case, one local.
        if let ast::Pat::IdentPat(ip) = &pat
            && ip.at_token().is_none()
            && ip.ref_token().is_none()
            && l.let_else().is_none()
        {
            let name = ip.name().map(|n| n.text().to_string()).unwrap_or_default();
            let op = init.as_ref().map(|i| self.operand(i));
            let nspan = ip.name().map(|n| self.span(n.syntax())).unwrap_or(span);
            let local = self.declare(&name, rty, nspan);
            if let Some(op) = op {
                self.emit(StmtKind::Assign(Place::local(local), Rvalue::Use(op)), span);
            }
            return;
        }
        let Some(init) = init else {
            // `let (a, b);`: the names, with nothing to bind to yet.
            for ip in pat.syntax().descendants().filter_map(ast::IdentPat::cast) {
                if let Some(n) = ip.name() {
                    let t = self.rty_at(n.syntax());
                    let s = self.span(n.syntax());
                    self.declare(n.text(), t, s);
                }
            }
            return;
        };
        if let Some(le) = l.let_else() {
            let scrut = self.materialize(init, Ctx { extend: true });
            let bind = self.new_block();
            let els = self.new_block();
            let op = Operand {
                place: scrut.place.clone(),
                moves: false,
                span,
            };
            self.terminate(Term::Switch(Some(op), vec![bind, els]), span);
            self.switch_to(els);
            if let Some(b) = le.block_expr() {
                self.block(&b, Ctx::default());
            }
            self.terminate(Term::Unreachable, span);
            self.switch_to(bind);
            self.pattern(&pat, scrut.place, scrut.rty, None, span);
            return;
        }
        self.pattern(&pat, init.place, init.rty, None, span);
    }

    // ------------------------------------------------------------ patterns

    /// Bind the names of `p` against `place` (of type `rty`). `mode`: the default binding mode
    /// (`Some(mutable)` once a reference was matched through).
    fn pattern(&mut self, p: &ast::Pat, place: Place, rty: RTy, mode: Option<bool>, span: Span) {
        let (place, rty, mode) = match p {
            // A `&x` pattern consumes a reference layer; everything else matches through one.
            ast::Pat::RefPat(_) => (place, rty, mode),
            ast::Pat::IdentPat(_) | ast::Pat::WildcardPat(_) | ast::Pat::RestPat(_) => {
                (place, rty, mode)
            }
            _ => self.auto_deref(place, rty, mode),
        };
        match p {
            ast::Pat::IdentPat(ip) => {
                let Some(name) = ip.name() else { return };
                if is_const_pattern(ip) {
                    return;
                }
                let nspan = self.span(name.syntax());
                let by_ref = match (ip.ref_token().is_some(), mode) {
                    (true, _) => Some(ip.mut_token().is_some()),
                    (false, m) => m,
                };
                let ex = Ex {
                    place: place.clone(),
                    rty: rty.clone(),
                    span: nspan,
                    is_place: true,
                };
                self.bind_simple(name.text(), ex, by_ref, nspan);
                if let Some(sub) = ip.pat() {
                    self.pattern(&sub, place, rty, mode, span);
                }
            }
            ast::Pat::RefPat(rp) => {
                if let Some(sub) = rp.pat() {
                    match rty {
                        RTy::Ref(_, _, inner) => {
                            self.pattern(&sub, place.deref(), *inner, None, span)
                        }
                        other => self.pattern(&sub, place, other, mode, span),
                    }
                }
            }
            ast::Pat::ParenPat(pp) => {
                if let Some(sub) = pp.pat() {
                    self.pattern(&sub, place, rty, mode, span);
                }
            }
            ast::Pat::TuplePat(tp) => {
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let es = match &rty {
                    RTy::Tuple(es) => es.clone(),
                    _ => Vec::new(),
                };
                self.positional(&fields, place, &es, mode, span);
            }
            ast::Pat::TupleStructPat(tp) => {
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let variant = tp
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.name_ref())
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();
                let (place, es) = self.variant_fields(place, &rty, &variant, fields.len());
                self.positional(&fields, place, &es, mode, span);
            }
            ast::Pat::RecordPat(rp) => {
                let variant = rp
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.name_ref())
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();
                let place = match &rty {
                    RTy::Adt(n, ..) if *n != variant => place.downcast(variant.clone()),
                    _ => place,
                };
                let vname = match &rty {
                    RTy::Adt(n, ..) if *n == variant => String::new(),
                    _ => variant.clone(),
                };
                for f in rp
                    .record_pat_field_list()
                    .into_iter()
                    .flat_map(|l| l.fields())
                {
                    let Some(name) = f.field_name() else { continue };
                    let fname = name.to_string();
                    let frty = match &rty {
                        RTy::Adt(..) => self.field_rty(&vname, &fname, &rty),
                        _ => RTy::opaque(),
                    };
                    let fplace = place.clone().field(fname.clone());
                    match f.pat() {
                        Some(sub) => self.pattern(&sub, fplace, frty, mode, span),
                        // `S { name, .. }`: a shorthand binding.
                        None => {
                            let nspan = self.span(f.syntax());
                            let ip = ast::IdentPat::cast(f.syntax().first_child().unwrap_or_else(|| f.syntax().clone()));
                            match ip {
                                Some(ip) => self.pattern(&ast::Pat::IdentPat(ip), fplace, frty, mode, span),
                                None => {
                                    let ex = Ex {
                                        place: fplace.clone(),
                                        rty: frty.clone(),
                                        span: nspan,
                                        is_place: true,
                                    };
                                    self.bind_simple(&fname, ex, mode, nspan);
                                }
                            }
                        }
                    }
                }
            }
            ast::Pat::SlicePat(sp) => {
                let elem = match &rty {
                    RTy::Vec(e) => (**e).clone(),
                    _ => RTy::opaque(),
                };
                for sub in sp.pats() {
                    match &sub {
                        ast::Pat::RestPat(_) => {}
                        ast::Pat::IdentPat(ip)
                            if ip.pat().is_some_and(|x| matches!(x, ast::Pat::RestPat(_))) =>
                        {
                            // `rest @ ..`: the rest of the slice.
                            let Some(name) = ip.name() else { continue };
                            let nspan = self.span(name.syntax());
                            let ex = Ex {
                                place: place.clone(),
                                rty: RTy::Vec(Box::new(elem.clone())),
                                span: nspan,
                                is_place: true,
                            };
                            let m = mode.unwrap_or(false);
                            self.bind_simple(name.text(), ex, Some(m), nspan);
                        }
                        _ => self.pattern(&sub, place.clone().index(), elem.clone(), mode, span),
                    }
                }
            }
            ast::Pat::OrPat(op) => {
                // (Alternatives bind the same names: the first one's bindings stand for all.)
                if let Some(first) = op.pats().next() {
                    self.pattern(&first, place, rty, mode, span);
                }
            }
            ast::Pat::BoxPat(bp) => {
                if let Some(sub) = bp.pat() {
                    self.pattern(&sub, place, rty, mode, span);
                }
            }
            ast::Pat::WildcardPat(_)
            | ast::Pat::RestPat(_)
            | ast::Pat::LiteralPat(_)
            | ast::Pat::RangePat(_)
            | ast::Pat::PathPat(_)
            | ast::Pat::MacroPat(_)
            | ast::Pat::ConstBlockPat(_) => {}
            _ => {}
        }
    }

    /// A reference matched by a non-reference pattern: match through it, binding by reference
    /// from here on (rustc's default binding modes).
    fn auto_deref(&mut self, mut place: Place, mut rty: RTy, mut mode: Option<bool>) -> (Place, RTy, Option<bool>) {
        while let RTy::Ref(_, m, inner) = rty {
            place = place.deref();
            rty = *inner;
            mode = Some(match mode {
                Some(false) => false,
                _ => m,
            });
        }
        (place, rty, mode)
    }

    fn bind_simple(&mut self, name: &str, ex: Ex, mode: Option<bool>, span: Span) {
        match mode {
            Some(m) => {
                let r = self.fresh();
                self.reborrow_constraints(&ex.place, r, span);
                let local = self.declare(name, RTy::Ref(r, m, Box::new(ex.rty.clone())), span);
                self.emit(
                    StmtKind::Assign(
                        Place::local(local),
                        Rvalue::Ref {
                            region: r,
                            mutable: m,
                            place: ex.place,
                            two_phase: false,
                        },
                    ),
                    span,
                );
            }
            None => {
                let op = self.operand(&ex);
                let local = self.declare(name, ex.rty, span);
                self.emit(StmtKind::Assign(Place::local(local), Rvalue::Use(op)), span);
            }
        }
    }

    fn positional(&mut self, fields: &[ast::Pat], place: Place, es: &[RTy], mode: Option<bool>, span: Span) {
        let rest = fields.iter().position(|f| matches!(f, ast::Pat::RestPat(_)));
        for (i, f) in fields.iter().enumerate() {
            if matches!(f, ast::Pat::RestPat(_)) {
                continue;
            }
            let idx = match rest {
                Some(r) if i > r => {
                    // After `..`: counted from the end.
                    let from_end = fields.len() - i;
                    if es.len() >= from_end { es.len() - from_end } else { continue }
                }
                _ => i,
            };
            let frty = es.get(idx).cloned().unwrap_or(RTy::opaque());
            self.pattern(f, place.clone().field(idx.to_string()), frty, mode, span);
        }
    }

    /// The place and field types of a variant pattern (`Some(x)`, `Ok(v)`, `Shape::Circle(r)`,
    /// `Point(a, b)`).
    fn variant_fields(&mut self, place: Place, rty: &RTy, variant: &str, n: usize) -> (Place, Vec<RTy>) {
        match rty {
            RTy::Option(inner) => (place.downcast("Some"), vec![(**inner).clone()]),
            RTy::Result(a, b) => {
                let t = if variant == "Err" { (**b).clone() } else { (**a).clone() };
                (place.downcast(variant.to_string()), vec![t])
            }
            RTy::Adt(name, ..) => {
                let is_struct = name == variant;
                let vname = if is_struct { String::new() } else { variant.to_string() };
                let place = if is_struct { place } else { place.downcast(variant.to_string()) };
                let fields: Vec<RTy> = (0..n)
                    .map(|i| self.field_rty(&vname, &i.to_string(), rty))
                    .collect();
                (place, fields)
            }
            _ => (place, Vec::new()),
        }
    }

    // ------------------------------------------------------------ expressions

    fn expr(&mut self, e: &ast::Expr, ctx: Ctx) -> Ex {
        self.depth += 1;
        if self.depth > 400 {
            self.too_big = true;
        }
        let ex = if self.too_big {
            self.unit_ex(self.span(e.syntax()))
        } else {
            self.expr_inner(e, ctx)
        };
        self.depth -= 1;
        ex
    }

    fn expr_inner(&mut self, e: &ast::Expr, ctx: Ctx) -> Ex {
        let span = self.span(e.syntax());
        match e {
            ast::Expr::Literal(l) => {
                let rty = match l.kind() {
                    ast::LiteralKind::String(_) | ast::LiteralKind::ByteString(_) => {
                        RTy::Ref(Region(0), false, Box::new(RTy::Str))
                    }
                    _ => RTy::opaque(),
                };
                self.const_ex(rty, span)
            }
            ast::Expr::ParenExpr(p) => match p.expr() {
                Some(inner) => self.expr(&inner, ctx),
                None => self.unit_ex(span),
            },
            ast::Expr::PathExpr(p) => self.path_expr(p, span),
            ast::Expr::RefExpr(r) => self.ref_expr(r, ctx, span),
            ast::Expr::FieldExpr(f) => self.field_expr(f, ctx, span),
            ast::Expr::IndexExpr(ix) => self.index_expr(ix, false, ctx, span),
            ast::Expr::PrefixExpr(p) => self.prefix_expr(p, ctx, span),
            ast::Expr::BinExpr(b) => self.bin_expr(b, ctx, span),
            ast::Expr::CallExpr(c) => self.call_expr(c, ctx, span),
            ast::Expr::MethodCallExpr(m) => self.method_call(m, ctx, span),
            ast::Expr::ClosureExpr(c) => self.closure_expr(c, ctx, span),
            ast::Expr::IfExpr(i) => self.if_expr(i, ctx, span),
            ast::Expr::MatchExpr(m) => self.match_expr(m, ctx, span),
            ast::Expr::BlockExpr(b) => self.block(b, ctx),
            ast::Expr::LoopExpr(l) => self.loop_expr(l, span),
            ast::Expr::WhileExpr(w) => self.while_expr(w, span),
            ast::Expr::ForExpr(f) => self.for_expr(f, span),
            ast::Expr::BreakExpr(b) => self.break_expr(b, span),
            ast::Expr::ContinueExpr(c) => self.continue_expr(c, span),
            ast::Expr::ReturnExpr(r) => self.return_expr(r, span),
            ast::Expr::TryExpr(t) => self.try_expr(t, ctx, span),
            ast::Expr::AwaitExpr(a) => {
                let inner = match a.expr() {
                    Some(i) => self.expr(&i, Ctx::default()),
                    None => return self.unit_ex(span),
                };
                let op = self.operand(&inner);
                let rty = inner.rty.clone();
                self.ops(rty, vec![op], span, ctx)
            }
            ast::Expr::CastExpr(c) => {
                let inner = match c.expr() {
                    Some(i) => self.expr(&i, Ctx::default()),
                    None => return self.unit_ex(span),
                };
                let op = Operand {
                    place: inner.place.clone(),
                    moves: false,
                    span: inner.span,
                };
                self.ops(RTy::opaque(), vec![op], span, ctx)
            }
            ast::Expr::RangeExpr(r) => {
                let mut args = Vec::new();
                for side in [r.start(), r.end()].into_iter().flatten() {
                    let ex = self.expr(&side, Ctx::default());
                    args.push(self.operand(&ex));
                }
                self.ops(RTy::opaque(), args, span, ctx)
            }
            ast::Expr::TupleExpr(t) if is_constant(e) => {
                let rty = RTy::Tuple(t.fields().map(|_| RTy::opaque()).collect());
                self.const_ex(rty, span)
            }
            ast::Expr::ArrayExpr(_) if is_constant(e) => {
                self.const_ex(RTy::Vec(Box::new(RTy::opaque())), span)
            }
            ast::Expr::TupleExpr(t) => {
                let mut args = Vec::new();
                let mut tys = Vec::new();
                for f in t.fields() {
                    let ex = self.expr(&f, ctx);
                    args.push(self.operand(&ex));
                    tys.push(ex.rty);
                }
                if tys.is_empty() {
                    return self.unit_ex(span);
                }
                self.ops(RTy::Tuple(tys), args, span, ctx)
            }
            ast::Expr::ArrayExpr(a) => self.array_expr(a, ctx, span),
            ast::Expr::RecordExpr(r) => self.record_expr(r, ctx, span),
            ast::Expr::MacroExpr(m) => match m.macro_call() {
                Some(mc) => self.macro_call(&mc, ctx, span),
                None => self.unit_ex(span),
            },
            ast::Expr::LetExpr(_) => {
                // (Only valid in a condition, handled by `cond`.)
                self.unit_ex(span)
            }
            ast::Expr::UnderscoreExpr(_) => self.unit_ex(span),
            ast::Expr::FormatArgsExpr(_)
            | ast::Expr::AsmExpr(_)
            | ast::Expr::OffsetOfExpr(_)
            | ast::Expr::YieldExpr(_)
            | ast::Expr::YeetExpr(_)
            | ast::Expr::BecomeExpr(_) => self.unit_ex(span),
            _ => self.unit_ex(span),
        }
    }

    fn path_expr(&mut self, p: &ast::PathExpr, span: Span) -> Ex {
        let Some(path) = p.path() else {
            return self.unit_ex(span);
        };
        if let Some(name) = path.as_single_name_ref() {
            let name = name.text().to_string();
            if let Some(l) = self.lookup(&name) {
                let rty = self.body.locals[l.idx()].rty.clone();
                return Ex {
                    place: Place::local(l),
                    rty,
                    span,
                    is_place: true,
                };
            }
            if let Some(up) = self.upvars.get(&name).cloned() {
                let (place, rty) = match up.by_ref {
                    Some(_) => {
                        let rty = match &self.body.locals[up.local.idx()].rty {
                            RTy::Ref(_, _, inner) => (**inner).clone(),
                            other => other.clone(),
                        };
                        (Place::local(up.local).deref(), rty)
                    }
                    None => (
                        Place::local(up.local),
                        self.body.locals[up.local.idx()].rty.clone(),
                    ),
                };
                return Ex {
                    place,
                    rty,
                    span,
                    is_place: true,
                };
            }
            if name == "self"
                && let Some(l) = self.lookup("self")
            {
                let rty = self.body.locals[l.idx()].rty.clone();
                return Ex {
                    place: Place::local(l),
                    rty,
                    span,
                    is_place: true,
                };
            }
        }
        // A constant, static, unit struct, unit variant or function item.
        let rty = self.rty_at(p.syntax());
        self.const_ex(rty, span)
    }

    /// `e` as a place, if it is a place expression (else a temporary holding it).
    fn place_of(&mut self, e: &ast::Expr, mutable: bool, ctx: Ctx) -> Ex {
        match e {
            ast::Expr::ParenExpr(p) => match p.expr() {
                Some(inner) => self.place_of(&inner, mutable, ctx),
                None => self.unit_ex(self.span(e.syntax())),
            },
            ast::Expr::IndexExpr(ix) => {
                let span = self.span(e.syntax());
                self.index_expr(ix, mutable, ctx, span)
            }
            _ => self.expr(e, ctx),
        }
    }

    fn ref_expr(&mut self, r: &ast::RefExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(inner) = r.expr() else {
            return self.unit_ex(span);
        };
        let mutable = r.mut_token().is_some();
        if r.raw_token().is_some() {
            // (`&raw const place`: a pointer, which borrows nothing.)
            self.place_of(&inner, mutable, ctx);
            return self.const_ex(RTy::opaque(), span);
        }
        let ex = self.place_of(&inner, mutable, ctx);
        // (`&expr` of a value: the temporary is extended in a `let`.)
        let region = self.fresh();
        let t = self.borrow(region, mutable, ex.place.clone(), ex.rty.clone(), span, false);
        if ctx.extend && !ex.is_place {
            // Both the temporary and the reference live to the end of the block.
            self.extend_temp(ex.place.local);
            self.extend_temp(t);
        }
        Ex {
            place: Place::local(t),
            rty: RTy::Ref(region, mutable, Box::new(ex.rty)),
            span,
            is_place: false,
        }
    }

    /// Move a temporary from the statement's scope to the block's.
    fn extend_temp(&mut self, t: Local) {
        if self.body.locals[t.idx()].kind != LocalKind::Temp {
            return;
        }
        let Some(ts) = self.temps.last_mut() else { return };
        if let Some(i) = ts.iter().position(|x| *x == t) {
            ts.remove(i);
            self.scopes.last_mut().unwrap().locals.push(t);
        }
    }

    fn field_expr(&mut self, f: &ast::FieldExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(base) = f.expr() else {
            return self.unit_ex(span);
        };
        let name = f.name_ref().map(|n| n.text().to_string()).unwrap_or_default();
        let ex = self.place_of(&base, false, ctx);
        let (place, rty, _) = self.auto_deref(ex.place, ex.rty, None);
        let frty = match &rty {
            RTy::Tuple(es) => name
                .parse::<usize>()
                .ok()
                .and_then(|i| es.get(i).cloned())
                .unwrap_or(RTy::opaque()),
            RTy::Adt(..) => self.field_rty("", &name, &rty),
            _ => {
                // (A struct from another module, or unknown: the erased type.)
                self.rty_at(f.syntax())
            }
        };
        Ex {
            place: place.field(name),
            rty: frty,
            span,
            is_place: true,
        }
    }

    /// `base[i]`: on a `Vec`, map, string or `Index` impl, a call of `index` / `index_mut`
    /// (auto-ref'd, two-phase for `&mut`) whose result is dereferenced.
    fn index_expr(&mut self, ix: &ast::IndexExpr, mutable: bool, ctx: Ctx, span: Span) -> Ex {
        let (Some(base), Some(idx)) = (ix.base(), ix.index()) else {
            return self.unit_ex(span);
        };
        let ex = self.place_of(&base, mutable, ctx);
        let (place, rty, _) = self.auto_deref(ex.place, ex.rty, None);
        let is_range = matches!(
            self.ty_at(idx.syntax()),
            Some(Ty::Range(..)) | Some(Ty::Unknown)
        ) || matches!(idx, ast::Expr::RangeExpr(_));
        let elem = match &rty {
            RTy::Vec(e) if is_range => RTy::Vec(e.clone()),
            RTy::Vec(e) => (**e).clone(),
            RTy::Str => RTy::Str,
            RTy::String => RTy::Str,
            RTy::Map(_, v) => (**v).clone(),
            _ => self.rty_at(ix.syntax()),
        };
        let region = self.fresh();
        // (`Index::index(&v, i)`: rustc puts the borrow on `v`.)
        let base_span = self.span(base.syntax());
        let recv = self.borrow(region, mutable, place.clone(), rty, base_span, mutable);
        let i = self.expr(&idx, Ctx::default());
        let iop = self.operand(&i);
        let out = self.temp(RTy::Ref(region, mutable, Box::new(elem.clone())), span);
        self.emit(
            StmtKind::Assign(
                Place::local(out),
                Rvalue::Ops {
                    args: vec![
                        Operand {
                            place: Place::local(recv),
                            moves: false,
                            span,
                        },
                        iop,
                    ],
                    activate: if mutable { vec![(place, region)] } else { Vec::new() },
                },
            ),
            span,
        );
        Ex {
            place: Place::local(out).deref(),
            rty: elem,
            span,
            is_place: true,
        }
    }

    fn prefix_expr(&mut self, p: &ast::PrefixExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(inner) = p.expr() else {
            return self.unit_ex(span);
        };
        match p.op_kind() {
            Some(ast::UnaryOp::Deref) => {
                let ex = self.place_of(&inner, false, ctx);
                match ex.rty {
                    RTy::Ref(_, _, t) => Ex {
                        place: ex.place.deref(),
                        rty: *t,
                        span,
                        is_place: true,
                    },
                    // `*boxed`, `*wrapper` (a `Deref` impl), `*ptr`: the value itself.
                    other => Ex {
                        place: ex.place,
                        rty: other,
                        span,
                        is_place: ex.is_place,
                    },
                }
            }
            _ => {
                let ex = self.expr(&inner, Ctx::default());
                let op = Operand {
                    place: ex.place,
                    moves: false,
                    span: ex.span,
                };
                self.ops(RTy::opaque(), vec![op], span, ctx)
            }
        }
    }

    fn bin_expr(&mut self, b: &ast::BinExpr, ctx: Ctx, span: Span) -> Ex {
        let (Some(lhs), Some(rhs)) = (b.lhs(), b.rhs()) else {
            return self.unit_ex(span);
        };
        match b.op_kind() {
            Some(ast::BinaryOp::Assignment { op: None }) => {
                // (The value is computed before the place: `v[0] = v[1]` reads first.)
                let r = self.expr(&rhs, Ctx::default());
                let r = self.materialize(r, Ctx::default());
                let op = self.operand(&r);
                if matches!(lhs, ast::Expr::UnderscoreExpr(_)) {
                    return self.unit_ex(span);
                }
                let l = self.place_of(&lhs, true, Ctx::default());
                // (`let x;` assigned later: the first value settles its references.)
                if l.place.is_local() && !l.rty.has_regions() && r.rty.has_regions() {
                    self.body.locals[l.place.local.idx()].rty = r.rty.clone();
                } else {
                    self.env.relate(&mut self.body, &r.rty, &l.rty, span);
                }
                self.emit(StmtKind::Assign(l.place, Rvalue::Use(op)), span);
                self.unit_ex(span)
            }
            Some(ast::BinaryOp::Assignment { op: Some(_) }) => {
                let r = self.expr(&rhs, Ctx::default());
                let r = self.materialize(r, Ctx::default());
                let rop = self.operand(&r);
                let l = self.place_of(&lhs, true, Ctx::default());
                let primitive = matches!(
                    self.ty_at(lhs.syntax()),
                    Some(Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char)
                ) || matches!(l.rty, RTy::Opaque(false));
                if primitive {
                    let lop = Operand {
                        place: l.place.clone(),
                        moves: false,
                        span: l.span,
                    };
                    self.emit(
                        StmtKind::Assign(
                            l.place,
                            Rvalue::Ops {
                                args: vec![lop, rop],
                                activate: Vec::new(),
                            },
                        ),
                        span,
                    );
                } else {
                    // `s += x` on a `String` or an `AddAssign` impl: `add_assign(&mut s, x)`.
                    let region = self.fresh();
                    let recv = self.borrow(region, true, l.place.clone(), l.rty.clone(), span, true);
                    let t = self.temp(RTy::opaque(), span);
                    self.emit(
                        StmtKind::Assign(
                            Place::local(t),
                            Rvalue::Ops {
                                args: vec![
                                    Operand {
                                        place: Place::local(recv),
                                        moves: false,
                                        span,
                                    },
                                    rop,
                                ],
                                activate: vec![(l.place, region)],
                            },
                        ),
                        span,
                    );
                }
                self.unit_ex(span)
            }
            Some(ast::BinaryOp::LogicOp(_)) => {
                // Short-circuit: the right side runs on one branch only.
                let then = self.new_block();
                let els = self.new_block();
                self.cond(&ast::Expr::BinExpr(b.clone()), then, els);
                let join = self.new_block();
                let t = self.temp(RTy::opaque(), span);
                self.switch_to(then);
                self.goto(join, span);
                self.switch_to(els);
                self.goto(join, span);
                self.switch_to(join);
                Ex {
                    place: Place::local(t),
                    rty: RTy::opaque(),
                    span,
                    is_place: false,
                }
            }
            Some(ast::BinaryOp::CmpOp(_)) => {
                // `PartialEq::eq(&a, &b)`: both sides borrowed for the comparison.
                let mut args = Vec::new();
                for side in [lhs, rhs] {
                    let ex = self.place_of(&side, false, Ctx::default());
                    let region = self.fresh();
                    let t = self.borrow(region, false, ex.place, ex.rty, ex.span, false);
                    args.push(Operand {
                        place: Place::local(t),
                        moves: false,
                        span: ex.span,
                    });
                }
                self.ops(RTy::opaque(), args, span, ctx)
            }
            _ => {
                // Arithmetic: by value (a move of a non-`Copy` operand, as `String + &str`).
                let l = self.expr(&lhs, Ctx::default());
                let lop = self.operand(&l);
                let r = self.expr(&rhs, Ctx::default());
                let rop = self.operand(&r);
                let rty = match (&l.rty, self.ty_at(b.syntax())) {
                    (RTy::String, _) => RTy::String,
                    (_, Some(Ty::Str)) => RTy::String,
                    (RTy::Adt(..), _) => l.rty.clone(),
                    _ => RTy::opaque(),
                };
                self.ops(rty, vec![lop, rop], span, ctx)
            }
        }
    }

    /// Branch on `e`: to `then` when true, `els` when false. `let` conditions bind into the
    /// current scope (the caller pushes one around the condition and the `then` branch).
    fn cond(&mut self, e: &ast::Expr, then: Block, els: Block) {
        let span = self.span(e.syntax());
        match e {
            ast::Expr::ParenExpr(p) if p.expr().is_some() => {
                self.cond(&p.expr().unwrap(), then, els)
            }
            ast::Expr::BinExpr(b)
                if matches!(b.op_kind(), Some(ast::BinaryOp::LogicOp(ast::LogicOp::And))) =>
            {
                let (Some(l), Some(r)) = (b.lhs(), b.rhs()) else {
                    return self.terminate(Term::Goto(els), span);
                };
                let mid = self.new_block();
                self.cond(&l, mid, els);
                self.switch_to(mid);
                self.cond(&r, then, els);
            }
            ast::Expr::BinExpr(b)
                if matches!(b.op_kind(), Some(ast::BinaryOp::LogicOp(ast::LogicOp::Or))) =>
            {
                let (Some(l), Some(r)) = (b.lhs(), b.rhs()) else {
                    return self.terminate(Term::Goto(els), span);
                };
                let mid = self.new_block();
                self.cond(&l, then, mid);
                self.switch_to(mid);
                self.cond(&r, then, els);
            }
            ast::Expr::LetExpr(l) => {
                let (Some(pat), Some(scrut)) = (l.pat(), l.expr()) else {
                    return self.terminate(Term::Goto(els), span);
                };
                let ex = self.place_of(&scrut, false, Ctx::default());
                let op = Operand {
                    place: ex.place.clone(),
                    moves: false,
                    span: ex.span,
                };
                let bind = self.new_block();
                self.terminate(Term::Switch(Some(op), vec![bind, els]), span);
                self.switch_to(bind);
                self.pattern(&pat, ex.place, ex.rty, None, span);
                self.goto(then, span);
            }
            _ => {
                let ex = self.expr(e, Ctx::default());
                let op = Operand {
                    place: ex.place,
                    moves: false,
                    span: ex.span,
                };
                self.terminate(Term::Switch(Some(op), vec![then, els]), span);
            }
        }
    }

    fn if_expr(&mut self, i: &ast::IfExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(c) = i.condition() else {
            return self.unit_ex(span);
        };
        let rty = self.rty_at(i.syntax());
        let result = self.temp(rty.clone(), span);
        let then = self.new_block();
        let els = self.new_block();
        let join = self.new_block();
        self.push_scope();
        self.cond(&c, then, els);
        self.switch_to(then);
        if let Some(tb) = i.then_branch() {
            let ex = self.block_inner(&tb, ctx);
            self.store_result(result, ex, span);
        }
        self.pop_scope(span);
        self.goto(join, span);
        self.switch_to(els);
        match i.else_branch() {
            Some(ast::ElseBranch::Block(b)) => {
                let ex = self.block_inner(&b, ctx);
                self.store_result(result, ex, span);
            }
            Some(ast::ElseBranch::IfExpr(e)) => {
                let ex = self.if_expr(&e, ctx, span);
                self.store_result(result, ex, span);
            }
            None => {}
        }
        self.goto(join, span);
        self.switch_to(join);
        let rty = self.body.locals[result.idx()].rty.clone();
        Ex {
            place: Place::local(result),
            rty,
            span,
            is_place: false,
        }
    }

    /// A branch's value into the expression's result temporary: the first branch settles the
    /// result's reference shape, later ones relate to it.
    fn store_result(&mut self, result: Local, ex: Ex, span: Span) {
        if !self.reachable {
            return;
        }
        let op = self.operand(&ex);
        let cur = self.body.locals[result.idx()].rty.clone();
        if !cur.has_regions() && ex.rty.has_regions() {
            self.body.locals[result.idx()].rty = ex.rty.clone();
        } else {
            self.env.relate(&mut self.body, &ex.rty, &cur, span);
        }
        self.emit(StmtKind::Assign(Place::local(result), Rvalue::Use(op)), span);
    }

    fn match_expr(&mut self, m: &ast::MatchExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(scrut) = m.expr() else {
            return self.unit_ex(span);
        };
        let rty = self.rty_at(m.syntax());
        let result = self.temp(rty, span);
        let ex = self.place_of(&scrut, false, Ctx::default());
        let arms: Vec<ast::MatchArm> = m.match_arm_list().map(|l| l.arms().collect()).unwrap_or_default();
        let tests: Vec<Block> = arms.iter().map(|_| self.new_block()).collect();
        let join = self.new_block();
        let op = Operand {
            place: ex.place.clone(),
            moves: false,
            span: ex.span,
        };
        let mut targets = tests.clone();
        if targets.is_empty() {
            targets.push(join);
        }
        self.terminate(Term::Switch(Some(op), targets), span);
        for (i, arm) in arms.iter().enumerate() {
            self.switch_to(tests[i]);
            self.push_scope();
            if let Some(p) = arm.pat() {
                self.pattern(&p, ex.place.clone(), ex.rty.clone(), None, span);
            }
            if let Some(g) = arm.guard().and_then(|g| g.condition()) {
                let body = self.new_block();
                let next = tests.get(i + 1).copied().unwrap_or(join);
                self.cond(&g, body, next);
                self.switch_to(body);
            }
            if let Some(e) = arm.expr() {
                let aex = self.expr(&e, ctx);
                let aex = self.materialize(aex, ctx);
                self.store_result(result, aex, span);
            }
            self.pop_scope(span);
            self.goto(join, span);
        }
        self.switch_to(join);
        let rty = self.body.locals[result.idx()].rty.clone();
        Ex {
            place: Place::local(result),
            rty,
            span,
            is_place: false,
        }
    }

    fn loop_expr(&mut self, l: &ast::LoopExpr, span: Span) -> Ex {
        let label = l.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
        let rty = self.rty_at(l.syntax());
        let result = self.temp(rty, span);
        let head = self.new_block();
        let brk = self.new_block();
        self.goto(head, span);
        self.switch_to(head);
        self.loops.push(LoopCtx {
            label,
            brk,
            cont: Some(head),
            scope_depth: self.scopes.len(),
            result: Some(result),
        });
        if let Some(b) = l.loop_body() {
            self.block_inner(&b, Ctx::default());
        }
        self.loops.pop();
        self.goto(head, span);
        self.switch_to(brk);
        let rty = self.body.locals[result.idx()].rty.clone();
        Ex {
            place: Place::local(result),
            rty,
            span,
            is_place: false,
        }
    }

    fn while_expr(&mut self, w: &ast::WhileExpr, span: Span) -> Ex {
        let label = w.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
        let head = self.new_block();
        let body = self.new_block();
        let brk = self.new_block();
        self.goto(head, span);
        self.switch_to(head);
        self.push_scope();
        match w.condition() {
            Some(c) => self.cond(&c, body, brk),
            None => self.goto(brk, span),
        }
        self.switch_to(body);
        self.loops.push(LoopCtx {
            label,
            brk,
            cont: Some(head),
            scope_depth: self.scopes.len(),
            result: None,
        });
        if let Some(b) = w.loop_body() {
            self.block_inner(&b, Ctx::default());
        }
        self.loops.pop();
        self.pop_scope(span);
        self.goto(head, span);
        self.switch_to(brk);
        self.unit_ex(span)
    }

    fn for_expr(&mut self, f: &ast::ForExpr, span: Span) -> Ex {
        let label = f.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
        let (Some(pat), Some(iterable)) = (f.pat(), f.iterable()) else {
            return self.unit_ex(span);
        };
        // `IntoIterator::into_iter(iterable)`: what the loop iterates and what it yields.
        let ex = self.expr(&iterable, Ctx::default());
        let (it, elem) = self.loop_iterator(ex, span);
        let head = self.new_block();
        let body = self.new_block();
        let brk = self.new_block();
        self.goto(head, span);
        self.switch_to(head);
        // `next(&mut it)`, on the iterable as rustc has it (`immutable borrow later used here`
        // under `&v`).
        let iter_span = self.span(iterable.syntax());
        let region = self.fresh();
        let it_rty = self.body.locals[it.idx()].rty.clone();
        let recv = self.borrow(region, true, Place::local(it), it_rty, iter_span, true);
        let next = self.temp(RTy::Option(Box::new(elem.clone())), iter_span);
        self.emit(
            StmtKind::Assign(
                Place::local(next),
                Rvalue::Ops {
                    args: vec![Operand {
                        place: Place::local(recv),
                        moves: false,
                        span: iter_span,
                    }],
                    activate: vec![(Place::local(it), region)],
                },
            ),
            iter_span,
        );
        self.emit(StmtKind::StorageDead(recv), iter_span);
        let op = Operand {
            place: Place::local(next),
            moves: false,
            span: iter_span,
        };
        self.terminate(Term::Switch(Some(op), vec![body, brk]), iter_span);
        self.switch_to(body);
        self.push_scope();
        self.pattern(
            &pat,
            Place::local(next).downcast("Some").field("0"),
            elem,
            None,
            span,
        );
        self.loops.push(LoopCtx {
            label,
            brk,
            cont: Some(head),
            scope_depth: self.scopes.len(),
            result: None,
        });
        if let Some(b) = f.loop_body() {
            self.block_inner(&b, Ctx::default());
        }
        self.loops.pop();
        self.pop_scope(span);
        self.goto(head, span);
        self.switch_to(brk);
        self.unit_ex(span)
    }

    /// The iterator a `for` loop runs over, as a temporary, and its items.
    fn loop_iterator(&mut self, ex: Ex, span: Span) -> (Local, RTy) {
        let (it_rty, elem) = match &ex.rty {
            RTy::Iter { elem, .. } => (ex.rty.clone(), (**elem).clone()),
            RTy::Ref(r, m, inner) => {
                let elem = match &**inner {
                    RTy::Vec(e) | RTy::Set(e) => RTy::Ref(*r, *m, e.clone()),
                    RTy::Map(k, v) => RTy::Tuple(vec![
                        RTy::Ref(*r, false, k.clone()),
                        RTy::Ref(*r, *m, v.clone()),
                    ]),
                    RTy::Option(e) => RTy::Ref(*r, *m, e.clone()),
                    RTy::Iter { elem, .. } => (**elem).clone(),
                    _ => RTy::opaque(),
                };
                (
                    RTy::Iter {
                        region: Some(*r),
                        mutable: *m,
                        elem: Box::new(elem.clone()),
                    },
                    elem,
                )
            }
            RTy::Vec(e) | RTy::Set(e) | RTy::Option(e) => (
                RTy::Iter {
                    region: None,
                    mutable: false,
                    elem: e.clone(),
                },
                (**e).clone(),
            ),
            RTy::Map(k, v) => {
                let elem = RTy::Tuple(vec![(**k).clone(), (**v).clone()]);
                (
                    RTy::Iter {
                        region: None,
                        mutable: false,
                        elem: Box::new(elem.clone()),
                    },
                    elem,
                )
            }
            RTy::Adt(n, ..) => {
                let elem = match self.env.adts.iter_items.get(n).cloned() {
                    Some(t) => self.env.rty_of_ty(&mut self.body, &t),
                    None => RTy::opaque(),
                };
                let mut rs = Vec::new();
                ex.rty.regions(&mut rs);
                let region = match rs.as_slice() {
                    [] => None,
                    [r] => Some(*r),
                    many => {
                        let r = self.fresh();
                        for x in many {
                            self.body.outlives(*x, r, span);
                        }
                        Some(r)
                    }
                };
                (
                    RTy::Iter {
                        region,
                        mutable: false,
                        elem: Box::new(elem.clone()),
                    },
                    elem,
                )
            }
            _ => (
                RTy::Iter {
                    region: None,
                    mutable: false,
                    elem: Box::new(RTy::opaque()),
                },
                RTy::opaque(),
            ),
        };
        let op = self.operand(&ex);
        let it = self.temp(it_rty, span);
        self.emit(StmtKind::Assign(Place::local(it), Rvalue::Use(op)), span);
        (it, elem)
    }

    fn break_expr(&mut self, b: &ast::BreakExpr, span: Span) -> Ex {
        let label = b.lifetime().map(|l| l.text().to_string());
        let value = b.expr().map(|e| self.expr(&e, Ctx::default()));
        let target = self.find_loop(label.as_deref());
        if let Some(i) = target {
            if let (Some(v), Some(result)) = (value, self.loops[i].result) {
                let op = self.operand(&v);
                let cur = self.body.locals[result.idx()].rty.clone();
                if !cur.has_regions() && v.rty.has_regions() {
                    self.body.locals[result.idx()].rty = v.rty.clone();
                } else {
                    self.env.relate(&mut self.body, &v.rty, &cur, span);
                }
                self.emit(StmtKind::Assign(Place::local(result), Rvalue::Use(op)), span);
            }
            let depth = self.loops[i].scope_depth;
            let brk = self.loops[i].brk;
            self.unwind_to(depth, span);
            self.goto(brk, span);
        } else {
            self.terminate(Term::Unreachable, span);
        }
        self.unit_ex(span)
    }

    fn continue_expr(&mut self, c: &ast::ContinueExpr, span: Span) -> Ex {
        let label = c.lifetime().map(|l| l.text().to_string());
        if let Some(i) = self.find_loop(label.as_deref())
            && let Some(cont) = self.loops[i].cont
        {
            let depth = self.loops[i].scope_depth;
            self.unwind_to(depth, span);
            self.goto(cont, span);
        } else {
            self.terminate(Term::Unreachable, span);
        }
        self.unit_ex(span)
    }

    /// The loop a `break` / `continue` leaves: by label, or the innermost loop (an unlabeled
    /// `break` can't leave a labeled block).
    fn find_loop(&self, label: Option<&str>) -> Option<usize> {
        self.loops.iter().rposition(|l| match label {
            Some(n) => l.label.as_deref() == Some(n),
            None => l.cont.is_some(),
        })
    }

    fn return_expr(&mut self, r: &ast::ReturnExpr, span: Span) -> Ex {
        let ex = match r.expr() {
            Some(e) => self.expr(&e, Ctx::default()),
            None => self.unit_ex(span),
        };
        let direct = matches!(ex.rty, RTy::Ref(..));
        self.unwind_to(1, span);
        self.ret_value(ex, span, direct);
        self.unit_ex(span)
    }

    fn try_expr(&mut self, t: &ast::TryExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(inner) = t.expr() else {
            return self.unit_ex(span);
        };
        let ex = self.expr(&inner, Ctx::default());
        let ex = self.materialize(ex, ctx);
        let ok = self.new_block();
        let err = self.new_block();
        let op = Operand {
            place: ex.place.clone(),
            moves: false,
            span: ex.span,
        };
        self.terminate(Term::Switch(Some(op), vec![ok, err]), span);
        self.switch_to(err);
        let (ok_rty, err_rty, variant) = match &ex.rty {
            RTy::Result(a, b) => ((**a).clone(), Some((**b).clone()), "Ok"),
            RTy::Option(a) => ((**a).clone(), None, "Some"),
            _ => (RTy::opaque(), None, "Ok"),
        };
        let err_ex = Ex {
            place: ex.place.clone().downcast("Err").field("0"),
            rty: err_rty.clone().unwrap_or(RTy::opaque()),
            span,
            is_place: true,
        };
        let eop = self.operand(&err_ex);
        let ret_err = match &self.ret_rty {
            RTy::Result(_, e) => Some((**e).clone()),
            _ => None,
        };
        if let (Some(e), Some(re)) = (&err_rty, &ret_err) {
            self.env.relate(&mut self.body, e, re, span);
        }
        self.unwind_to(1, span);
        self.emit(
            StmtKind::Assign(Place::local(self.ret_local), Rvalue::Use(eop)),
            span,
        );
        self.body.returns.push(ReturnSite {
            span,
            direct_ref: false,
        });
        self.terminate(Term::Return, span);
        self.switch_to(ok);
        Ex {
            place: ex.place.downcast(variant).field("0"),
            rty: ok_rty,
            span,
            is_place: true,
        }
    }

    fn array_expr(&mut self, a: &ast::ArrayExpr, ctx: Ctx, span: Span) -> Ex {
        let mut args = Vec::new();
        let mut elem: Option<RTy> = None;
        match a.kind() {
            ast::ArrayExprKind::Repeat { initializer, .. } => {
                if let Some(e) = initializer {
                    let ex = self.expr(&e, ctx);
                    args.push(self.operand(&ex));
                    elem = Some(ex.rty);
                }
            }
            ast::ArrayExprKind::ElementList(es) => {
                for e in es {
                    let ex = self.expr(&e, ctx);
                    args.push(self.operand(&ex));
                    match &elem {
                        None => elem = Some(ex.rty.clone()),
                        Some(first) => {
                            let first = first.clone();
                            self.env.relate(&mut self.body, &ex.rty, &first, span);
                        }
                    }
                }
            }
        }
        let rty = RTy::Vec(Box::new(elem.unwrap_or(RTy::opaque())));
        self.ops(rty, args, span, ctx)
    }

    fn record_expr(&mut self, r: &ast::RecordExpr, ctx: Ctx, span: Span) -> Ex {
        let rty = self.rty_at(r.syntax());
        let segs: Vec<String> = r
            .path()
            .map(|p| {
                p.segments()
                    .filter_map(|s| s.name_ref().map(|n| n.text().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let variant = match (&rty, segs.as_slice()) {
            (RTy::Adt(n, ..), [.., v]) if n != v && v != "Self" => v.clone(),
            _ => String::new(),
        };
        let mut args = Vec::new();
        for f in r.record_expr_field_list().into_iter().flat_map(|l| l.fields()) {
            let Some(name) = f.field_name() else { continue };
            let fname = name.to_string();
            let ex = match f.expr() {
                Some(e) => self.expr(&e, ctx),
                // `S { name }` shorthand.
                None => {
                    let fspan = self.span(f.syntax());
                    match self.lookup(&fname) {
                        Some(l) => Ex {
                            place: Place::local(l),
                            rty: self.body.locals[l.idx()].rty.clone(),
                            span: fspan,
                            is_place: true,
                        },
                        None => match self.upvars.get(&fname).cloned() {
                            Some(_) => {
                                let p = ast::Expr::PathExpr(
                                    ast::PathExpr::cast(f.syntax().first_child().unwrap_or_else(|| f.syntax().clone()))
                                        .unwrap_or_else(|| panic!()),
                                );
                                self.expr(&p, ctx)
                            }
                            None => self.unit_ex(fspan),
                        },
                    }
                }
            };
            if let RTy::Adt(..) = &rty {
                let frty = self.field_rty(&variant, &fname, &rty);
                self.env.relate(&mut self.body, &ex.rty, &frty, span);
            }
            args.push(self.operand(&ex));
        }
        if let Some(base) = r.record_expr_field_list().and_then(|l| l.spread()) {
            let ex = self.expr(&base, ctx);
            // (The remaining fields move out of the base; moving a borrowed base is reported
            // by rustc only for the fields moved: read it instead.)
            args.push(Operand {
                place: ex.place.clone(),
                moves: false,
                span: ex.span,
            });
            let base_rty = ex.rty.clone();
            self.env.relate(&mut self.body, &base_rty, &rty, span);
        }
        self.ops(rty, args, span, ctx)
    }

    // ------------------------------------------------------------ calls

    fn call_expr(&mut self, c: &ast::CallExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(callee) = c.expr() else {
            return self.unit_ex(span);
        };
        let arg_exprs: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let segs: Vec<String> = match &callee {
            ast::Expr::PathExpr(p) => p
                .path()
                .map(|p| {
                    p.segments()
                        .filter_map(|s| s.name_ref().map(|n| n.text().to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        // A closure or function value held in a local: `f(x)`.
        let callee_local = match segs.as_slice() {
            [n] => self.lookup(n).map(|l| (l, false)).or_else(|| self.upvars.get(n).map(|u| (u.local, u.by_ref.is_some()))),
            _ => None,
        };
        if let Some((l, through_ref)) = callee_local {
            let place = if through_ref { Place::local(l).deref() } else { Place::local(l) };
            let frty = self.place_rty(&place);
            let region = self.fresh();
            let recv = self.borrow(region, false, place, frty.clone(), span, false);
            let mut args = vec![Operand {
                place: Place::local(recv),
                moves: false,
                span,
            }];
            for a in &arg_exprs {
                let ex = self.expr(a, Ctx::default());
                args.push(self.operand(&ex));
            }
            let ret = match frty.strip_refs() {
                RTy::Closure { ret, .. } => (**ret).clone(),
                _ => RTy::opaque(),
            };
            return self.ops(ret, args, span, ctx);
        }
        // A function of this file (free, or `Type::f` / `Self::f`).
        let fs: Option<FnSyn> = match segs.as_slice() {
            [f] => self.env.free_fns.get(f).cloned().flatten(),
            [t, f] => {
                let t = if t == "Self" {
                    self.self_adt.clone().unwrap_or_default()
                } else {
                    t.clone()
                };
                self.env.methods.get(&(t, f.clone())).cloned().flatten()
            }
            _ => None,
        };
        if let Some(fs) = fs {
            // (For `Type::f`, the type arguments are unknown here: the impl's are opaque.)
            let self_adt = match segs.as_slice() {
                [t, _] => {
                    let name = if t == "Self" {
                        self.self_adt.clone().unwrap_or_default()
                    } else {
                        t.clone()
                    };
                    let ty = self.ty_at(c.syntax()).cloned();
                    match ty {
                        Some(Ty::Adt(n, args)) if n == name => {
                            let args: Vec<RTy> = args.iter().map(|a| self.env.rty_of_ty(&mut self.body, a)).collect();
                            Some(RTy::Adt(n, args, Vec::new()))
                        }
                        _ => Some(RTy::Adt(name, Vec::new(), Vec::new())),
                    }
                }
                _ => None,
            };
            let sig = self.env.signature(&mut self.body, &fs, self_adt.as_ref(), false, span);
            let mut args = Vec::new();
            // (`Type::method(&x, ..)`: the receiver is the first argument.)
            let mut expects: Vec<RTy> = Vec::new();
            if let Some(st) = &sig.self_ty {
                expects.push(st.clone());
            }
            expects.extend(sig.params.iter().cloned());
            for (i, a) in arg_exprs.iter().enumerate() {
                let ex = self.expr(a, Ctx::default());
                if let Some(exp) = expects.get(i) {
                    self.env.relate(&mut self.body, &ex.rty, exp, span);
                }
                args.push(self.operand(&ex));
            }
            return self.ops(sig.ret.clone(), args, span, ctx);
        }
        // A tuple struct or enum variant constructor: an aggregate.
        let ty = self.ty_at(c.syntax()).cloned();
        let is_ctor = match (&ty, segs.as_slice()) {
            (Some(Ty::Adt(n, _)), [f]) => {
                self.env.adts.defs.get(n).is_some_and(|d| d.name == *f || d.variants.iter().any(|v| v.name == *f))
            }
            (Some(Ty::Adt(n, _)), [t, f]) => {
                (t == n || t == "Self")
                    && self.env.adts.defs.get(n).is_some_and(|d| d.variants.iter().any(|v| v.name == *f))
            }
            _ => false,
        };
        if is_ctor && let Some(Ty::Adt(n, targs)) = &ty {
            let targs: Vec<RTy> = targs.iter().map(|a| self.env.rty_of_ty(&mut self.body, a)).collect();
            let regions: Vec<Region> = self
                .env
                .adt_syn
                .get(n)
                .map(|d| d.lifetimes.iter().map(|_| self.body.new_region()).collect())
                .unwrap_or_default();
            let rty = RTy::Adt(n.clone(), targs, regions);
            let variant = match segs.last() {
                Some(v) if v != n => v.clone(),
                _ => String::new(),
            };
            let mut args = Vec::new();
            for (i, a) in arg_exprs.iter().enumerate() {
                let ex = self.expr(a, ctx);
                let frty = self.field_rty(&variant, &i.to_string(), &rty);
                self.env.relate(&mut self.body, &ex.rty, &frty, span);
                args.push(self.operand(&ex));
            }
            return self.ops(rty, args, span, ctx);
        }
        // std: `Some(x)`, `String::from(s)`, `std::mem::take(&mut x)`, ..
        let mut exs = Vec::new();
        if matches!(callee, ast::Expr::PathExpr(_)) {
            for a in &arg_exprs {
                exs.push(self.expr(a, ctx));
            }
            let rtys: Vec<RTy> = exs.iter().map(|e| e.rty.clone()).collect();
            if let Some((ret, expects)) = nll_std::free_fn_result(self.env, &mut self.body, &segs, &rtys) {
                let mut args = Vec::new();
                for (i, ex) in exs.iter().enumerate() {
                    if let Some(Some(exp)) = expects.get(i) {
                        self.env.relate(&mut self.body, &ex.rty, exp, span);
                    }
                    args.push(self.operand(ex));
                }
                return self.ops(ret, args, span, ctx);
            }
            let mut args = Vec::new();
            for ex in &exs {
                args.push(self.operand(ex));
            }
            let ret = match segs.last().map(|s| s.as_str()) {
                Some("drop") => RTy::opaque(),
                _ => self.rty_at(c.syntax()),
            };
            return self.ops(ret, args, span, ctx);
        }
        // Calling an expression: `(self.f)(x)`, `make()(x)`.
        let fex = self.expr(&callee, Ctx::default());
        let mut args = vec![Operand {
            place: fex.place.clone(),
            moves: false,
            span: fex.span,
        }];
        for a in &arg_exprs {
            let ex = self.expr(a, Ctx::default());
            args.push(self.operand(&ex));
        }
        let ret = match fex.rty.strip_refs() {
            RTy::Closure { ret, .. } => (**ret).clone(),
            _ => RTy::opaque(),
        };
        self.ops(ret, args, span, ctx)
    }

    fn method_call(&mut self, m: &ast::MethodCallExpr, ctx: Ctx, span: Span) -> Ex {
        let (Some(name), Some(recv_expr)) = (m.name_ref(), m.receiver()) else {
            return self.unit_ex(span);
        };
        let name = name.text().to_string();
        let arg_exprs: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let recv = self.place_of(&recv_expr, false, Ctx::default());
        // Auto-deref to the type the method is on.
        let (place, mut base, _) = self.auto_deref(recv.place.clone(), recv.rty.clone(), None);
        // `(&v).into_iter()`: the reference itself is the receiver.
        let by_ref_into_iter = name == "into_iter" && recv.rty.is_ref();
        if by_ref_into_iter {
            let (region, elem) = match &recv.rty {
                RTy::Ref(r, mu, inner) => (
                    Some(*r),
                    match &**inner {
                        RTy::Vec(e) | RTy::Set(e) | RTy::Option(e) => RTy::Ref(*r, *mu, e.clone()),
                        RTy::Map(k, v) => RTy::Tuple(vec![RTy::Ref(*r, false, k.clone()), RTy::Ref(*r, *mu, v.clone())]),
                        RTy::Iter { elem, .. } => (**elem).clone(),
                        _ => RTy::opaque(),
                    },
                ),
                _ => (None, RTy::opaque()),
            };
            let op = self.operand(&recv);
            return self.ops(
                RTy::Iter {
                    region,
                    mutable: matches!(recv.rty, RTy::Ref(_, true, _)),
                    elem: Box::new(elem),
                },
                vec![op],
                span,
                ctx,
            );
        }
        // A method of a type of this file.
        let mut user: Option<(FnSyn, RTy)> = None;
        // (Through at most a few `Deref` impls; a target that lowers to the same shape stops.)
        for _ in 0..4 {
            let before = base.clone();
            match &base {
                RTy::Adt(n, ..) => {
                    if let Some(Some(fs)) = self.env.methods.get(&(n.clone(), name.clone())) {
                        user = Some((fs.clone(), base.clone()));
                        break;
                    }
                    // Through an `impl Deref`: the method is the target's.
                    if let Some(target) = self.env.adts.deref_targets.get(n).cloned() {
                        base = self.env.rty_of_ty(&mut self.body, &target);
                        continue;
                    }
                    break;
                }
                RTy::Opaque(_) => {
                    // A struct from another module: its erased type.
                    if let Some(Ty::Adt(n, _)) = self.ty_at(recv_expr.syntax())
                        && let Some(target) = self.env.adts.deref_targets.get(n).cloned()
                    {
                        base = self.env.rty_of_ty(&mut self.body, &target);
                        if base == before {
                            break;
                        }
                        continue;
                    }
                    break;
                }
                _ => break,
            }
        }
        if let Some((fs, adt)) = user {
            let sig = self.env.signature(&mut self.body, &fs, Some(&adt), false, span);
            let recv_op = match &sig.self_ty {
                Some(RTy::Ref(r, mu, inner)) => {
                    let inner = (**inner).clone();
                    let t = self.borrow(*r, *mu, place.clone(), inner, recv.span, *mu);
                    if *mu {
                        (Operand { place: Place::local(t), moves: false, span: recv.span }, Some((place.clone(), *r)))
                    } else {
                        (Operand { place: Place::local(t), moves: false, span: recv.span }, None)
                    }
                }
                Some(st) => {
                    let st = st.clone();
                    self.env.relate(&mut self.body, &base, &st, span);
                    let ex = Ex { place: place.clone(), rty: base.clone(), span: recv.span, is_place: recv.is_place };
                    (self.operand(&ex), None)
                }
                None => {
                    let ex = Ex { place: place.clone(), rty: base.clone(), span: recv.span, is_place: recv.is_place };
                    (Operand { place: ex.place, moves: false, span: recv.span }, None)
                }
            };
            let (recv_op, activate) = recv_op;
            let mut args = vec![recv_op];
            for (i, a) in arg_exprs.iter().enumerate() {
                let ex = self.expr(a, Ctx::default());
                if let Some(exp) = sig.params.get(i) {
                    self.env.relate(&mut self.body, &ex.rty, exp, span);
                }
                args.push(self.operand(&ex));
            }
            return self.assign_temp(
                sig.ret.clone(),
                Rvalue::Ops { args, activate: activate.into_iter().collect() },
                span,
                ctx,
            );
        }
        // std (or an unknown method): the table.
        let mut mode = nll_std::recv_mode(&base, &name);
        if matches!(base, RTy::Opaque(_) | RTy::Adt(..))
            && let Some(r) = self.env.trait_recv.get(&name)
        {
            mode = *r;
        }
        // A by-value method on a place behind a reference can't move it out: `iter()` of a
        // `&Vec` is by reference; so treat by-value on a deref'd place as a read.
        let region = self.fresh();
        let (recv_op, activate) = match mode {
            Recv::Ref => {
                let t = self.borrow(region, false, place.clone(), base.clone(), recv.span, false);
                (Operand { place: Place::local(t), moves: false, span: recv.span }, None)
            }
            Recv::MutRef => {
                let t = self.borrow(region, true, place.clone(), base.clone(), recv.span, true);
                (Operand { place: Place::local(t), moves: false, span: recv.span }, Some((place.clone(), region)))
            }
            Recv::Value => {
                let ex = Ex { place: place.clone(), rty: base.clone(), span: recv.span, is_place: recv.is_place };
                (self.operand(&ex), None)
            }
        };
        let mut exs: Vec<Ex> = Vec::new();
        for (i, a) in arg_exprs.iter().enumerate() {
            if matches!(crate::typeck::strip_parens(a), ast::Expr::ClosureExpr(_)) {
                self.pending_hints = self.closure_hints(&base, &name, i, &exs);
            }
            let ex = self.expr(a, Ctx::default());
            self.pending_hints.clear();
            exs.push(ex);
        }
        let rtys: Vec<RTy> = exs.iter().map(|e| e.rty.clone()).collect();
        let ty = self.ty_at(m.syntax()).cloned();
        let (ret, expects) = nll_std::result(self.env, &mut self.body, &base, region, &name, &rtys, ty.as_ref(), span);
        // A by-value receiver that is an iterator or closure hands its borrows on.
        let ret = match (&mode, &base, &ret) {
            (Recv::Value, _, RTy::Iter { region: Some(_), .. }) => ret,
            (Recv::Value, RTy::Iter { .. }, RTy::Iter { region: None, mutable, elem }) => {
                let mut rs = Vec::new();
                base.regions(&mut rs);
                for e in &exs {
                    e.rty.regions(&mut rs);
                }
                if rs.is_empty() {
                    ret
                } else {
                    let r = self.fresh();
                    for x in rs {
                        self.body.outlives(x, r, span);
                    }
                    RTy::Iter { region: Some(r), mutable: *mutable, elem: elem.clone() }
                }
            }
            _ => ret,
        };
        let mut args = vec![recv_op];
        for (i, ex) in exs.iter().enumerate() {
            if let Some(Some(exp)) = expects.get(i) {
                self.env.relate(&mut self.body, &ex.rty, exp, span);
            }
            args.push(self.operand(ex));
        }
        // Closures handed to an adapter are held by the result: their captures live on.
        if let RTy::Iter { region: Some(r), .. } = &ret {
            let r = *r;
            for ex in &exs {
                if let RTy::Closure { region: Some(cr), .. } = &ex.rty {
                    self.body.outlives(*cr, r, span);
                }
            }
        }
        self.assign_temp(ret, Rvalue::Ops { args, activate: activate.into_iter().collect() }, span, ctx)
    }

    // ------------------------------------------------------------ macros

    fn macro_call(&mut self, mc: &ast::MacroCall, ctx: Ctx, span: Span) -> Ex {
        let name = macros::macro_name(mc).unwrap_or_default();
        let args = macros::split_args(self.env.src, self.base, mc);
        match name.as_str() {
            "vec" => {
                if let Some((value, _count)) = macros::split_repeat(self.env.src, self.base, mc) {
                    let ex = self.macro_arg(&value, ctx);
                    let op = ex.as_ref().map(|e| self.operand(e));
                    let elem = ex.map(|e| e.rty).unwrap_or(RTy::opaque());
                    return self.ops(RTy::Vec(Box::new(elem)), op.into_iter().collect(), span, ctx);
                }
                let mut ops = Vec::new();
                let mut elem: Option<RTy> = None;
                for a in &args {
                    if let Some(ex) = self.macro_arg(a, ctx) {
                        ops.push(self.operand(&ex));
                        match &elem {
                            None => elem = Some(ex.rty),
                            Some(first) => {
                                let first = first.clone();
                                self.env.relate(&mut self.body, &ex.rty, &first, span);
                            }
                        }
                    }
                }
                self.ops(RTy::Vec(Box::new(elem.unwrap_or(RTy::opaque()))), ops, span, ctx)
            }
            "matches" => match macros::matches_as_match(&args) {
                Some(arg) => self.macro_arg(&arg, ctx).unwrap_or_else(|| self.unit_ex(span)),
                None => self.unit_ex(span),
            },
            "dbg" => {
                let mut ops = Vec::new();
                let mut rty = RTy::opaque();
                for a in &args {
                    if let Some(ex) = self.macro_arg(a, ctx) {
                        ops.push(self.operand(&ex));
                        rty = ex.rty;
                    }
                }
                self.ops(rty, ops, span, ctx)
            }
            "println" | "print" | "eprintln" | "eprint" | "format" | "format_args" | "write"
            | "writeln" | "panic" | "assert" | "assert_eq" | "assert_ne" | "debug_assert"
            | "debug_assert_eq" | "debug_assert_ne" | "unreachable" | "todo" | "unimplemented"
            | "log" | "info" | "warn" | "error" | "debug" | "trace" | "tprint" | "anyhow"
            | "bail" | "ensure" => {
                let diverges = matches!(name.as_str(), "panic" | "unreachable" | "todo" | "unimplemented" | "bail");
                let mut ops = Vec::new();
                let mut activate = Vec::new();
                // The arguments are evaluated first, then all of them borrowed (as
                // `format_args!` does): (place, type, span).
                let mut borrows: Vec<(Place, RTy, Span)> = Vec::new();
                let mut rest = args.as_slice();
                // `write!(dest, ..)`: the destination is borrowed mutably.
                if matches!(name.as_str(), "write" | "writeln")
                    && let Some((dest, tail)) = args.split_first()
                {
                    rest = tail;
                    let (expr, _) = macros::parse_arg(dest);
                    if let Some(e) = expr {
                        let saved = std::mem::replace(&mut self.base, dest.start);
                        let ex = self.place_of(&e, true, Ctx::default());
                        let (place, rty, _) = self.auto_deref(ex.place, ex.rty, None);
                        let region = self.fresh();
                        // (`f.write_fmt(..)`: a two-phase borrow, activated by the call.)
                        let t = self.borrow(region, true, place.clone(), rty, ex.span, true);
                        activate.push((place, region));
                        self.base = saved;
                        ops.push(Operand { place: Place::local(t), moves: false, span: ex.span });
                    }
                }
                // `assert!(cond, ..)`: the condition is a value.
                if matches!(name.as_str(), "assert" | "debug_assert" | "ensure")
                    && let Some((c, tail)) = rest.split_first()
                {
                    rest = tail;
                    if let Some(ex) = self.macro_arg(c, Ctx::default()) {
                        ops.push(Operand { place: ex.place, moves: false, span: ex.span });
                    }
                }
                let mut first = true;
                for a in rest {
                    // The format string: its `{name}` captures borrow those locals.
                    if first
                        && let Some(fs) = macros::format_string(a)
                    {
                        first = false;
                        for cap in format_captures(&fs) {
                            let p = self.lookup(&cap).map(|l| (Place::local(l), self.body.locals[l.idx()].rty.clone()))
                                .or_else(|| self.upvars.get(&cap).cloned().map(|u| match u.by_ref {
                                    Some(_) => (Place::local(u.local).deref(), match &self.body.locals[u.local.idx()].rty { RTy::Ref(_, _, i) => (**i).clone(), o => o.clone() }),
                                    None => (Place::local(u.local), self.body.locals[u.local.idx()].rty.clone()),
                                }));
                            if let Some((place, rty)) = p {
                                // (On the name inside the string, as rustc has it: `{t}`.)
                                let at = a.text.find(&format!("{{{cap}}}")).or_else(|| a.text.find(&format!("{{{cap}:")));
                                let cspan = at.map_or((a.start, a.end()), |i| (a.start + i + 1, a.start + i + 1 + cap.len()));
                                borrows.push((place, rty, cspan));
                            }
                        }
                        continue;
                    }
                    first = false;
                    let (expr, _) = macros::parse_arg(a);
                    let Some(e) = expr else { continue };
                    let saved = std::mem::replace(&mut self.base, a.start);
                    // A format argument is taken by reference.
                    let ex = self.place_of(&e, false, Ctx::default());
                    self.base = saved;
                    borrows.push((ex.place, ex.rty, ex.span));
                }
                for (place, rty, bspan) in borrows {
                    let region = self.fresh();
                    let t = self.borrow(region, false, place, rty, bspan, false);
                    ops.push(Operand { place: Place::local(t), moves: false, span: bspan });
                }
                let rty = if matches!(name.as_str(), "format" | "anyhow") { RTy::String } else { RTy::opaque() };
                let out = self.assign_temp(rty, Rvalue::Ops { args: ops, activate }, span, ctx);
                if diverges {
                    self.terminate(Term::Unreachable, span);
                }
                out
            }
            "include_str" | "include_bytes" | "env" | "option_env" | "concat" | "stringify"
            | "line" | "column" | "file" | "module_path" | "cfg" | "compile_error"
            | "thread_local" | "const_format" => self.const_ex(RTy::Ref(Region(0), false, Box::new(RTy::Str)), span),
            _ => {
                // An unknown macro: its arguments, if they parse, are read.
                let mut ops = Vec::new();
                for a in &args {
                    if let Some(ex) = self.macro_arg(a, Ctx::default()) {
                        ops.push(Operand { place: ex.place, moves: false, span: ex.span });
                    }
                }
                let rty = self.rty_at(mc.syntax());
                self.ops(rty, ops, span, ctx)
            }
        }
    }

    fn macro_arg(&mut self, a: &macros::MacroArg, ctx: Ctx) -> Option<Ex> {
        let (expr, _) = macros::parse_arg(a);
        let e = expr?;
        let saved = std::mem::replace(&mut self.base, a.start);
        let ex = self.expr(&e, ctx);
        self.base = saved;
        Some(ex)
    }

    // ------------------------------------------------------------ closures

    fn closure_expr(&mut self, c: &ast::ClosureExpr, ctx: Ctx, span: Span) -> Ex {
        let Some(body) = c.body() else {
            return self.unit_ex(span);
        };
        let is_move = c.move_token().is_some();
        let params: Vec<ast::Param> = c.param_list().map(|l| l.params().collect()).unwrap_or_default();
        let hints = std::mem::take(&mut self.pending_hints);
        let captures = self.captures(body.syntax(), &params, is_move);
        // In this body: the captured places are borrowed (or moved) into the closure value.
        let region = self.fresh();
        let mut ops = Vec::new();
        let mut upvars: Vec<(String, Local, Option<bool>, RTy)> = Vec::new();
        for (name, proj, mode) in &captures {
            let (base_place, base_rty) = match self.lookup(name) {
                Some(l) => (Place::local(l), self.body.locals[l.idx()].rty.clone()),
                None => match self.upvars.get(name).cloned() {
                    Some(u) => match u.by_ref {
                        Some(_) => (
                            Place::local(u.local).deref(),
                            match &self.body.locals[u.local.idx()].rty {
                                RTy::Ref(_, _, i) => (**i).clone(),
                                o => o.clone(),
                            },
                        ),
                        None => (Place::local(u.local), self.body.locals[u.local.idx()].rty.clone()),
                    },
                    None => continue,
                },
            };
            let mut place = base_place;
            for f in proj {
                place = place.field(f.clone());
            }
            let prty = self.place_rty(&place);
            match mode {
                CapMode::Value => {
                    let ex = Ex { place: place.clone(), rty: prty.clone(), span, is_place: true };
                    let op = self.operand(&ex);
                    let t = self.temp(prty.clone(), span);
                    self.emit(StmtKind::Assign(Place::local(t), Rvalue::Use(op)), span);
                    self.env.hold(&mut self.body, &prty, region, span);
                    ops.push(Operand { place: Place::local(t), moves: self.env.moves(&prty), span });
                    if !upvars.iter().any(|u| u.0 == *name) {
                        upvars.push((name.clone(), t, None, base_rty));
                    }
                }
                CapMode::Shared | CapMode::Mut => {
                    let mutable = *mode == CapMode::Mut;
                    let r = self.fresh();
                    let t = self.borrow(r, mutable, place, prty, span, false);
                    self.body.outlives(r, region, span);
                    ops.push(Operand { place: Place::local(t), moves: false, span });
                    if !upvars.iter().any(|u| u.0 == *name) {
                        upvars.push((name.clone(), t, Some(mutable), base_rty));
                    }
                }
            }
        }
        let ret_ty = match self.ty_at(c.syntax()) {
            Some(Ty::Closure { ret, .. }) => self.env.rty_of_ty(&mut self.body, &ret.clone()),
            _ => RTy::opaque(),
        };
        // The closure's own body, checked on its own with the captures as references; what it
        // returns (in terms of this body's regions) is what calling it gives.
        let ret_ty = self.closure_body(c, &body, &params, &upvars, ret_ty, &hints);
        let has_captures = !ops.is_empty();
        let rty = RTy::Closure {
            region: if has_captures { Some(region) } else { None },
            ret: Box::new(ret_ty),
        };
        self.ops(rty, ops, span, ctx)
    }

    fn closure_body(
        &mut self,
        c: &ast::ClosureExpr,
        body: &ast::Expr,
        params: &[ast::Param],
        upvars: &[(String, Local, Option<bool>, RTy)],
        ret: RTy,
        hints: &[(usize, RTy)],
    ) -> RTy {
        let name = format!("{}::{{closure}}", self.body.name);
        let mut lw = Lower::new(self.env, &name);
        lw.base = self.base;
        lw.self_adt = self.self_adt.clone();
        lw.inherit_tenv(&self.tenv);
        let span = lw.span(c.syntax());
        // Captures: `&var` (or `var`) with a universal region each; the variable's own regions
        // are the enclosing body's, so they become universal regions of this one.
        for (n, _, by_ref, base_rty) in upvars {
            let base_rty = lw.import_rty(base_rty);
            let rty = match by_ref {
                Some(m) => {
                    let u = lw.body.new_universal("'_");
                    RTy::Ref(u, *m, Box::new(base_rty))
                }
                None => base_rty,
            };
            let l = lw.body.new_local(n, rty, LocalKind::User, span);
            lw.upvars.insert(n.clone(), Upvar { local: l, by_ref: *by_ref });
        }
        // Parameters: written types with universal regions; else what the method the closure
        // is passed to hands it; else the type checker's view.
        for (pi, p) in params.iter().enumerate() {
            let Some(pat) = p.pat() else { continue };
            let pspan = lw.span(pat.syntax());
            let hint = hints.iter().find(|(i, _)| *i == pi).map(|(_, t)| t.clone());
            let rty = match (p.ty(), hint) {
                (Some(t), _) => {
                    let tenv = lw.tenv.clone();
                    lw.env.rty_from_syntax(&mut lw.body, &tenv, &t, &mut |b| b.new_universal("'_"))
                }
                (None, Some(h)) => lw.import_rty(&h),
                (None, None) => {
                    let t = lw.ty_at(pat.syntax()).cloned();
                    match t {
                        Some(Ty::MutRef(inner)) => {
                            let u = lw.body.new_universal("'_");
                            let inner = lw.env.rty_of_ty(&mut lw.body, &inner);
                            RTy::Ref(u, true, Box::new(inner))
                        }
                        Some(t) => lw.env.rty_of_ty(&mut lw.body, &t),
                        None => RTy::opaque(),
                    }
                }
            };
            crate::nll_rty::implied_bounds(&mut lw.body, &rty);
            match &pat {
                ast::Pat::IdentPat(ip) if ip.at_token().is_none() => {
                    let n = ip.name().map(|n| n.text().to_string()).unwrap_or_default();
                    lw.declare(&n, rty, pspan);
                }
                _ => {
                    let l = lw.declare("", rty.clone(), pspan);
                    lw.pattern(&pat, Place::local(l), rty, None, pspan);
                }
            }
        }
        let declared = c.ret_type().and_then(|r| r.ty()).map(|t| {
            let tenv = lw.tenv.clone();
            lw.env.rty_from_syntax(&mut lw.body, &tenv, &t, &mut |b| b.new_universal("'_"))
        });
        // (The type checker's view was made with the enclosing body's regions, and constrains
        // nothing: fresh regions.)
        let ret = lw.import_fresh(&ret);
        lw.ret_rty = declared.clone().unwrap_or(ret.clone());
        lw.body.locals[lw.ret_local.idx()].rty = lw.ret_rty.clone();
        let bspan = lw.span(body.syntax());
        let ex = lw.expr(body, Ctx::default());
        // What the body produces, seen from the enclosing body.
        let produced = match &declared {
            Some(d) => d.clone(),
            None if ex.rty.has_regions() => ex.rty.clone(),
            None => ret,
        };
        let out = lw.export_rty(&produced, &mut self.body, span);
        if lw.reachable {
            let ex = lw.materialize(ex, Ctx::default());
            lw.ret_value(ex, bspan, false);
        }
        if lw.too_big {
            self.too_big = true;
            return out;
        }
        self.closures.push(lw.body);
        self.closures.append(&mut lw.closures);
        out
    }

    /// The parameter types a std method gives the closure it takes as argument `i`
    /// (`iter.map(|item| ..)`, `v.retain(|x: &T| ..)`, `opt.map(|v| ..)`, `fold(init, |acc, x| ..)`).
    fn closure_hints(&mut self, base: &RTy, name: &str, i: usize, prev: &[Ex]) -> Vec<(usize, RTy)> {
        let r = |t: RTy, m: bool, lw: &mut Self| {
            let region = lw.fresh();
            RTy::Ref(region, m, Box::new(t))
        };
        match base {
            RTy::Iter { elem, .. } => {
                let e = (**elem).clone();
                match (name, i) {
                    (
                        "map" | "for_each" | "flat_map" | "filter_map" | "map_while" | "any"
                        | "all" | "find_map" | "position" | "rposition" | "try_for_each"
                        | "partition" | "max_by_key" | "min_by_key" | "is_sorted_by_key"
                        | "sum_by" | "group_by" | "chunk_by",
                        0,
                    ) => {
                        if matches!(name, "partition" | "max_by_key" | "min_by_key" | "is_sorted_by_key") {
                            vec![(0, r(e, false, self))]
                        } else {
                            vec![(0, e)]
                        }
                    }
                    ("filter" | "find" | "take_while" | "skip_while" | "inspect", 0) => {
                        vec![(0, r(e, false, self))]
                    }
                    ("max_by" | "min_by" | "is_sorted_by", 0) => {
                        vec![(0, r(e.clone(), false, self)), (1, r(e, false, self))]
                    }
                    ("reduce", 0) => vec![(0, e.clone()), (1, e)],
                    ("fold" | "try_fold", 1) => {
                        let init = prev.first().map(|p| p.rty.clone()).unwrap_or(RTy::opaque());
                        vec![(0, init), (1, e)]
                    }
                    ("scan", 1) => {
                        let init = prev.first().map(|p| p.rty.clone()).unwrap_or(RTy::opaque());
                        vec![(0, r(init, true, self)), (1, e)]
                    }
                    ("and_modify", 0) => vec![(0, r(e, true, self))],
                    _ => Vec::new(),
                }
            }
            RTy::Vec(e) => {
                let e = (**e).clone();
                match (name, i) {
                    ("retain" | "sort_by_key" | "sort_unstable_by_key" | "sort_by_cached_key"
                    | "binary_search_by" | "binary_search_by_key" | "partition_point"
                    | "iter_position", 0) => vec![(0, r(e, false, self))],
                    ("retain_mut" | "dedup_by_key" | "extract_if", 0) => vec![(0, r(e, true, self))],
                    ("sort_by" | "sort_unstable_by" | "dedup_by", 0) => {
                        vec![(0, r(e.clone(), false, self)), (1, r(e, false, self))]
                    }
                    _ => Vec::new(),
                }
            }
            RTy::Set(e) => match (name, i) {
                ("retain", 0) => vec![(0, r((**e).clone(), false, self))],
                _ => Vec::new(),
            },
            RTy::Map(k, v) => match (name, i) {
                ("retain", 0) => vec![
                    (0, r((**k).clone(), false, self)),
                    (1, r((**v).clone(), true, self)),
                ],
                _ => Vec::new(),
            },
            RTy::Option(t) => {
                let t = (**t).clone();
                match (name, i) {
                    ("map" | "and_then" | "is_some_and" | "is_none_or", 0) => vec![(0, t)],
                    ("map_or" | "map_or_else", 1) => vec![(0, t)],
                    ("filter" | "inspect", 0) => vec![(0, r(t, false, self))],
                    ("take_if", 0) => vec![(0, r(t, true, self))],
                    _ => Vec::new(),
                }
            }
            RTy::Result(t, e) => {
                let (t, e) = ((**t).clone(), (**e).clone());
                match (name, i) {
                    ("map" | "and_then" | "is_ok_and", 0) => vec![(0, t)],
                    ("map_err" | "or_else" | "is_err_and" | "unwrap_or_else", 0) => vec![(0, e)],
                    ("map_or" | "map_or_else", 1) => vec![(0, t)],
                    ("map_or_else", 0) => vec![(0, e)],
                    ("inspect", 0) => vec![(0, r(t, false, self))],
                    ("inspect_err", 0) => vec![(0, r(e, false, self))],
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// The enclosing function's lifetime and type parameter names, for annotations inside a
    /// closure, with their regions imported.
    fn inherit_tenv(&mut self, outer: &TyEnv) {
        let mut tenv = TyEnv::default();
        for (name, r) in &outer.lifetimes {
            let imported = self.import_rty(&RTy::Ref(*r, false, Box::new(RTy::opaque())));
            if let RTy::Ref(r2, ..) = imported {
                tenv.lifetimes.insert(name.clone(), r2);
            }
        }
        for (name, t) in &outer.params {
            tenv.params.insert(name.clone(), self.import_rty(t));
        }
        tenv.self_ty = outer.self_ty.as_ref().map(|t| self.import_rty(t));
        self.tenv = tenv;
    }

    /// A type from the enclosing body, with its regions replaced by universal regions of this
    /// one (`'static` stays).
    fn import_rty(&mut self, t: &RTy) -> RTy {
        let imported = &mut self.imported;
        let body = &mut self.body;
        t.map_regions(&mut |r| {
            if r == Region(0) {
                return r;
            }
            *imported.entry(r).or_insert_with(|| body.new_universal("'_"))
        })
    }

    /// A type from the enclosing body with every region a fresh inference region of this one:
    /// a shape that constrains nothing.
    fn import_fresh(&mut self, t: &RTy) -> RTy {
        let body = &mut self.body;
        t.map_regions(&mut |r| if r == Region(0) { r } else { body.new_region() })
    }

    /// The reverse of `import_rty`, for what a closure returns: its universal regions back to
    /// the enclosing body's. A region of the body's own (`s.as_str()` borrows through the
    /// parameter `s`) becomes a fresh region outlived by the counterparts of the universal
    /// regions that outlive it; one nothing outlives (a local's) is a fresh region on its own.
    fn export_rty(&self, t: &RTy, outer: &mut Body, span: Span) -> RTy {
        let back: HashMap<Region, Region> = self.imported.iter().map(|(o, u)| (*u, *o)).collect();
        // Which universal regions outlive each region (through the constraints).
        let n = self.body.regions.len();
        let mut above: Vec<Vec<Region>> = vec![Vec::new(); n];
        for i in 0..n {
            if self.body.regions[i].universal.is_some() && i != 0 {
                above[i].push(Region(i as u32));
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for c in &self.body.constraints {
                let from = above[c.longer.idx()].clone();
                for u in from {
                    if !above[c.shorter.idx()].contains(&u) {
                        above[c.shorter.idx()].push(u);
                        changed = true;
                    }
                }
            }
        }
        let mut fresh: HashMap<Region, Region> = HashMap::new();
        t.map_regions(&mut |r| {
            if r == Region(0) {
                return r;
            }
            if let Some(o) = back.get(&r) {
                return *o;
            }
            *fresh.entry(r).or_insert_with(|| {
                let out = outer.new_region();
                for u in &above[r.idx()] {
                    if let Some(o) = back.get(u) {
                        outer.outlives(*o, out, span);
                    }
                }
                out
            })
        })
    }

    /// The places a closure body captures from the enclosing scopes, with how (RFC 2229:
    /// field paths, by reference unless moved or `move`).
    fn captures(&self, body: &SyntaxNode, params: &[ast::Param], is_move: bool) -> Vec<(String, Vec<String>, CapMode)> {
        // Names bound inside the closure shadow the enclosing ones (approximately: any such
        // name isn't a capture).
        let mut inner: std::collections::HashSet<String> = std::collections::HashSet::new();
        for p in params {
            if let Some(pat) = p.pat() {
                for ip in pat.syntax().descendants().filter_map(ast::IdentPat::cast) {
                    if let Some(n) = ip.name() {
                        inner.insert(n.text().to_string());
                    }
                }
            }
        }
        for ip in body.descendants().filter_map(ast::IdentPat::cast) {
            if let Some(n) = ip.name() {
                inner.insert(n.text().to_string());
            }
        }
        let mut out: Vec<(String, Vec<String>, CapMode)> = Vec::new();
        for p in body.descendants().filter_map(ast::PathExpr::cast) {
            let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
                continue;
            };
            let name = name.text().to_string();
            if inner.contains(&name) && name != "self" {
                continue;
            }
            let known = self.lookup(&name).is_some() || self.upvars.contains_key(&name);
            if !known {
                continue;
            }
            // The place used: the longest chain of field accesses from the variable.
            let mut node: SyntaxNode = p.syntax().clone();
            let mut proj: Vec<String> = Vec::new();
            loop {
                let Some(parent) = node.parent() else { break };
                match parent.kind() {
                    ra_ap_syntax::SyntaxKind::PAREN_EXPR => node = parent,
                    ra_ap_syntax::SyntaxKind::FIELD_EXPR => {
                        let fe = ast::FieldExpr::cast(parent.clone()).unwrap();
                        if fe.expr().is_some_and(|e| e.syntax() == &node) {
                            // (A field through a reference is captured by reference to the
                            // referent; through an unknown type, stop at the variable.)
                            if let Some(n) = fe.name_ref() {
                                proj.push(n.text().to_string());
                            }
                            node = parent;
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            // Only field paths through owned structs: through a reference the whole
            // reference's referent is captured (the path is still fine); through an `Opaque`
            // (unknown) base, keep the variable only.
            let mut mode = if is_move { CapMode::Value } else { self.use_mode(&node, &name) };
            if is_move {
                proj.clear();
            }
            // A `&mut` / `&` local itself used by value in a `move` closure moves the
            // reference; by reference captures reborrow (fine as is).
            if mode == CapMode::Value && !is_move {
                // By-value use of a `Copy` value is a read.
                mode = CapMode::Shared;
                let rty = self.lookup(&name).map(|l| self.body.locals[l.idx()].rty.clone());
                if let Some(rty) = rty {
                    // The projected type: moved if owned and not `Copy`.
                    let mut t = rty;
                    for f in &proj {
                        t = match &t {
                            RTy::Adt(..) => {
                                let mut b = Body::default();
                                self.env.field_rty(&mut b, "", f, &t).unwrap_or(RTy::opaque())
                            }
                            RTy::Tuple(es) => f.parse::<usize>().ok().and_then(|i| es.get(i).cloned()).unwrap_or(RTy::opaque()),
                            _ => RTy::opaque(),
                        };
                    }
                    if self.env.moves(&t) {
                        mode = CapMode::Value;
                    }
                }
            }
            // Merge with earlier captures of the same variable: prefixes subsume.
            if let Some(prev) = out.iter_mut().find(|(n, pr, _)| {
                *n == name && (pr.iter().zip(&proj).all(|(a, b)| a == b))
            }) {
                if proj.len() < prev.1.len() {
                    prev.1 = proj.clone();
                }
                if mode > prev.2 {
                    prev.2 = mode;
                }
            } else {
                out.push((name, proj, mode));
            }
        }
        out
    }

    /// How a use (the place expression node `n`, rooted at variable `name`) treats the place:
    /// mutated, moved, or read.
    fn use_mode(&self, n: &SyntaxNode, name: &str) -> CapMode {
        let Some(parent) = n.parent() else {
            return CapMode::Shared;
        };
        let is_first = |p: &SyntaxNode| p.first_child().is_some_and(|c| &c == n);
        match parent.kind() {
            ra_ap_syntax::SyntaxKind::REF_EXPR => {
                let r = ast::RefExpr::cast(parent).unwrap();
                if r.mut_token().is_some() { CapMode::Mut } else { CapMode::Shared }
            }
            ra_ap_syntax::SyntaxKind::BIN_EXPR => {
                let b = ast::BinExpr::cast(parent.clone()).unwrap();
                match b.op_kind() {
                    Some(ast::BinaryOp::Assignment { .. }) if is_first(&parent) => CapMode::Mut,
                    Some(ast::BinaryOp::CmpOp(_)) | Some(ast::BinaryOp::LogicOp(_)) => CapMode::Shared,
                    Some(ast::BinaryOp::Assignment { .. }) => CapMode::Value,
                    _ => CapMode::Value,
                }
            }
            ra_ap_syntax::SyntaxKind::METHOD_CALL_EXPR => {
                let m = ast::MethodCallExpr::cast(parent.clone()).unwrap();
                if !m.receiver().is_some_and(|r| r.syntax() == n) {
                    return CapMode::Value;
                }
                let method = m.name_ref().map(|x| x.text().to_string()).unwrap_or_default();
                let rty = self.lookup(name).map(|l| self.body.locals[l.idx()].rty.clone()).unwrap_or(RTy::opaque());
                let base = rty.strip_refs().clone();
                let mode = match &base {
                    RTy::Adt(adt, ..) => match self.env.methods.get(&(adt.clone(), method.clone())) {
                        Some(Some(fs)) => match fs.f.param_list().and_then(|l| l.self_param()) {
                            Some(sp) if sp.amp_token().is_some() && sp.mut_token().is_some() => Recv::MutRef,
                            Some(sp) if sp.amp_token().is_some() => Recv::Ref,
                            Some(sp) => match sp.ty() {
                                Some(ast::Type::RefType(r)) if r.mut_token().is_some() => Recv::MutRef,
                                Some(ast::Type::RefType(_)) => Recv::Ref,
                                _ => Recv::Value,
                            },
                            None => Recv::Ref,
                        },
                        _ => nll_std::recv_mode(&base, &method),
                    },
                    _ => nll_std::recv_mode(&base, &method),
                };
                match mode {
                    Recv::MutRef => CapMode::Mut,
                    Recv::Ref => CapMode::Shared,
                    Recv::Value => {
                        if rty.is_ref() { CapMode::Shared } else { CapMode::Value }
                    }
                }
            }
            ra_ap_syntax::SyntaxKind::INDEX_EXPR => {
                // `v[i] = x` / `v[i].push(..)` mutate `v`; `v[i]` reads.
                let ix = ast::IndexExpr::cast(parent.clone()).unwrap();
                if ix.base().is_some_and(|b| b.syntax() == n) {
                    match self.use_mode(&parent, name) {
                        CapMode::Mut => CapMode::Mut,
                        _ => CapMode::Shared,
                    }
                } else {
                    CapMode::Shared
                }
            }
            ra_ap_syntax::SyntaxKind::PREFIX_EXPR => {
                let p = ast::PrefixExpr::cast(parent.clone()).unwrap();
                if p.op_kind() == Some(ast::UnaryOp::Deref) {
                    match self.use_mode(&parent, name) {
                        CapMode::Mut => CapMode::Mut,
                        CapMode::Value => CapMode::Shared,
                        m => m,
                    }
                } else {
                    CapMode::Shared
                }
            }
            ra_ap_syntax::SyntaxKind::FIELD_EXPR => match self.use_mode(&parent, name) {
                CapMode::Value => CapMode::Value,
                m => m,
            },
            ra_ap_syntax::SyntaxKind::PAREN_EXPR => self.use_mode(&parent, name),
            ra_ap_syntax::SyntaxKind::MATCH_EXPR
            | ra_ap_syntax::SyntaxKind::LET_EXPR
            | ra_ap_syntax::SyntaxKind::FOR_EXPR => {
                // A scrutinee or iterable: by value when it moves (owned, not `Copy`).
                CapMode::Value
            }
            ra_ap_syntax::SyntaxKind::MACRO_CALL | ra_ap_syntax::SyntaxKind::TOKEN_TREE => CapMode::Shared,
            ra_ap_syntax::SyntaxKind::ARG_LIST
            | ra_ap_syntax::SyntaxKind::LET_STMT
            | ra_ap_syntax::SyntaxKind::RETURN_EXPR
            | ra_ap_syntax::SyntaxKind::RECORD_EXPR_FIELD
            | ra_ap_syntax::SyntaxKind::TUPLE_EXPR
            | ra_ap_syntax::SyntaxKind::ARRAY_EXPR
            | ra_ap_syntax::SyntaxKind::STMT_LIST
            | ra_ap_syntax::SyntaxKind::BREAK_EXPR
            | ra_ap_syntax::SyntaxKind::CLOSURE_EXPR => CapMode::Value,
            _ => CapMode::Shared,
        }
    }

    fn async_block(&mut self, b: &ast::BlockExpr, ctx: Ctx) -> Ex {
        // `async { .. }` / `async move { .. }`: a closure with no parameters, run later.
        let span = self.span(b.syntax());
        let is_move = b.syntax().children_with_tokens().any(|t| t.kind() == T![move]);
        let captures = self.captures(b.syntax(), &[], is_move);
        let region = self.fresh();
        let mut ops = Vec::new();
        let mut upvars: Vec<(String, Local, Option<bool>, RTy)> = Vec::new();
        for (name, proj, mode) in &captures {
            let Some(l) = self.lookup(name) else { continue };
            let base_rty = self.body.locals[l.idx()].rty.clone();
            let mut place = Place::local(l);
            for f in proj {
                place = place.field(f.clone());
            }
            let prty = self.place_rty(&place);
            match mode {
                CapMode::Value => {
                    let ex = Ex { place: place.clone(), rty: prty.clone(), span, is_place: true };
                    let op = self.operand(&ex);
                    let t = self.temp(prty.clone(), span);
                    self.emit(StmtKind::Assign(Place::local(t), Rvalue::Use(op)), span);
                    self.env.hold(&mut self.body, &prty, region, span);
                    ops.push(Operand { place: Place::local(t), moves: self.env.moves(&prty), span });
                    if !upvars.iter().any(|u| u.0 == *name) {
                        upvars.push((name.clone(), t, None, base_rty));
                    }
                }
                CapMode::Shared | CapMode::Mut => {
                    let mutable = *mode == CapMode::Mut;
                    let r = self.fresh();
                    let t = self.borrow(r, mutable, place, prty, span, false);
                    self.body.outlives(r, region, span);
                    ops.push(Operand { place: Place::local(t), moves: false, span });
                    if !upvars.iter().any(|u| u.0 == *name) {
                        upvars.push((name.clone(), t, Some(mutable), base_rty));
                    }
                }
            }
        }
        let ret = self.rty_at(b.syntax());
        let out = self.ops(
            RTy::Closure { region: if ops.is_empty() { None } else { Some(region) }, ret: Box::new(ret.clone()) },
            ops,
            span,
            ctx,
        );
        // The block's body as its own function.
        let name = format!("{}::{{async}}", self.body.name);
        let mut lw = Lower::new(self.env, &name);
        lw.base = self.base;
        lw.self_adt = self.self_adt.clone();
        lw.inherit_tenv(&self.tenv);
        for (n, _, by_ref, base_rty) in &upvars {
            let base_rty = lw.import_rty(base_rty);
            let rty = match by_ref {
                Some(m) => {
                    let u = lw.body.new_universal("'_");
                    RTy::Ref(u, *m, Box::new(base_rty))
                }
                None => base_rty,
            };
            let l = lw.body.new_local(n, rty, LocalKind::User, span);
            lw.upvars.insert(n.clone(), Upvar { local: l, by_ref: *by_ref });
        }
        let ret = lw.import_fresh(&ret);
        lw.ret_rty = ret.clone();
        lw.body.locals[lw.ret_local.idx()].rty = ret;
        let ex = lw.block_inner(b, Ctx::default());
        if lw.reachable {
            lw.ret_value(ex, span, false);
        }
        if !lw.too_big {
            self.closures.push(lw.body);
            self.closures.append(&mut lw.closures);
        }
        out
    }
}

/// An identifier pattern that names a constant or a unit variant rather than binding (`None`,
/// `Color::Red` written as `Red`, `MAX`): by convention, a capitalized name with no `ref`,
/// `mut` or `@`.
fn is_const_pattern(ip: &ast::IdentPat) -> bool {
    ip.ref_token().is_none()
        && ip.mut_token().is_none()
        && ip.at_token().is_none()
        && ip
            .name()
            .is_some_and(|n| n.text().chars().next().is_some_and(|c| c.is_ascii_uppercase()))
}

/// A constant expression (literals, and tuples, arrays and `&` of them), which rustc promotes
/// to a `'static` so a borrow of it never dangles.
fn is_constant(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Literal(_) => true,
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| is_constant(&i)),
        ast::Expr::RefExpr(r) => r.expr().is_some_and(|i| is_constant(&i)),
        ast::Expr::PrefixExpr(p) => p.expr().is_some_and(|i| is_constant(&i)),
        ast::Expr::TupleExpr(t) => t.fields().all(|f| is_constant(&f)),
        ast::Expr::ArrayExpr(a) => match a.kind() {
            ast::ArrayExprKind::Repeat {
                initializer,
                repeat,
            } => {
                initializer.is_some_and(|i| is_constant(&i))
                    && repeat.is_some_and(|n| is_constant(&n))
            }
            ast::ArrayExprKind::ElementList(es) => es.into_iter().all(|f| is_constant(&f)),
        },
        _ => false,
    }
}

/// The `{name}` captures of a format string.
fn format_captures(fs: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = fs.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            if b.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j > start && b[start].is_ascii_alphabetic() || (j > start && b[start] == b'_') {
                let name = &fs[start..j];
                if matches!(b.get(j), Some(b'}') | Some(b':')) && !out.iter().any(|n| n == name) {
                    out.push(name.to_string());
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Lower every function of the file.
pub fn lower_file(env: &Env, file: &ast::SourceFile, generated: &dyn Fn(usize) -> bool) -> Vec<Body> {
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut lower = |fs: FnSyn, out: &mut Vec<Body>| {
        let start = usize::from(fs.f.syntax().text_range().start());
        if !seen.insert(start) || crate::adt::test_only(fs.f.syntax()) || generated(start) {
            return;
        }
        if let Some(bodies) = Lower::function(env, &fs) {
            out.extend(bodies);
        }
    };
    for fs in env.free_fns.values().flatten() {
        lower(fs.clone(), &mut out);
    }
    for fs in env.methods.values().flatten() {
        lower(fs.clone(), &mut out);
    }
    // Everything else with a body: same-named functions, trait default methods, nested ones.
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        if f.body().is_none() {
            continue;
        }
        let imp = f
            .syntax()
            .parent()
            .and_then(|p| p.parent())
            .and_then(ast::Impl::cast)
            .map(|imp| {
                let (lifetimes, type_params) = generics_of(imp.generic_param_list());
                crate::nll_rty::ImplSyn {
                    lifetimes,
                    type_params,
                    self_ty: imp.self_ty(),
                    outlives: Vec::new(),
                }
            });
        lower(
            FnSyn {
                f: f.clone(),
                imp,
            },
            &mut out,
        );
    }
    out
}
