//! Move checking: a use of a binding after its value was moved is an error, as in rustc. Also
//! definite initialization: a `let x;` must be assigned on every path before it's used, and only
//! once unless it's `mut`.
//!
//! In JS a "moved" object is still reachable through the old name, so without this check code that
//! rustc rejects would run with shared-reference semantics. The checker is deliberately
//! conservative (types are partly erased, so references and owned values can look alike): it
//! tracks only bindings that certainly own a non-`Copy` value, and only the moves listed in
//! `Walker::expr`. Control flow is followed: branches are checked separately and joined, loop
//! bodies run twice (to catch a move in a previous iteration), and assigning a new value to a
//! moved binding makes it usable again.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasLoopBody, HasName},
};

use crate::adt::Adts;
use crate::lineindex::LineIndex;
use crate::macros;
use crate::ty::Ty;
use crate::typeck::RawDiag;

/// A binding, by its name's absolute range.
type Decl = (usize, usize);

#[derive(Clone, Default)]
struct State {
    /// Moved bindings: where, and how (`UNINIT` for a `let x;` not yet assigned).
    moved: HashMap<Decl, (usize, &'static str)>,
    /// Moved bindings that are only partly moved (`let S { name, .. } = s;`): the fields that
    /// moved. Reading another field (`s.x`) is still fine.
    partial: HashMap<Decl, HashSet<String>>,
    /// `let x;` bindings that may have been assigned already.
    assigned: HashSet<Decl>,
    /// `let x;` bindings assigned on no path yet.
    unassigned: HashSet<Decl>,
    /// After `return`/`break`/`continue`: nothing below runs.
    unreachable: bool,
}

/// How a `let x;` binding without a value is "moved": it has no value yet.
const UNINIT: &str = "declared without a value";

impl State {
    fn join(a: State, b: State) -> State {
        if a.unreachable {
            return b;
        }
        if b.unreachable {
            return a;
        }
        // A binding partly moved on both paths (or partly on one, not at all on the other) is
        // partly moved; fully moved on either path, fully.
        let mut partial = HashMap::new();
        for (k, _) in a.moved.iter().chain(&b.moved) {
            let (pa, pb) = (a.partial.get(k), b.partial.get(k));
            let full = (a.moved.contains_key(k) && pa.is_none())
                || (b.moved.contains_key(k) && pb.is_none());
            if !full {
                let fields: HashSet<String> = pa.into_iter().chain(pb).flatten().cloned().collect();
                partial.insert(*k, fields);
            }
        }
        let mut moved = a.moved;
        for (k, v) in b.moved {
            moved.entry(k).or_insert(v);
        }
        let mut assigned = a.assigned;
        assigned.extend(b.assigned);
        let unassigned = a.unassigned.intersection(&b.unassigned).copied().collect();
        State {
            moved,
            partial,
            assigned,
            unassigned,
            unreachable: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Use,
    Move(&'static str),
}

struct Walker<'a> {
    src: &'a str,
    types: &'a HashMap<(usize, usize), Ty>,
    adts: &'a Adts,
    /// Bindings of fixed-size arrays of `Copy` values: copied, never moved.
    copy_arrays: &'a HashSet<Decl>,
    /// Functions declared in the file, by impl type (`None` for free functions) and name: whether
    /// they certainly return an owned value (no references anywhere in the return type).
    fns: HashMap<(Option<String>, String), bool>,
    /// The type of the `impl` being checked, for `Self::new()`.
    cur_impl: Option<String>,
    base: usize,
    scopes: Vec<HashMap<String, Decl>>,
    /// Bindings that own a non-`Copy` value.
    tracked: HashSet<Decl>,
    /// `let x;` bindings (declared without a value), and whether they're `mut`.
    deferred: HashMap<Decl, bool>,
    /// Closures in bindings: their moves are recorded (a closure may own values with a `Drop`
    /// impl) but never reported, as many closures are `Copy`.
    soft: HashSet<Decl>,
    names: HashMap<Decl, String>,
    reported: HashSet<(Decl, usize)>,
    diags: Vec<RawDiag>,
    /// Enclosing loops and labeled blocks, innermost last.
    targets: Vec<Target>,
    /// Every use that moves a tracked binding.
    move_sites: HashSet<(usize, usize)>,
    /// Per function (by impl type and name): whether it takes `self` by reference, and which
    /// parameters are declared `&T`. An argument there is borrowed, not moved (typeck reports
    /// an owned one as rustc's E0308 where it's certain; here the move isn't recorded).
    ref_params: HashMap<(Option<String>, String), (Option<bool>, Vec<bool>)>,
    /// Struct fields declared `String` or `Box<..>`, by struct and field name (or tuple index).
    owned_fields: HashSet<(String, String)>,
    /// The same for enum variants, by variant name: whether every variant of that name agrees.
    owned_variant_fields: HashMap<(String, String), bool>,
    /// Functions whose return type holds an owned `String` (`String`, `Option<String>`, ..).
    string_fns: HashSet<(Option<String>, String)>,
    /// Tracked bindings whose strings are `String`s, not `&str`s (types alone don't tell): a
    /// string bound out of them by a pattern moves.
    strings: HashSet<Decl>,
    /// Closures that moved a capture, by where they start: where the capture is used in them.
    closure_uses: HashMap<usize, usize>,
    /// Pattern bindings that moved part of a value, by where they are: the moved part's type.
    pattern_moves: HashMap<usize, (String, Option<String>)>,
    /// While reading the binding under a field taken by value (`let w = p.v;`): a use of a
    /// moved field there is a use, not a borrow.
    field_by_value: bool,
}

/// How a pattern binding moves (part of) the value it destructures.
const BY_PATTERN: &str = "by binding it in a pattern";
/// How a closure that uses a capture by value takes it.
const INTO_CLOSURE: &str = "into a closure";
const INTO_MOVE_CLOSURE: &str = "into a `move` closure";

/// A pattern move: the binding moved out of, where, the part's type and field (`top_field`),
/// and the fields moved (`None`: the whole value).
type PatMove = (Decl, usize, (String, Option<String>), Option<HashSet<String>>);

/// What a destructuring pattern matches (see `Walker::binds_move`).
#[derive(Clone, Copy)]
struct PatCtx {
    /// The value's strings are `String`s (a tracked binding in `Walker::strings`).
    strings: bool,
    /// A binding whose type isn't known moves.
    unknown_moves: bool,
}

/// Whether `t` has a string in it (a `String`, when the value is known to own its strings).
fn has_str(t: &Ty) -> bool {
    match t {
        Ty::Str => true,
        Ty::Option(x) => has_str(x),
        Ty::Result(a, b) => has_str(a) || has_str(b),
        Ty::Tuple(es) => es.iter().any(has_str),
        _ => false,
    }
}

/// Whether a declared type certainly holds an owned `String`: `String`, `Option<String>`,
/// `Result<String, E>`, a tuple with one; no references, functions or trait objects.
fn string_ty(t: &str) -> bool {
    let has_word = t.match_indices("String").any(|(i, _)| {
        !t[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_')
            && !t[i + 6..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
    });
    has_word
        && ["String", "Option<", "Result<", "("].iter().any(|p| t.starts_with(p))
        && !["&", "fn(", "Fn", "impl ", "dyn "].iter().any(|x| t.contains(x))
}

/// Where `break`/`continue` go: the states they leave with.
struct Target {
    label: Option<String>,
    is_loop: bool,
    breaks: Vec<State>,
    continues: Vec<State>,
}

/// Check every function and method body in `file`.
/// Also returns where bindings are moved (each use that moves its value), which codegen needs
/// for `Drop`: a moved value is dropped by its new owner, not at the end of its old scope.
pub fn check(
    file: &ast::SourceFile,
    src: &str,
    types: &HashMap<(usize, usize), Ty>,
    adts: &Adts,
    copy_arrays: &HashSet<Decl>,
) -> (Vec<RawDiag>, HashSet<(usize, usize)>) {
    let mut fns = HashMap::new();
    let mut ref_params = HashMap::new();
    let mut string_fns: HashMap<(Option<String>, String), bool> = HashMap::new();
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(n) = f.name() else { continue };
        let owned = f.ret_type().and_then(|r| r.ty()).is_some_and(|t| {
            !t.syntax().text().contains_char('&') && !matches!(t, ast::Type::ImplTraitType(_))
        });
        let string = f
            .ret_type()
            .and_then(|r| r.ty())
            .is_some_and(|t| string_ty(&t.syntax().text().to_string()));
        string_fns
            .entry((impl_of(&f), n.text().to_string()))
            .and_modify(|s| *s &= string)
            .or_insert(string);
        // Same-named functions that disagree (in different modules) aren't trusted.
        fns.entry((impl_of(&f), n.text().to_string()))
            .and_modify(|o| *o &= owned)
            .or_insert(owned);
        let self_ref = f
            .param_list()
            .and_then(|pl| pl.self_param())
            .map(|sp| sp.amp_token().is_some());
        let refs: Vec<bool> = f
            .param_list()
            .map(|pl| {
                pl.params()
                    .map(|p| matches!(p.ty(), Some(ast::Type::RefType(_))))
                    .collect()
            })
            .unwrap_or_default();
        ref_params
            .entry((impl_of(&f), n.text().to_string()))
            .and_modify(|(s, ps): &mut (Option<bool>, Vec<bool>)| {
                if *s != self_ref {
                    *s = None;
                }
                for (a, b) in ps.iter_mut().zip(&refs) {
                    *a &= *b;
                }
                ps.truncate(refs.len());
            })
            .or_insert((self_ref, refs));
    }
    let owned_ty = |t: Option<ast::Type>| {
        t.is_some_and(|t| {
            let t = t.syntax().text().to_string();
            t == "String" || t.starts_with("Box<")
        })
    };
    let mut owned_fields = HashSet::new();
    for s in file.syntax().descendants().filter_map(ast::Struct::cast) {
        let Some(sn) = s.name().map(|n| n.text().to_string()) else {
            continue;
        };
        match s.field_list() {
            Some(ast::FieldList::RecordFieldList(l)) => {
                for f in l.fields() {
                    if let Some(n) = f.name()
                        && owned_ty(f.ty())
                    {
                        owned_fields.insert((sn.clone(), n.text().to_string()));
                    }
                }
            }
            Some(ast::FieldList::TupleFieldList(l)) => {
                for (i, f) in l.fields().enumerate() {
                    if owned_ty(f.ty()) {
                        owned_fields.insert((sn.clone(), i.to_string()));
                    }
                }
            }
            None => {}
        }
    }
    let mut owned_variant_fields: HashMap<(String, String), bool> = HashMap::new();
    for v in file.syntax().descendants().filter_map(ast::Variant::cast) {
        let Some(vn) = v.name().map(|n| n.text().to_string()) else {
            continue;
        };
        let fields: Vec<(String, Option<ast::Type>)> = match v.field_list() {
            Some(ast::FieldList::RecordFieldList(l)) => l
                .fields()
                .filter_map(|f| Some((f.name()?.text().to_string(), f.ty())))
                .collect(),
            Some(ast::FieldList::TupleFieldList(l)) => l
                .fields()
                .enumerate()
                .map(|(i, f)| (i.to_string(), f.ty()))
                .collect(),
            None => Vec::new(),
        };
        for (f, t) in fields {
            let owned = owned_ty(t);
            owned_variant_fields
                .entry((vn.clone(), f))
                .and_modify(|o| *o &= owned)
                .or_insert(owned);
        }
    }
    let mut w = Walker {
        src,
        types,
        adts,
        copy_arrays,
        fns,
        cur_impl: None,
        base: 0,
        scopes: Vec::new(),
        tracked: HashSet::new(),
        deferred: HashMap::new(),
        soft: HashSet::new(),
        names: HashMap::new(),
        reported: HashSet::new(),
        diags: Vec::new(),
        targets: Vec::new(),
        move_sites: HashSet::new(),
        ref_params,
        owned_fields,
        owned_variant_fields,
        string_fns: string_fns.into_iter().filter(|(_, s)| *s).map(|(k, _)| k).collect(),
        strings: HashSet::new(),
        closure_uses: HashMap::new(),
        pattern_moves: HashMap::new(),
        field_by_value: false,
    };
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        if !crate::adt::test_only(f.syntax()) {
            w.function(&f);
        }
    }
    (w.diags, w.move_sites)
}

impl Walker<'_> {
    fn range(&self, n: &SyntaxNode) -> (usize, usize) {
        let r = n.text_range();
        (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        )
    }

    fn ty(&self, n: &SyntaxNode) -> Option<&Ty> {
        self.types.get(&self.range(n))
    }

    /// `p.field` on a binding: the binding and the field's name (or tuple index).
    fn field_base(&self, f: &ast::FieldExpr) -> Option<(Decl, String)> {
        let ast::Expr::PathExpr(p) = strip(&f.expr()?) else {
            return None;
        };
        let d = self.lookup(p.path()?.as_single_name_ref()?.text().to_string().as_str())?;
        Some((d, f.name_ref()?.text().to_string()))
    }

    /// Whether reading the field `f` by value moves it: its type moves, or the struct declares it
    /// `String` / `Box<..>` (which types alone don't tell from `&str` / `&T`).
    fn field_moves(&self, f: &ast::FieldExpr) -> bool {
        if self.ty(f.syntax()).is_some_and(|t| self.moves(t)) {
            return true;
        }
        let Some(Ty::Adt(n, _)) = f.expr().and_then(|x| self.ty(x.syntax()).cloned()) else {
            return false;
        };
        let Some(field) = f.name_ref().map(|n| n.text().to_string()) else {
            return false;
        };
        let declared = self
            .adts
            .defs
            .get(&n)
            .filter(|d| !d.is_enum)
            .and_then(|d| d.variants.first()?.field(&field))
            .is_some_and(|(_, fd)| self.moves(&fd.ty));
        declared || self.owned_fields.contains(&(n, field))
    }

    /// The type of the binding `d`'s struct field `field`, as rustc writes it.
    fn field_ty(&self, d: Decl, field: &str) -> Option<String> {
        let Some(Ty::Adt(n, _)) = self.types.get(&d) else {
            return None;
        };
        let def = self.adts.defs.get(n).filter(|d| !d.is_enum)?;
        let (_, fd) = def.variants.first()?.field(field)?;
        Some(fd.ty.describe())
    }

    /// Whether values of `t` are moved (not copied) by assignment.
    fn moves(&self, t: &Ty) -> bool {
        match t {
            Ty::Vec(_)
            | Ty::Set(_)
            | Ty::Map(..)
            | Ty::Coll(..)
            | Ty::RefCell(_)
            | Ty::Dyn(_, _)
            | Ty::Iter { .. } => true,
            Ty::Adt(n, _) => self.adts.defs.get(n).is_some_and(|d| !d.derives("Copy")),
            Ty::Option(x) => self.moves(x),
            Ty::Result(a, b) => self.moves(a) || self.moves(b),
            Ty::Tuple(es) => es.iter().any(|e| self.moves(e)),
            _ => false,
        }
    }

    fn function(&mut self, f: &ast::Fn) {
        let Some(body) = f.body() else { return };
        self.cur_impl = impl_of(f);
        self.scopes = vec![HashMap::new()];
        if let Some(pl) = f.param_list() {
            self.self_param(&pl);
            for p in pl.params() {
                let (Some(ast::Pat::IdentPat(ip)), Some(t)) = (p.pat(), p.ty()) else {
                    continue;
                };
                let Some(n) = ip.name() else { continue };
                let decl = self.range(n.syntax());
                // By-value parameters own their argument; `&T` ones don't.
                let text = t.syntax().text().to_string();
                let owned = !text.contains('&')
                    && !matches!(t, ast::Type::ImplTraitType(_))
                    && (self.ty(n.syntax()).is_some_and(|ty| self.moves(ty)) || string_ty(&text));
                if owned && string_ty(&text) {
                    self.strings.insert(decl);
                }
                self.declare(&n, decl, owned);
                self.soft_closure(&n, decl, owned);
            }
        }
        let mut st = State::default();
        // (The body's value is moved out to the caller.)
        self.block(&body, &mut st, Mode::Move("by being returned"));
    }

    /// A method's by-value `self` (not `self: Box<Self>`) owns its value, like a parameter.
    fn self_param(&mut self, pl: &ast::ParamList) {
        let Some(sp) = pl.self_param() else { return };
        if sp.amp_token().is_some() || sp.ty().is_some() {
            return;
        }
        let Some(n) = sp.name() else { return };
        let owned = self
            .cur_impl
            .as_ref()
            .is_some_and(|i| self.moves(&Ty::Adt(i.clone(), Vec::new())));
        let decl = self.range(n.syntax());
        self.declare(&n, decl, owned);
    }

    /// A binding holding a closure: its moves are recorded, not checked (see `soft`).
    fn soft_closure(&mut self, n: &ast::Name, decl: Decl, owned: bool) {
        if !owned && matches!(self.ty(n.syntax()), Some(Ty::Closure { .. })) {
            self.soft.insert(decl);
        }
    }

    fn declare(&mut self, n: &ast::Name, decl: Decl, owned: bool) {
        let owned = owned && !self.copy_arrays.contains(&decl);
        self.scopes
            .last_mut()
            .unwrap()
            .insert(n.text().to_string(), decl);
        self.names.insert(decl, n.text().to_string());
        if owned {
            self.tracked.insert(decl);
        }
    }

    fn lookup(&self, name: &str) -> Option<Decl> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    /// Bind every name in a pattern (untracked: pattern bindings may be references).
    fn bind_pattern(&mut self, p: &ast::Pat) {
        for ip in p.syntax().descendants().filter_map(ast::IdentPat::cast) {
            if let Some(n) = ip.name() {
                let decl = self.range(n.syntax());
                self.declare(&n, decl, false);
            }
        }
    }

    /// What destructuring with `p` moves out of the value: `Some(fields)` names the top-level
    /// fields (empty when nothing moves), `None` is the whole value (or fields we can't name).
    /// `how` says what the value is (see `binds_move`).
    fn moved_fields(&self, p: &ast::Pat, how: PatCtx) -> Option<HashSet<String>> {
        let elems = |fields: Vec<ast::Pat>| {
            let rest = fields
                .iter()
                .position(|f| matches!(f, ast::Pat::RestPat(_)));
            let mut out = HashSet::new();
            for (i, f) in fields.iter().enumerate() {
                if self.binds_move(f, how) {
                    // (After a `..` the index depends on the arity.)
                    if rest.is_some_and(|r| i > r) {
                        return None;
                    }
                    out.insert(i.to_string());
                }
            }
            Some(out)
        };
        match p {
            ast::Pat::ParenPat(pp) => pp.pat().and_then(|i| self.moved_fields(&i, how)),
            ast::Pat::RecordPat(rp) => {
                let mut out = HashSet::new();
                for f in rp
                    .record_pat_field_list()
                    .into_iter()
                    .flat_map(|l| l.fields())
                {
                    let (Some(name), Some(sub)) = (f.field_name(), f.pat()) else {
                        continue;
                    };
                    if self.binds_move(&sub, how) {
                        out.insert(name.text().to_string());
                    }
                }
                Some(out)
            }
            ast::Pat::TupleStructPat(tp) => elems(tp.fields().collect()),
            ast::Pat::TuplePat(tp) => elems(tp.fields().collect()),
            other if self.binds_move(other, how) => None,
            _ => Some(HashSet::new()),
        }
    }

    /// Whether `p` binds a value that moves by value (not by `ref`, nor through a `&` pattern).
    fn binds_move(&self, p: &ast::Pat, how: PatCtx) -> bool {
        self.move_binding(p, how).is_some()
    }

    /// The first binding in `p` that moves its part of the value, and that part's type.
    fn move_binding(&self, p: &ast::Pat, how: PatCtx) -> Option<(ast::Name, String)> {
        p.syntax()
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .find_map(|ip| {
                if ip.ref_token().is_some()
                    || ip
                        .syntax()
                        .ancestors()
                        .take_while(|a| a != p.syntax())
                        .any(|a| ast::RefPat::can_cast(a.kind()))
                    || ast::RefPat::can_cast(p.syntax().kind())
                {
                    return None;
                }
                let n = ip.name()?;
                let t = self.ty(n.syntax());
                let moves = match t {
                    Some(t) => {
                        self.moves(t)
                            || (how.strings && has_str(t))
                            || (*t == Ty::Str && self.owned_field_binding(&ip))
                    }
                    // (Unknown: as before, assume it moves, where the caller says so.)
                    None => how.unknown_moves,
                };
                moves.then(|| (n, t.map_or_else(|| "String".into(), Ty::describe)))
            })
    }

    /// Whether `ip` binds a struct's or variant's field declared `String` / `Box<..>`
    /// (`P { name, .. }`, `Cmd::Say(text)`).
    fn owned_field_binding(&self, ip: &ast::IdentPat) -> bool {
        let Some(parent) = ip.syntax().parent() else {
            return false;
        };
        let last = |p: Option<ast::Path>| {
            p.and_then(|p| p.segment()?.name_ref())
                .map(|n| n.text().to_string())
        };
        let (owner, field) = if let Some(f) = ast::RecordPatField::cast(parent.clone()) {
            let Some(rp) = f.syntax().ancestors().find_map(ast::RecordPat::cast) else {
                return false;
            };
            let name = match f.field_name() {
                Some(n) => n.text().to_string(),
                None => return false,
            };
            (last(rp.path()), name)
        } else if let Some(tp) = ast::TupleStructPat::cast(parent) {
            let fields: Vec<ast::Pat> = tp.fields().collect();
            let Some(i) = fields.iter().position(|f| f.syntax() == ip.syntax()) else {
                return false;
            };
            // (After a `..` the index depends on the arity.)
            if fields[..i].iter().any(|f| matches!(f, ast::Pat::RestPat(_))) {
                return false;
            }
            (last(tp.path()), i.to_string())
        } else {
            return false;
        };
        let Some(owner) = owner else { return false };
        let key = (owner, field);
        match self.adts.defs.get(&key.0) {
            Some(d) if !d.is_enum => self.owned_fields.contains(&key),
            _ => self.owned_variant_fields.get(&key) == Some(&true),
        }
    }

    /// What a pattern destructures: whether its strings are `String`s, and whether to assume
    /// a binding of an unknown type moves.
    fn pat_ctx(&self, scrutinee: Option<&ast::Expr>, unknown_moves: bool) -> PatCtx {
        let strings = scrutinee
            .and_then(|e| self.binding_of(e))
            .is_some_and(|d| self.strings.contains(&d));
        PatCtx {
            strings,
            unknown_moves,
        }
    }

    /// The binding a plain path expression (`x`, `(x)`) names.
    fn binding_of(&self, e: &ast::Expr) -> Option<Decl> {
        let ast::Expr::PathExpr(p) = strip(e) else {
            return None;
        };
        self.lookup(p.path()?.as_single_name_ref()?.text().to_string().as_str())
    }

    /// What a `match` arm's or `if let`'s pattern moves out of a tracked binding it matches:
    /// the binding, where (the pattern binding that moved), the part's type, and the fields
    /// (`None`: the whole value).
    fn pattern_move(
        &self,
        p: &ast::Pat,
        scrutinee: &ast::Expr,
    ) -> Option<PatMove> {
        let d = self.binding_of(scrutinee)?;
        if !self.tracked.contains(&d) {
            return None;
        }
        let how = self.pat_ctx(Some(scrutinee), false);
        let fields = self.moved_fields(p, how);
        if fields.as_ref().is_some_and(|f| f.is_empty()) {
            return None;
        }
        let (n, t) = self.move_binding(p, how)?;
        let field = top_field(self.adts, p, &n);
        Some((d, self.range(n.syntax()).0, (t, field), fields))
    }

    /// Record a move that `pattern_move` found.
    fn apply_pattern_move(
        &mut self,
        m: Option<PatMove>,
        st: &mut State,
    ) {
        let Some((d, at, t, fields)) = m else { return };
        if st.unreachable || (st.moved.contains_key(&d) && !st.partial.contains_key(&d)) {
            return;
        }
        self.pattern_moves.insert(at, t);
        match fields {
            Some(f) => {
                st.moved.entry(d).or_insert((at, BY_PATTERN));
                st.partial.entry(d).or_default().extend(f);
            }
            None => {
                st.moved.insert(d, (at, BY_PATTERN));
                st.partial.remove(&d);
            }
        }
    }

    /// Whether `init` certainly produces an owned value (so the binding owns it).
    fn owned_init(&self, init: &ast::Expr) -> bool {
        match strip(init) {
            ast::Expr::RecordExpr(_) | ast::Expr::ArrayExpr(_) | ast::Expr::TupleExpr(_) => true,
            ast::Expr::MacroExpr(m) => m
                .macro_call()
                .and_then(|mc| macros::macro_name(&mc))
                .is_some_and(|n| matches!(n.as_str(), "vec" | "format")),
            ast::Expr::PathExpr(p) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()))
                .is_some_and(|d| self.tracked.contains(&d)),
            ast::Expr::CallExpr(c) => match c.expr() {
                Some(ast::Expr::PathExpr(p)) => {
                    let segs: Vec<String> = p
                        .path()
                        .map(|p| {
                            p.segments()
                                .filter_map(|s| s.name_ref())
                                .map(|n| n.text().to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    match segs.as_slice() {
                        [.., t, f]
                            if matches!(
                                t.as_str(),
                                "Vec" | "HashMap" | "HashSet" | "String" | "RefCell" | "Box"
                            ) && matches!(f.as_str(), "new" | "from" | "with_capacity") =>
                        {
                            true
                        }
                        // `Path::new(..)` (the path prelude's `PathBuf::__new` by now): a `&Path`.
                        [.., t, f] if t == "PathBuf" && f == "__new" => false,
                        // `Some(v)`, `Ok(s)`: owned when what they wrap is.
                        [f] if matches!(f.as_str(), "Some" | "Ok" | "Err") => {
                            self.wrapped(&c).is_some_and(|a| self.owned_init(&a))
                        }
                        // A tuple struct.
                        [f] if self.adts.defs.contains_key(f.as_str()) => true,
                        // A user function that doesn't return a reference.
                        [f] => self.fns.get(&(None, f.clone())).copied().unwrap_or(false),
                        [.., t, f] => {
                            let t = if t == "Self" {
                                self.cur_impl.clone().unwrap_or_default()
                            } else {
                                t.clone()
                            };
                            match self.fns.get(&(Some(t.clone()), f.clone())) {
                                Some(&o) => o,
                                // An enum variant, or a derived `default()`.
                                None => self.adts.defs.get(&t).is_some_and(|d| {
                                    f == "default" || d.variants.iter().any(|v| v.name == *f)
                                }),
                            }
                        }
                        _ => false,
                    }
                }
                _ => false,
            },
            ast::Expr::MethodCallExpr(m) => m.name_ref().is_some_and(|n| {
                matches!(
                    n.text().to_string().as_str(),
                    "clone"
                        | "to_string"
                        | "to_owned"
                        | "to_vec"
                        | "collect"
                        | "into_iter"
                        | "to_uppercase"
                        | "to_lowercase"
                ) || match m.receiver().and_then(|r| self.ty(r.syntax()).cloned()) {
                    Some(Ty::Adt(t, _)) => self
                        .fns
                        .get(&(Some(t), n.text().to_string()))
                        .copied()
                        .unwrap_or(false),
                    _ => false,
                }
            }),
            _ => false,
        }
    }

    /// A read of a binding; a move when `mode` says so.
    fn path(&mut self, p: &ast::PathExpr, st: &mut State, mode: Mode) {
        let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return;
        };
        let at = self.range(p.syntax());
        // (A closure given to a std method, `it.map(f)`, is a move too: codegen drops it at the
        // end of that statement, where Rust drops the adapter that owns it.)
        self.touch(name.text(), at, st, mode);
    }

    /// A read of the binding `name` at `at`.
    fn touch(&mut self, name: &str, at: (usize, usize), st: &mut State, mode: Mode) {
        let Some(decl) = self.lookup(name) else {
            return;
        };
        // (A `let f;` assigned a closure later is still checked for a use before that.)
        if self.soft.contains(&decl) && !self.deferred.contains_key(&decl) {
            if matches!(mode, Mode::Move(_)) && !st.unreachable {
                self.move_sites.insert(at);
            }
            return;
        }
        if !self.tracked.contains(&decl) && !self.deferred.contains_key(&decl) {
            return;
        }
        if let Some(&(moved_at, how)) = st.moved.get(&decl)
            && !st.unreachable
            && self.reported.insert((decl, at.0))
        {
            let idx = LineIndex::new(self.src);
            let (l, c) = idx.line_col(moved_at);
            let n = &self.names[&decl];
            // rustc's "borrow of moved value" when this use only reads. A field read (`p.x`, not
            // `p.x.len()`) is a use, of the place `p.x`; of the field itself, when it's the one
            // that moved.
            let field = self.src[at.1..]
                .strip_prefix('.')
                .map(|r| &r[..r.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(r.len())])
                .filter(|f| !f.is_empty() && !self.src[at.1 + 1 + f.len()..].trim_start().starts_with(['(', ':']));
            let field_read = field.is_some_and(|f| !self.src[at.1 + 1 + f.len()..].starts_with('.'));
            let moved_field = field.filter(|f| st.partial.get(&decl).is_some_and(|m| m.contains(*f)));
            // (A moved field isn't `Copy`: reading it in place borrows it.)
            let verb = if matches!(mode, Mode::Use)
                && (!field_read || (moved_field.is_some() && !self.field_by_value))
            {
                "borrow"
            } else {
                "use"
            };
            let at = match field {
                Some(f) if field_read || moved_field.is_some() => (at.0, at.1 + 1 + f.len()),
                _ => at,
            };
            // (Where the binding is declared, and why its value moves.)
            let mut spans = Vec::new();
            if how != UNINIT
                && !st.partial.contains_key(&decl)
                && let Some(t) = self.types.get(&decl)
            {
                let why = format!(
                    "move occurs because `{n}` has type `{}`, which does not implement the `Copy` trait",
                    t.describe()
                );
                spans.push((decl.0, decl.1, why));
            }
            // (Moved by a pattern binding: the part's type, which rustc gives in a note.)
            if (how == BY_PATTERN || how == INTO_CLOSURE)
                && st.partial.contains_key(&decl)
                && let Some((t, top)) = self.pattern_moves.get(&moved_at)
            {
                let why = match (moved_field, top) {
                    (Some(f), _) => format!(
                        "move occurs because `{n}.{f}` has type `{t}`, which does not implement the `Copy` trait"
                    ),
                    (None, Some(f)) => format!(
                        "partial move occurs because `{n}.{f}` has type `{t}`, which does not implement the `Copy` trait"
                    ),
                    (None, None) => format!(
                        "partial move occurs because value has type `{t}`, which does not implement the `Copy` trait"
                    ),
                };
                spans.push((moved_at, moved_at, why));
            }
            // (Moved into a closure: where the closure uses it.)
            let how = match self.closure_uses.get(&moved_at) {
                Some(&u) if how == INTO_CLOSURE || how == INTO_MOVE_CLOSURE => {
                    let (ul, uc) = idx.line_col(u);
                    format!("{how}, by its use on line {}:{}", ul + 1, uc + 1)
                }
                _ => how.to_string(),
            };
            self.diags.push(RawDiag {
                start: at.0,
                end: at.1,
                message: if how == UNINIT && st.unassigned.contains(&decl) {
                    format!(
                        "used binding `{n}` isn't initialized (declared without a value on line {}; assign it before this use)",
                        l + 1
                    )
                } else if how == UNINIT {
                    format!(
                        "used binding `{n}` is possibly-uninitialized (declared without a value on line {}; assign it on every path before this use)",
                        l + 1
                    )
                } else if moved_at == at.0 {
                    format!("{verb} of moved value: `{n}` (moved here {how} in the loop's previous iteration; `.clone()` it to keep a copy)")
                } else if let Some(f) = moved_field {
                    format!(
                        "{verb} of moved value: `{n}.{f}` (moved on line {}:{} {how}; `.clone()` it there to keep a copy)",
                        l + 1,
                        c + 1
                    )
                } else if st.partial.contains_key(&decl) {
                    format!(
                        "{verb} of partially moved value: `{n}` (a field moved on line {}:{} {how}; bind it with `ref` or `.clone()` it there to keep it)",
                        l + 1,
                        c + 1
                    )
                } else {
                    format!(
                        "{verb} of moved value: `{n}` (moved on line {}:{} {how}; `.clone()` it there to keep a copy)",
                        l + 1,
                        c + 1
                    )
                },
                severity: "error",
                help: None,
                spans,
            });
        }
        if let Mode::Move(how) = mode
            && !st.unreachable
            && self.tracked.contains(&decl)
        {
            st.moved.insert(decl, (at.0, how));
            st.partial.remove(&decl);
            self.move_sites.insert(at);
        }
    }

    fn block(&mut self, b: &ast::BlockExpr, st: &mut State, mode: Mode) {
        let Some(list) = b.stmt_list() else { return };
        self.scopes.push(HashMap::new());
        for s in list.statements() {
            match s {
                ast::Stmt::LetStmt(l) => self.let_stmt(&l, st),
                ast::Stmt::ExprStmt(es) => {
                    if let Some(e) = es.expr() {
                        self.expr(&e, st, Mode::Use);
                    }
                }
                ast::Stmt::Item(_) => {}
            }
        }
        if let Some(t) = list.tail_expr() {
            self.expr(&t, st, mode);
        }
        self.scopes.pop();
    }

    fn let_stmt(&mut self, l: &ast::LetStmt, st: &mut State) {
        let init = l.initializer();
        let pat = l.pat();
        // A destructuring pattern moves only the fields it binds by value, and only when their
        // values move (`let S { x, .. } = s;` with a `Copy` `x` leaves `s` whole).
        let how = self.pat_ctx(init.as_ref(), true);
        let moved_fields = match &pat {
            Some(ast::Pat::IdentPat(ip)) if ip.pat().is_none() => None,
            Some(p) => Some(self.moved_fields(p, how)),
            None => None,
        };
        if let Some(e) = &init {
            match &moved_fields {
                Some(Some(f)) if f.is_empty() => self.expr(e, st, Mode::Use),
                _ => self.expr(e, st, Mode::Move("by `let`")),
            }
            // Partly moved: the other fields can still be read.
            if let Some(Some(f)) = &moved_fields
                && !f.is_empty()
                && let ast::Expr::PathExpr(p) = strip(e)
                && let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
                && let Some(d) = self.lookup(n.text().to_string().as_str())
                && st
                    .moved
                    .get(&d)
                    .is_some_and(|m| m.0 == self.range(p.syntax()).0)
            {
                st.partial.insert(d, f.clone());
                // (rustc points at the binding that took the field.)
                if let Some(p) = &pat
                    && let Some((n, t)) = self.move_binding(p, how)
                {
                    let at = self.range(n.syntax()).0;
                    self.pattern_moves.insert(at, (t, top_field(self.adts, p, &n)));
                    st.moved.insert(d, (at, BY_PATTERN));
                }
            }
        }
        if let Some(le) = l.let_else()
            && let Some(b) = le.block_expr()
        {
            let mut s2 = st.clone();
            self.block(&b, &mut s2, Mode::Use);
        }
        let Some(pat) = pat else { return };
        match &pat {
            ast::Pat::IdentPat(ip) if ip.pat().is_none() && ip.ref_token().is_none() => {
                let Some(n) = ip.name() else { return };
                let decl = self.range(n.syntax());
                let typed_string = l
                    .ty()
                    .is_some_and(|t| string_ty(&t.syntax().text().to_string()));
                let string = typed_string || init.as_ref().is_some_and(|e| self.owned_string(e));
                let owned = match &init {
                    Some(e) => {
                        self.owned_init(e)
                            && (self.ty(n.syntax()).is_some_and(|t| self.moves(t)) || string)
                    }
                    None => self.ty(n.syntax()).is_some_and(|t| self.moves(t)) || typed_string,
                };
                if owned && string {
                    self.strings.insert(decl);
                }
                self.declare(&n, decl, owned);
                self.soft_closure(&n, decl, owned);
                st.moved.remove(&decl);
                if init.is_none() {
                    self.deferred.insert(decl, ip.mut_token().is_some());
                    st.moved.insert(decl, (decl.0, UNINIT));
                    st.assigned.remove(&decl);
                    st.unassigned.insert(decl);
                }
            }
            other => self.bind_pattern(other),
        }
    }

    /// A `String` (not a `&str`) from its initializer.
    fn owned_string(&self, e: &ast::Expr) -> bool {
        match strip(e) {
            ast::Expr::MacroExpr(m) => m
                .macro_call()
                .and_then(|mc| macros::macro_name(&mc))
                .is_some_and(|n| n == "format"),
            ast::Expr::CallExpr(c) => {
                let t = e.syntax().text().to_string();
                if t.starts_with("String::") {
                    return true;
                }
                // (Not `Ok(s)` / `Err(s)`: the other side may hold a `&str`.)
                if let Some(a) = self.wrapped(&c)
                    && t.starts_with("Some")
                {
                    return self.owned_string(&a);
                }
                // A function of the file returning one (`-> Option<String>`).
                let Some(ast::Expr::PathExpr(p)) = c.expr() else {
                    return false;
                };
                let segs: Vec<String> = p
                    .path()
                    .map(|p| {
                        p.segments()
                            .filter_map(|s| s.name_ref())
                            .map(|n| n.text().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let key = match segs.as_slice() {
                    [f] => (None, f.clone()),
                    [.., t, f] if t == "Self" => (self.cur_impl.clone(), f.clone()),
                    [.., t, f] => (Some(t.clone()), f.clone()),
                    [] => return false,
                };
                self.string_fns.contains(&key)
            }
            ast::Expr::MethodCallExpr(m) => m.name_ref().is_some_and(|n| {
                matches!(
                    n.text().to_string().as_str(),
                    "to_string" | "to_owned" | "to_uppercase" | "to_lowercase"
                ) || matches!(
                    m.receiver().and_then(|r| self.ty(r.syntax()).cloned()),
                    Some(Ty::Adt(t, _)) if self.string_fns.contains(&(Some(t.clone()), n.text().to_string()))
                )
            }),
            ast::Expr::PathExpr(_) => self
                .binding_of(e)
                .is_some_and(|d| self.tracked.contains(&d) && self.strings.contains(&d)),
            _ => false,
        }
    }

    /// What `Some(x)` / `Ok(x)` / `Err(x)` wraps.
    fn wrapped(&self, c: &ast::CallExpr) -> Option<ast::Expr> {
        let ast::Expr::PathExpr(p) = c.expr()? else {
            return None;
        };
        let name = p.path()?.as_single_name_ref()?.text().to_string();
        if !matches!(name.as_str(), "Some" | "Ok" | "Err") {
            return None;
        }
        let mut args = c.arg_list()?.args();
        let a = args.next()?;
        args.next().is_none().then_some(a)
    }

    /// Which arguments of the call `c` go to parameters declared `&T` (borrowed, not moved):
    /// `f(..)` for a file's function, `Type::f(..)` for an associated function or method (the
    /// receiver first).
    fn call_ref_params(&self, c: &ast::CallExpr) -> Vec<bool> {
        let Some(ast::Expr::PathExpr(p)) = c.expr().map(|e| strip(&e)) else {
            return vec![];
        };
        let Some(path) = p.path() else { return vec![] };
        let Some(name) = path.segment().and_then(|s| s.name_ref()) else {
            return vec![];
        };
        let name = name.text().to_string();
        let owner = match path.qualifier() {
            None => None,
            Some(q) if q.qualifier().is_none() => match q.segment().and_then(|s| s.name_ref()) {
                Some(n) if n.text() == "Self" => self.cur_impl.clone(),
                Some(n) => Some(n.text().to_string()),
                None => return vec![],
            },
            Some(_) => return vec![],
        };
        let Some((self_ref, params)) = self.ref_params.get(&(owner, name)) else {
            return vec![];
        };
        let mut out = params.clone();
        if let Some(r) = self_ref {
            out.insert(0, *r);
        }
        out
    }

    /// Which arguments of the method call `m` go to parameters declared `&T`: a method of your
    /// type, or an NS / JS binding's (`ns.hack(host)`: `host: &str`).
    fn method_ref_params(&self, m: &ast::MethodCallExpr) -> Vec<bool> {
        let Some(name) = m.name_ref().map(|n| n.text().to_string()) else {
            return vec![];
        };
        match m.receiver().and_then(|r| self.ty(r.syntax()).cloned()) {
            Some(Ty::Adt(adt, _)) => self
                .ref_params
                .get(&(Some(adt), name))
                .map(|(_, ps)| ps.clone())
                .unwrap_or_default(),
            Some(Ty::Extern(t)) => {
                let b = crate::bindings::current();
                b.types
                    .get(&t)
                    .and_then(|et| et.methods.get(&crate::bindings::bare(&name)))
                    .map(|sig| sig.params.iter().map(|p| p.shown.starts_with('&')).collect())
                    .unwrap_or_default()
            }
            _ => vec![],
        }
    }

    /// Whether `m` consumes its receiver.
    fn consumes_receiver(&self, m: &ast::MethodCallExpr) -> bool {
        let Some(name) = m.name_ref().map(|n| n.text().to_string()) else {
            return false;
        };
        let rt = m.receiver().and_then(|r| self.ty(r.syntax()).cloned());
        // `x.into()` takes `self` whatever `x` is.
        if name == "into"
            && m.arg_list().is_some_and(|l| l.args().next().is_none())
            && !matches!(&rt, Some(Ty::Adt(n, _)) if self.adts.method(n, "into").is_some())
        {
            return true;
        }
        match &rt {
            Some(Ty::Adt(adt, _)) => self
                .adts
                .method(adt, &name)
                .is_some_and(|s| s.receiver == Some(crate::adt::Receiver::Value)),
            // Adapters and consumers; `next`, `by_ref`, `any`, `find`, .. take `&mut self`.
            Some(Ty::Iter { .. }) => matches!(
                name.as_str(),
                "map"
                    | "filter"
                    | "filter_map"
                    | "flat_map"
                    | "flatten"
                    | "enumerate"
                    | "zip"
                    | "chain"
                    | "rev"
                    | "skip"
                    | "take"
                    | "skip_while"
                    | "take_while"
                    | "step_by"
                    | "peekable"
                    | "cloned"
                    | "copied"
                    | "inspect"
                    | "scan"
                    | "fuse"
                    | "cycle"
                    | "collect"
                    | "sum"
                    | "product"
                    | "count"
                    | "fold"
                    | "reduce"
                    | "for_each"
                    | "last"
                    | "max"
                    | "min"
                    | "max_by"
                    | "min_by"
                    | "max_by_key"
                    | "min_by_key"
                    | "partition"
                    | "unzip"
            ),
            Some(Ty::Option(_) | Ty::Result(..)) => matches!(
                name.as_str(),
                "unwrap"
                    | "expect"
                    | "unwrap_or"
                    | "unwrap_or_default"
                    | "unwrap_or_else"
                    | "map"
                    | "and_then"
                    | "ok"
                    | "err"
            ),
            Some(Ty::Vec(_) | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..)) => {
                matches!(name.as_str(), "into_iter" | "into_keys" | "into_values")
            }
            Some(Ty::RefCell(_)) => name == "into_inner",
            _ => false,
        }
    }

    fn expr(&mut self, e: &ast::Expr, st: &mut State, mode: Mode) {
        match e {
            ast::Expr::PathExpr(p) => self.path(p, st, mode),
            ast::Expr::ParenExpr(p) => {
                if let Some(i) = p.expr() {
                    self.expr(&i, st, mode);
                }
            }
            ast::Expr::RefExpr(r) => {
                if let Some(i) = r.expr() {
                    self.expr(&i, st, Mode::Use);
                }
            }
            ast::Expr::BinExpr(b) => {
                let (lhs, rhs) = (b.lhs(), b.rhs());
                match b.op_kind() {
                    Some(ast::BinaryOp::Assignment { op: None }) => {
                        if let Some(r) = &rhs {
                            self.expr(r, st, Mode::Move("by assignment"));
                        }
                        // Assigning to a moved binding gives it a value again.
                        match lhs.as_ref().map(strip) {
                            Some(ast::Expr::PathExpr(p)) => {
                                if let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
                                    && let Some(d) = self.lookup(n.text().to_string().as_str())
                                {
                                    self.assign_deferred(d, self.range(p.syntax()), st);
                                    st.moved.remove(&d);
                                    st.partial.remove(&d);
                                }
                            }
                            // `p.a = ..` after `p.a` moved gives the field a value again.
                            Some(ast::Expr::FieldExpr(f))
                                if self.field_base(&f).is_some_and(|(d, field)| {
                                    st.partial.get(&d).is_some_and(|m| m.contains(&field))
                                }) =>
                            {
                                let (d, field) = self.field_base(&f).unwrap();
                                let moved = st.partial.get_mut(&d).unwrap();
                                moved.remove(&field);
                                if moved.is_empty() {
                                    st.partial.remove(&d);
                                    st.moved.remove(&d);
                                }
                            }
                            Some(other) => self.expr(&other, st, Mode::Use),
                            None => {}
                        }
                    }
                    // `s1 + &s2` moves `s1`.
                    Some(ast::BinaryOp::ArithOp(ast::ArithOp::Add))
                        if lhs
                            .as_ref()
                            .and_then(|l| self.ty(l.syntax()))
                            .is_some_and(|t| *t == Ty::Str) =>
                    {
                        if let Some(l) = &lhs {
                            self.expr(l, st, Mode::Move("by `+`"));
                        }
                        if let Some(r) = &rhs {
                            self.expr(r, st, Mode::Use);
                        }
                    }
                    Some(ast::BinaryOp::LogicOp(_)) => {
                        if let Some(l) = &lhs {
                            self.expr(l, st, Mode::Use);
                        }
                        // The right side may not run.
                        if let Some(r) = &rhs {
                            let mut s2 = st.clone();
                            self.expr(r, &mut s2, Mode::Use);
                            *st = State::join(st.clone(), s2);
                        }
                    }
                    _ => {
                        for x in [lhs, rhs].into_iter().flatten() {
                            self.expr(&x, st, Mode::Use);
                        }
                    }
                }
            }
            ast::Expr::CallExpr(c) => {
                if let Some(f) = c.expr() {
                    self.expr(&f, st, Mode::Use);
                }
                let refs = self.call_ref_params(c);
                for (i, a) in c.arg_list().into_iter().flat_map(|l| l.args()).enumerate() {
                    let mode = if refs.get(i) == Some(&true) {
                        Mode::Use
                    } else {
                        Mode::Move("as an argument")
                    };
                    self.expr(&a, st, mode);
                }
            }
            ast::Expr::MethodCallExpr(m) => {
                if let Some(r) = m.receiver() {
                    let how = if self.consumes_receiver(m) {
                        Mode::Move("by a method that takes `self`")
                    } else {
                        Mode::Use
                    };
                    self.expr(&r, st, how);
                }
                let refs = self.method_ref_params(m);
                for (i, a) in m.arg_list().into_iter().flat_map(|l| l.args()).enumerate() {
                    let mode = if refs.get(i) == Some(&true) {
                        Mode::Use
                    } else {
                        Mode::Move("as an argument")
                    };
                    self.expr(&a, st, mode);
                }
            }
            ast::Expr::FieldExpr(f) => {
                // A field that stayed behind in a partly moved struct can still be read.
                let base = self.field_base(f);
                let kept = base.as_ref().is_some_and(|(d, field)| {
                    st.partial.get(d).is_some_and(|moved| !moved.contains(field))
                });
                if let Some(x) = f.expr()
                    && !kept
                {
                    let by_value = matches!(mode, Mode::Move(_)) && matches!(strip(&x), ast::Expr::PathExpr(_));
                    let saved = std::mem::replace(&mut self.field_by_value, by_value);
                    self.expr(&x, st, Mode::Use);
                    self.field_by_value = saved;
                }
                // `let a = p.a;` moves only that field: `p` is partly moved.
                if let Mode::Move(how) = mode
                    && !st.unreachable
                    && let Some((d, field)) = base
                    && self.tracked.contains(&d)
                    && (!st.moved.contains_key(&d) || st.partial.contains_key(&d))
                    && self.field_moves(f)
                {
                    st.moved.entry(d).or_insert((self.range(f.syntax()).0, how));
                    st.partial.entry(d).or_default().insert(field);
                }
                // A field moved out of its struct (`let x = s.inner;`): the struct no longer
                // drops it (codegen marks it, when its type has something to drop).
                if matches!(mode, Mode::Move(_)) && !st.unreachable {
                    self.move_sites.insert(self.range(f.syntax()));
                }
            }
            ast::Expr::IndexExpr(ix) => {
                for x in [ix.base(), ix.index()].into_iter().flatten() {
                    self.expr(&x, st, Mode::Use);
                }
            }
            ast::Expr::RecordExpr(r) => {
                if let Some(l) = r.record_expr_field_list() {
                    for f in l.fields() {
                        match f.expr() {
                            Some(x) => self.expr(&x, st, Mode::Move("into a struct")),
                            // Shorthand `Point { x }` moves `x`.
                            None => {
                                if let Some(n) = f.field_name()
                                    && let Some(d) = self.lookup(n.text().to_string().as_str())
                                    && self.tracked.contains(&d)
                                {
                                    let at = self.range(f.syntax());
                                    if !st.unreachable {
                                        st.moved.insert(d, (at.0, "into a struct"));
                                    }
                                }
                            }
                        }
                    }
                    if let Some(b) = l.spread() {
                        self.expr(&b, st, Mode::Use);
                    }
                }
            }
            ast::Expr::TupleExpr(t) => {
                for x in t.fields() {
                    self.expr(&x, st, Mode::Move("into a tuple"));
                }
            }
            ast::Expr::ArrayExpr(a) => {
                for x in a.syntax().children().filter_map(ast::Expr::cast) {
                    self.expr(&x, st, Mode::Move("into an array"));
                }
            }
            ast::Expr::MacroExpr(m) => {
                let Some(mc) = m.macro_call() else { return };
                let name = macros::macro_name(&mc).unwrap_or_default();
                // A `quote!` template isn't code: only its `#var`s read variables.
                if matches!(name.as_str(), "quote" | "quote_spanned" | "parse_quote") {
                    fn vars(parts: &[macros::QuotePart], out: &mut Vec<String>) {
                        for p in parts {
                            match p {
                                macros::QuotePart::Var { name, .. } => out.push(name.clone()),
                                macros::QuotePart::Rep { parts, .. } => vars(parts, out),
                                macros::QuotePart::Text(_) => {}
                            }
                        }
                    }
                    let mut names = Vec::new();
                    vars(&macros::quote_parts(&mc, self.base), &mut names);
                    let at = self.range(mc.syntax());
                    for n in names {
                        self.touch(&n, at, st, Mode::Use);
                    }
                    return;
                }
                let mode = if name == "vec" {
                    Mode::Move("into a `vec!`")
                } else if name == "dbg" {
                    Mode::Move("into a `dbg!`")
                } else {
                    Mode::Use
                };
                let args = macros::split_args(self.src, self.base, &mc);
                // `{name}` in a format string reads `name`.
                if !matches!(name.as_str(), "vec" | "dbg")
                    && let Some(fs) = args.iter().find_map(macros::format_string)
                    && let Ok(pieces) = crate::fmt::parse(&fs)
                {
                    let at = self.range(mc.syntax());
                    for piece in pieces {
                        if let crate::fmt::Piece::Arg {
                            arg: crate::fmt::ArgRef::Name(n),
                            ..
                        } = piece
                            && !args.iter().any(|a| a.name.as_deref() == Some(n.as_str()))
                        {
                            self.touch(&n, at, st, Mode::Use);
                        }
                    }
                }
                for a in args {
                    let (Some(x), _) = macros::parse_arg(&a) else {
                        continue;
                    };
                    let saved = std::mem::replace(&mut self.base, a.start);
                    self.expr(&x, st, mode);
                    self.base = saved;
                }
            }
            ast::Expr::BlockExpr(b) => match b.label() {
                // `'a: { .. break 'a .. }`: the block ends at its end or at a `break 'a`.
                Some(l) => {
                    let label = l.lifetime().map(|l| l.text().to_string());
                    self.targets.push(Target {
                        label,
                        is_loop: false,
                        breaks: vec![],
                        continues: vec![],
                    });
                    self.block(b, st, mode);
                    let t = self.targets.pop().unwrap();
                    for s2 in t.breaks {
                        *st = State::join(st.clone(), s2);
                    }
                }
                None => self.block(b, st, mode),
            },
            ast::Expr::IfExpr(i) => {
                if let Some(c) = i.condition() {
                    self.condition(&c, st);
                }
                let mut then_st = st.clone();
                self.scopes.push(HashMap::new());
                if let Some(c) = i.condition() {
                    for le in let_chain(&c) {
                        if let Some(p) = le.pat() {
                            // `if let Some(x) = o` moves out of `o` when `x` takes a `String`, ..
                            let m = le.expr().and_then(|s| self.pattern_move(&p, &s));
                            self.apply_pattern_move(m, &mut then_st);
                            self.bind_pattern(&p);
                        }
                    }
                }
                if let Some(b) = i.then_branch() {
                    self.block(&b, &mut then_st, mode);
                }
                self.scopes.pop();
                let mut else_st = st.clone();
                match i.else_branch() {
                    Some(ast::ElseBranch::Block(b)) => self.block(&b, &mut else_st, mode),
                    Some(ast::ElseBranch::IfExpr(ei)) => {
                        self.expr(&ast::Expr::IfExpr(ei), &mut else_st, mode)
                    }
                    None => {}
                }
                *st = State::join(then_st, else_st);
            }
            ast::Expr::MatchExpr(mx) => {
                if let Some(s) = mx.expr() {
                    self.expr(&s, st, Mode::Use);
                }
                let mut out: Option<State> = None;
                let scrutinee = mx.expr();
                for arm in mx.match_arm_list().into_iter().flat_map(|l| l.arms()) {
                    let mut s2 = st.clone();
                    self.scopes.push(HashMap::new());
                    // (Looked up before the arm's own bindings, which may shadow it.)
                    let m = arm
                        .pat()
                        .zip(scrutinee.as_ref())
                        .and_then(|(p, s)| self.pattern_move(&p, s));
                    if let Some(p) = arm.pat() {
                        self.bind_pattern(&p);
                    }
                    if let Some(g) = arm.guard().and_then(|g| g.condition()) {
                        self.expr(&g, &mut s2, Mode::Use);
                    }
                    // The arm's by-value bindings move out of the value once the arm is taken.
                    self.apply_pattern_move(m, &mut s2);
                    if let Some(b) = arm.expr() {
                        self.expr(&b, &mut s2, mode);
                    }
                    self.scopes.pop();
                    out = Some(match out {
                        Some(o) => State::join(o, s2),
                        None => s2,
                    });
                }
                if let Some(o) = out {
                    *st = o;
                }
            }
            ast::Expr::WhileExpr(w) => {
                let body = w.loop_body();
                let cond = w.condition();
                let label = w
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.looped(st, label, false, |me, s| {
                    me.scopes.push(HashMap::new());
                    if let Some(c) = &cond {
                        me.condition(c, s);
                        for le in let_chain(c) {
                            if let Some(p) = le.pat() {
                                let m = le.expr().and_then(|x| me.pattern_move(&p, &x));
                                me.apply_pattern_move(m, s);
                                me.bind_pattern(&p);
                            }
                        }
                    }
                    if let Some(b) = &body {
                        me.block(b, s, Mode::Use);
                    }
                    me.scopes.pop();
                });
            }
            ast::Expr::LoopExpr(l) => {
                let body = l.loop_body();
                let label = l
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.looped(st, label, true, |me, s| {
                    if let Some(b) = &body {
                        me.block(b, s, Mode::Use);
                    }
                });
            }
            ast::Expr::ForExpr(f) => {
                // `for x in v` consumes `v`.
                if let Some(it) = f.iterable() {
                    self.expr(&it, st, Mode::Move("by a `for` loop"));
                }
                let body = f.loop_body();
                let pat = f.pat();
                let label = f
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.looped(st, label, false, |me, s| {
                    me.scopes.push(HashMap::new());
                    if let Some(p) = &pat {
                        me.bind_pattern(p);
                    }
                    if let Some(b) = &body {
                        me.block(b, s, Mode::Use);
                    }
                    me.scopes.pop();
                });
            }
            ast::Expr::ClosureExpr(c) => {
                let moves_captures = c.move_token().is_some();
                let Some(body) = c.body() else { return };
                // The body runs later: uses are checked now, its own moves don't leak out.
                let mut inner = st.clone();
                self.scopes.push(HashMap::new());
                if let Some(pl) = c.param_list() {
                    for p in pl.params() {
                        if let Some(pat) = p.pat() {
                            self.bind_pattern(&pat);
                        }
                    }
                }
                self.expr(&body, &mut inner, Mode::Use);
                let captured: Vec<Decl> = body
                    .syntax()
                    .descendants()
                    .filter_map(ast::PathExpr::cast)
                    .filter_map(|p| p.path()?.as_single_name_ref())
                    .filter_map(|n| self.lookup(n.text().to_string().as_str()))
                    .collect();
                self.scopes.pop();
                if st.unreachable {
                    return;
                }
                let at = self.range(c.syntax());
                let first_use = |me: &Self, d: Decl| {
                    body.syntax()
                        .descendants()
                        .filter_map(ast::PathExpr::cast)
                        .find(|p| {
                            p.path()
                                .and_then(|p| p.as_single_name_ref())
                                .and_then(|n| me.lookup(n.text().to_string().as_str()))
                                == Some(d)
                        })
                        .map(|p| me.range(p.syntax()).0)
                };
                for d in captured {
                    if !self.tracked.contains(&d) || !self.lookup_is_outer(d) {
                        continue;
                    }
                    if moves_captures {
                        if let Some(u) = first_use(self, d) {
                            self.closure_uses.insert(at.0, u);
                        }
                        st.moved.insert(d, (at.0, INTO_MOVE_CLOSURE));
                        st.partial.remove(&d);
                        continue;
                    }
                    // Without `move`, a capture the body moves (`drop(s)`, `s.into_bytes()`)
                    // is taken by value: it moves into the closure where the closure is made.
                    let Some(&(u, _)) = inner.moved.get(&d) else {
                        continue;
                    };
                    if !(at.0..at.1).contains(&u) || st.moved.get(&d) == inner.moved.get(&d) {
                        continue;
                    }
                    self.closure_uses.insert(at.0, u);
                    match inner.partial.get(&d) {
                        // (A field moved in the body: only that field is captured, by value.)
                        Some(f) => {
                            // (The field's type, for rustc's note.)
                            if let Some(name) = f.iter().min()
                                && let Some(t) = self.field_ty(d, name)
                            {
                                self.pattern_moves.insert(at.0, (t, Some(name.clone())));
                            }
                            st.moved.entry(d).or_insert((at.0, INTO_CLOSURE));
                            st.partial.entry(d).or_default().extend(f.iter().cloned());
                        }
                        None => {
                            st.moved.insert(d, (at.0, INTO_CLOSURE));
                            st.partial.remove(&d);
                        }
                    }
                }
            }
            ast::Expr::ReturnExpr(r) => {
                if let Some(x) = r.expr() {
                    self.expr(&x, st, Mode::Move("by `return`"));
                }
                st.unreachable = true;
            }
            ast::Expr::BreakExpr(b) => {
                if let Some(x) = b.expr() {
                    self.expr(&x, st, Mode::Move("by `break`"));
                }
                let label = b.lifetime().map(|l| l.text().to_string());
                if let Some(t) = self.target(label.as_deref(), false) {
                    t.breaks.push(st.clone());
                }
                st.unreachable = true;
            }
            ast::Expr::ContinueExpr(c) => {
                let label = c.lifetime().map(|l| l.text().to_string());
                if let Some(t) = self.target(label.as_deref(), true) {
                    t.continues.push(st.clone());
                }
                st.unreachable = true;
            }
            ast::Expr::TryExpr(t) => {
                if let Some(x) = t.expr() {
                    self.expr(&x, st, Mode::Move("by `?`"));
                }
            }
            ast::Expr::AwaitExpr(a) => {
                if let Some(x) = a.expr() {
                    self.expr(&x, st, Mode::Use);
                }
            }
            ast::Expr::LetExpr(le) => {
                if let Some(x) = le.expr() {
                    self.expr(&x, st, Mode::Use);
                }
            }
            other => {
                for x in other.syntax().children().filter_map(ast::Expr::cast) {
                    self.expr(&x, st, Mode::Use);
                }
            }
        }
    }

    /// An assignment to `d`: a second one to an immutable `let x;` is an error.
    fn assign_deferred(&mut self, d: Decl, at: (usize, usize), st: &mut State) {
        let Some(&mutable) = self.deferred.get(&d) else {
            return;
        };
        if st.unreachable {
            return;
        }
        if !mutable && st.assigned.contains(&d) && self.reported.insert((d, at.0)) {
            let n = &self.names[&d];
            self.diags.push(RawDiag {
                start: at.0,
                end: at.1,
                message: format!(
                    "cannot assign twice to immutable variable `{n}` (make it `let mut {n};` to assign it again)"
                ),
                severity: "error",
                help: None,
                spans: Vec::new(),
            });
        }
        st.assigned.insert(d);
        st.unassigned.remove(&d);
    }

    /// An `if` / `while` condition: its parts are reads.
    fn condition(&mut self, c: &ast::Expr, st: &mut State) {
        self.expr(c, st, Mode::Use);
    }

    /// Whether a captured binding was declared outside the closure (still in scope after it).
    fn lookup_is_outer(&self, d: Decl) -> bool {
        self.scopes.iter().any(|s| s.values().any(|x| *x == d))
    }

    /// The loop or labeled block a `break`/`continue` leaves.
    fn target(&mut self, label: Option<&str>, is_continue: bool) -> Option<&mut Target> {
        self.targets.iter_mut().rev().find(|t| match label {
            Some(l) => t.label.as_deref() == Some(l),
            None => t.is_loop,
        } && (t.is_loop || !is_continue))
    }

    /// A loop body: run twice so a move in one iteration is caught by the next. After the loop,
    /// the state is any `break`'s, plus (unless it is a `loop`) the entry and every iteration's end.
    fn looped(
        &mut self,
        st: &mut State,
        label: Option<String>,
        infinite: bool,
        mut body: impl FnMut(&mut Self, &mut State),
    ) {
        let entry = st.clone();
        let mut exit = if infinite {
            State {
                unreachable: true,
                ..State::default()
            }
        } else {
            entry.clone()
        };
        let mut next = entry.clone();
        for _ in 0..2 {
            if next.unreachable {
                break;
            }
            self.targets.push(Target {
                label: label.clone(),
                is_loop: true,
                breaks: vec![],
                continues: vec![],
            });
            let mut s = next.clone();
            body(self, &mut s);
            let t = self.targets.pop().unwrap();
            for c in t.continues {
                s = State::join(s, c);
            }
            for b in t.breaks {
                exit = State::join(exit, b);
            }
            if !infinite {
                exit = State::join(exit, s.clone());
            }
            next = s;
        }
        *st = exit;
    }
}

/// The type name of the `impl` that `f` is a method of.
fn impl_of(f: &ast::Fn) -> Option<String> {
    let imp = f.syntax().ancestors().find_map(ast::Impl::cast)?;
    // A function nested in a method body isn't one of the impl's.
    if f.syntax().parent()?.parent().as_ref() != Some(imp.syntax()) {
        return None;
    }
    crate::adt::impl_parts(&imp).1
}

/// The field of a struct or tuple that the binding `n` takes, when it's one of `p`'s own fields
/// (`P { name, .. }`, `(_, v)`), as rustc names the moved place (`p.name`).
fn top_field(adts: &Adts, p: &ast::Pat, n: &ast::Name) -> Option<String> {
    let ip = n.syntax().parent()?;
    let parent = ip.parent()?;
    match p {
        ast::Pat::RecordPat(rp) => {
            let s = rp.path()?.segment()?.name_ref()?.text().to_string();
            adts.defs.get(&s).filter(|d| !d.is_enum)?;
            let f = ast::RecordPatField::cast(parent)?;
            (f.syntax().parent()?.parent()? == *rp.syntax()).then_some(())?;
            Some(f.field_name()?.text().to_string())
        }
        ast::Pat::TuplePat(tp) if parent == *tp.syntax() => {
            let fields: Vec<ast::Pat> = tp.fields().collect();
            let i = fields.iter().position(|f| *f.syntax() == ip)?;
            (!fields[..i].iter().any(|f| matches!(f, ast::Pat::RestPat(_)))).then(|| i.to_string())
        }
        _ => None,
    }
}

/// The `let`s of an `if` / `while` condition (`let a = x && let b = y`), not those nested deeper.
fn let_chain(c: &ast::Expr) -> Vec<ast::LetExpr> {
    match c {
        ast::Expr::LetExpr(le) => vec![le.clone()],
        ast::Expr::ParenExpr(p) => p.expr().map(|i| let_chain(&i)).unwrap_or_default(),
        ast::Expr::BinExpr(b)
            if matches!(
                b.op_kind(),
                Some(ast::BinaryOp::LogicOp(ast::LogicOp::And))
            ) =>
        {
            [b.lhs(), b.rhs()]
                .into_iter()
                .flatten()
                .flat_map(|x| let_chain(&x))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn strip(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::ParenExpr(p) => p.expr().map(|i| strip(&i)).unwrap_or_else(|| e.clone()),
        _ => e.clone(),
    }
}
