//! Generic functions: type parameters, their trait bounds, instantiation at call sites, and what
//! a body may do with a value of type `T`. Generics are erased in JS, so a `T` only allows
//! operations that behave the same for every type it could be. A child module of `typeck`.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasGenericParams, HasName, HasTypeBounds},
};

use super::{Checker, VarKind};
use crate::ty::Ty;

/// The trait bounds a type parameter may have.
const KNOWN_BOUNDS: &[&str] = &[
    "Copy",
    "Clone",
    "Debug",
    "Display",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
    "Add",
    "Sub",
    "Mul",
    "Div",
    "Rem",
    "Neg",
    "AddAssign",
    "SubAssign",
    "MulAssign",
    "DivAssign",
    "RemAssign",
    "BitAnd",
    "BitOr",
    "BitXor",
    "Not",
    "Shl",
    "Shr",
    "BitAndAssign",
    "BitOrAssign",
    "BitXorAssign",
    "ShlAssign",
    "ShrAssign",
    "FromStr",
    "From",
    "Sum",
    "Product",
    // (Marker traits: nothing to check in JS.)
    "Send",
    "Sync",
    "Unpin",
    "Sized",
    "ToString",
];

/// A type parameter with the names of its bounds (`T: PartialOrd + Copy` → `["PartialOrd", "Copy"]`).
pub type TypeParam = (String, Vec<String>);

/// Every type parameter of `f` with its bound lists (inline and from `where`).
fn type_params(f: &ast::Fn) -> Vec<(String, Vec<ast::TypeBoundList>, SyntaxNode)> {
    let mut out: Vec<(String, Vec<ast::TypeBoundList>, SyntaxNode)> = Vec::new();
    if let Some(gp) = f.generic_param_list() {
        for p in gp.generic_params() {
            let ast::GenericParam::TypeParam(tp) = p else {
                continue;
            };
            let Some(n) = tp.name() else { continue };
            out.push((
                n.text().to_string(),
                tp.type_bound_list().into_iter().collect(),
                tp.syntax().clone(),
            ));
        }
    }
    if let Some(wc) = f.where_clause() {
        for pred in wc.predicates() {
            let name = match pred.ty() {
                Some(ast::Type::PathType(pt)) => pt
                    .path()
                    .and_then(|p| p.as_single_name_ref())
                    .map(|n| n.text().to_string()),
                _ => None,
            };
            if let (Some(n), Some(b)) = (name, pred.type_bound_list())
                && let Some(entry) = out.iter_mut().find(|(x, _, _)| *x == n)
            {
                entry.1.push(b);
            }
        }
    }
    out
}

/// The last path segment of each trait bound (`std::fmt::Display` → `Display`); `?Sized` and
/// lifetimes are skipped.
fn bound_names(lists: &[ast::TypeBoundList]) -> Vec<(String, SyntaxNode)> {
    let mut out = Vec::new();
    for l in lists {
        for b in l.bounds() {
            if b.question_mark_token().is_some() {
                continue;
            }
            if let Some(ast::Type::PathType(p)) = b.ty()
                && let Some(seg) = p.path().and_then(|p| p.segment())
                && let Some(n) = seg.name_ref()
            {
                out.push((n.text().to_string(), b.syntax().clone()));
            }
        }
    }
    out
}

/// Replace type parameters per `m`.
pub fn subst(t: &Ty, m: &HashMap<String, Ty>) -> Ty {
    let s = |x: &Ty| Box::new(subst(x, m));
    match t {
        // `T::Item` follows `T`: `P::Item` when `T` is now the parameter `P`, unchecked when it's
        // a concrete type or not known yet (the impl's `type Item` isn't looked up here).
        Ty::Param(n) if !m.contains_key(n) && n.contains("::") => {
            let (base, assoc) = n.split_once("::").unwrap_or_default();
            match m.get(base) {
                Some(Ty::Param(p)) => Ty::Param(format!("{p}::{assoc}")),
                Some(_) => Ty::Unknown,
                None => t.clone(),
            }
        }
        Ty::Param(n) => m.get(n).cloned().unwrap_or_else(|| t.clone()),
        Ty::Vec(e) => Ty::Vec(s(e)),
        Ty::Option(e) => Ty::Option(s(e)),
        Ty::Set(e) => Ty::Set(s(e)),
        Ty::Coll(k, e) => Ty::Coll(*k, s(e)),
        Ty::Future(e) => Ty::Future(s(e)),
        Ty::Map(k, v) => Ty::Map(s(k), s(v)),
        Ty::Entry(k, v) => Ty::Entry(s(k), s(v)),
        Ty::Result(k, v) => Ty::Result(s(k), s(v)),
        Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| subst(e, m)).collect()),
        Ty::Adt(n, args) => Ty::Adt(n.clone(), args.iter().map(|a| subst(a, m)).collect()),
        Ty::RefCell(x) => Ty::RefCell(s(x)),
        Ty::Iter { elem, mutable } => Ty::Iter {
            elem: s(elem),
            mutable: *mutable,
        },
        Ty::IntoIter(e) => Ty::IntoIter(s(e)),
        Ty::Range(e, incl) => Ty::Range(s(e), *incl),
        Ty::Closure { params, ret } => Ty::Closure {
            params: params.iter().map(|p| subst(p, m)).collect(),
            ret: s(ret),
        },
        other => other.clone(),
    }
}

impl Checker<'_> {
    /// The type parameters of `f`: `Fn`-bounded ones as closure types, the rest as `Ty::Param`.
    /// Sets `self.generics` and `self.param_bounds` for lowering the signature or checking the
    /// body. Unsupported bounds are reported when `report` is set (once per function).
    pub(super) fn enter_generics(&mut self, f: &ast::Fn, report: bool) -> Vec<TypeParam> {
        self.generics.clear();
        self.param_bounds.clear();
        self.param_trait_args.clear();
        self.lenient_projs.clear();
        self.const_tys.clear();
        self.from_args.clear();
        let mut params = Vec::new();
        // (`I: IntoIterator<Item = P>` names `P`, which may come later: those go last.)
        let mut tps = type_params(f);
        tps.sort_by_key(|(_, lists, _)| {
            lists.iter().any(|l| {
                super::bounds_into_iter_item(l).is_some()
                    || super::bounds_iterator_item(l).is_some()
            })
        });
        for (name, lists, node) in tps {
            if let Some(closure) = lists.iter().find_map(|l| self.lower_fn_bounds(l)) {
                self.generics.insert(name, closure);
                continue;
            }
            // `S: Into<String>` / `S: AsRef<str>`: the target type, like `impl Into<String>`.
            if let Some(target) = lists.iter().find_map(super::conversion_target) {
                let t = self.lower_type(&target);
                self.generics.insert(name, t);
                continue;
            }
            // `H: Hasher` in an `impl Hash`.
            if lists.iter().any(super::bounds_hasher) {
                self.generics.insert(name, Ty::Hasher);
                continue;
            }
            // `R: Rng` (the bundled `rand`).
            if lists.iter().any(super::crates::bounds_rng) {
                self.generics.insert(name, Ty::Rng);
                continue;
            }
            // `I: IntoIterator<Item = T>`: any iterable of `T`.
            if let Some(item) = lists.iter().find_map(super::bounds_into_iter_item) {
                let elem = match item {
                    Some(t) => self.lower_type(&t),
                    None => self.item_param(f, &name, report, &mut params),
                };
                self.generics.insert(name, Ty::IntoIter(Box::new(elem)));
                continue;
            }
            // `I: Iterator<Item = T>`: the runtime's iterator, like `impl Iterator<Item = T>`.
            if let Some(item) = lists.iter().find_map(super::bounds_iterator_item) {
                let elem = match item {
                    Some(t) => self.lower_type(&t),
                    None => self.item_param(f, &name, report, &mut params),
                };
                let it = Ty::Iter {
                    elem: Box::new(elem),
                    mutable: false,
                };
                self.generics.insert(name, it);
                continue;
            }
            let mut bounds = Vec::new();
            for (b, bnode) in bound_names(&lists) {
                if KNOWN_BOUNDS.contains(&b.as_str()) || self.adts.traits.contains_key(&b) {
                    bounds.push(b);
                } else if report {
                    self.error(
                        &bnode,
                        format!("the trait bound `{b}` is not supported yet on type parameters (supported: the comparison, formatting, `Clone`/`Copy`/`Default`/`Hash`, arithmetic and bitwise operator, `From`/`FromStr`, `Sum`/`Product`, iterator and closure traits, `Send`/`Sync`, and traits declared in the file)"),
                    );
                }
            }
            let _ = node;
            self.generics.insert(name.clone(), Ty::Param(name.clone()));
            self.param_bounds.insert(name.clone(), bounds.clone());
            params.push((name, bounds));
        }
        // `const N: usize`: a value in the body, and a type argument (`Buf<N>`).
        let mut order: Vec<String> = Vec::new();
        for p in f
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
        {
            match p {
                ast::GenericParam::ConstParam(cp) => {
                    let Some(n) = cp.name() else { continue };
                    let n = n.text().to_string();
                    let ty = self.const_param_ty(&cp, report);
                    self.generics.insert(n.clone(), Ty::Param(n.clone()));
                    self.const_tys.insert(n.clone(), ty);
                    params.push((n.clone(), Vec::new()));
                    order.push(n);
                }
                ast::GenericParam::TypeParam(tp) => {
                    if let Some(n) = tp.name() {
                        order.push(n.text().to_string());
                    }
                }
                ast::GenericParam::LifetimeParam(_) => {}
            }
        }
        params.sort_by_key(|(n, _)| order.iter().position(|x| x == n));
        // The bounds' arguments (`C: Convert<String>`, `S: Shape<Unit = f64>`), once every
        // parameter is in scope.
        for (name, lists, _) in type_params(f) {
            self.record_from_bound(&name, &lists);
            if self.param_bounds.contains_key(&name) {
                self.record_bound_args(&name, &lists);
            }
        }
        self.note_where_projections(f.where_clause());
        params
    }

    /// `I: IntoIterator` / `I: Iterator` without `Item = ..`: its items are a type parameter of
    /// their own, `I::Item` (a fresh type per call), with the bounds `where I::Item: ..` gives.
    fn item_param(
        &mut self,
        f: &ast::Fn,
        name: &str,
        report: bool,
        params: &mut Vec<TypeParam>,
    ) -> Ty {
        let item = format!("{name}::Item");
        let mut lists = Vec::new();
        for pred in f.where_clause().into_iter().flat_map(|w| w.predicates()) {
            if let Some(t) = pred.ty()
                && t.syntax().text() == item.as_str()
                && let Some(l) = pred.type_bound_list()
            {
                lists.push(l);
            }
        }
        let mut bounds = Vec::new();
        for (b, bnode) in bound_names(&lists) {
            if KNOWN_BOUNDS.contains(&b.as_str()) || self.adts.traits.contains_key(&b) {
                bounds.push(b);
            } else if report {
                self.error(
                    &bnode,
                    format!("the trait bound `{b}` is not supported yet on type parameters"),
                );
            }
        }
        self.generics.insert(item.clone(), Ty::Param(item.clone()));
        self.param_bounds.insert(item.clone(), bounds.clone());
        params.push((item.clone(), bounds));
        Ty::Param(item)
    }

    /// The associated types of each type parameter's bounds, in order: (parameter, trait,
    /// associated type).
    pub(super) fn proj_names(&self, type_params: &[TypeParam]) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for (p, bounds) in type_params {
            for b in bounds {
                if let Some(d) = self.adts.traits.get(b) {
                    for a in &d.assoc_types {
                        out.push((p.clone(), b.clone(), a.clone()));
                    }
                }
            }
        }
        out
    }

    /// `T: From<X>`: `X`, for `T::from(x)`.
    pub(super) fn record_from_bound(&mut self, name: &str, lists: &[ast::TypeBoundList]) {
        use ra_ap_syntax::ast::HasGenericArgs;
        for l in lists {
            for b in l.bounds() {
                if let Some(ast::Type::PathType(pt)) = b.ty()
                    && let Some(seg) = pt.path().and_then(|p| p.segment())
                    && seg.name_ref().is_some_and(|n| n.text() == "From")
                    && let Some(ast::GenericArg::TypeArg(ta)) =
                        seg.generic_arg_list().and_then(|l| l.generic_args().next())
                    && let Some(t) = ta.ty()
                {
                    let x = self.lower_type(&t);
                    self.from_args.insert(name.to_string(), x);
                }
            }
        }
    }

    /// Record `name`'s user-trait bounds with their arguments.
    pub(super) fn record_bound_args(&mut self, name: &str, lists: &[ast::TypeBoundList]) {
        use ra_ap_syntax::ast::HasGenericArgs;
        let mut per = HashMap::new();
        for l in lists {
            for b in l.bounds() {
                let Some(ast::Type::PathType(pt)) = b.ty() else {
                    continue;
                };
                let Some(seg) = pt.path().and_then(|p| p.segment()) else {
                    continue;
                };
                let Some(tr) = seg.name_ref().map(|n| n.text().to_string()) else {
                    continue;
                };
                if !self.adts.traits.contains_key(&tr) {
                    continue;
                }
                let mut pos = Vec::new();
                let mut named = HashMap::new();
                for g in seg
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|l| l.generic_args())
                {
                    match g {
                        ast::GenericArg::TypeArg(ta) => {
                            if let Some(t) = ta.ty() {
                                pos.push(self.lower_type(&t));
                            }
                        }
                        ast::GenericArg::AssocTypeArg(a) => {
                            if let (Some(n), Some(t)) = (a.name_ref(), a.ty()) {
                                named.insert(n.text().to_string(), self.lower_type(&t));
                            }
                        }
                        _ => {}
                    }
                }
                per.insert(tr, (pos, named));
            }
        }
        if !per.is_empty() {
            self.param_trait_args.insert(name.to_string(), per);
        }
    }

    pub(super) fn leave_generics(&mut self) {
        self.generics.clear();
        self.param_bounds.clear();
        self.param_trait_args.clear();
        self.lenient_projs.clear();
        self.const_tys.clear();
        self.from_args.clear();
    }

    /// Fresh type variables for a generic function's parameters, with the bounds to check once
    /// they're known.
    pub(super) fn instantiate(
        &mut self,
        node: &SyntaxNode,
        type_params: &[TypeParam],
        params: &[Ty],
        ret: &Ty,
        turbofish: &[Ty],
    ) -> (Vec<Ty>, Ty) {
        let mut m = HashMap::new();
        self.last_type_args.clear();
        for (i, (name, bounds)) in type_params.iter().enumerate() {
            let v = self.fresh(VarKind::General);
            self.last_type_args.push(v.clone());
            if let Some(t) = turbofish.get(i) {
                self.unify(&v, t);
            }
            for b in bounds {
                self.pending_bounds
                    .push((v.clone(), b.clone(), node.clone(), self.base));
            }
            m.insert(name.clone(), v);
        }
        // Each bound's associated types (`S::Unit`): variables, settled once `S` is known; they
        // follow the type arguments (for the `{:?}` formatters a function takes).
        for (s, tr, a) in self.proj_names(type_params) {
            let Some(sv) = m.get(&s).cloned() else {
                continue;
            };
            let x = self.fresh(VarKind::General);
            self.pending_projections
                .push((sv, tr, a.clone(), x.clone()));
            self.last_type_args.push(x.clone());
            m.insert(format!("{s}::{a}"), x);
        }
        // Any other `S::Unit` in the signature.
        let mut projs: Vec<String> = Vec::new();
        for t in params.iter().chain(std::iter::once(ret)) {
            t.walk(&mut |x| {
                if let Ty::Param(p) = x
                    && p.contains("::")
                    && !projs.contains(p)
                {
                    projs.push(p.clone());
                }
            });
        }
        for p in projs {
            if m.contains_key(&p) {
                continue;
            }
            let Some((s, assoc)) = p.split_once("::") else {
                continue;
            };
            let Some(sv) = m.get(s).cloned() else {
                continue;
            };
            let tr = type_params
                .iter()
                .find(|(n, _)| n == s)
                .and_then(|(_, bs)| {
                    bs.iter().find(|b| {
                        self.adts
                            .traits
                            .get(*b)
                            .is_some_and(|d| d.assoc_types.iter().any(|a| a == assoc))
                    })
                })
                .cloned();
            let Some(tr) = tr else { continue };
            let x = self.fresh(VarKind::General);
            self.pending_projections
                .push((sv, tr, assoc.to_string(), x.clone()));
            m.insert(p.clone(), x);
        }
        (
            params.iter().map(|p| subst(p, &m)).collect(),
            subst(ret, &m),
        )
    }

    /// Check the bounds of every generic call in the function.
    pub(super) fn resolve_bounds(&mut self) {
        let saved = self.base;
        for (t, b, node, base) in std::mem::take(&mut self.pending_bounds) {
            // (Calls inside macro arguments were parsed from the argument's text.)
            self.base = base;
            let z = self.zonk(&t);
            if matches!(z, Ty::Var(_) | Ty::Unknown) {
                continue;
            }
            if !self.satisfies(&z, &b) {
                let msg = format!(
                    "the trait bound `{}: {b}` is not satisfied",
                    self.describe(&z)
                );
                self.error(&node, msg);
            }
        }
        self.base = saved;
    }

    /// Whether type parameter `name` (in the function being checked) has bound `b`, directly or
    /// through a subtrait (`Ord` gives `PartialOrd`, `Copy` gives `Clone`, ...).
    pub(super) fn param_has(&self, name: &str, b: &str) -> bool {
        // A trait's own associated type or type parameter (`Shape::Unit`): unchecked.
        if self.is_template(name) {
            return true;
        }
        // `S::Unit`: what the trait declares (`type Unit: Debug;`).
        let declared: Vec<String>;
        let bounds = match name.split_once("::") {
            // (`I::Item` of an `I: IntoIterator`: a parameter of its own.)
            Some(_) if self.param_bounds.contains_key(name) => &self.param_bounds[name],
            Some((s, a)) => {
                let mut todo: Vec<String> = self
                    .bound_traits(s)
                    .iter()
                    .filter_map(|t| self.adts.traits.get(t)?.assoc_bounds.get(a).cloned())
                    .flatten()
                    .collect();
                // (A user trait's supertraits too: `type Bound: Bound;` with
                // `trait Bound: PartialOrd`.)
                let mut all: Vec<String> = Vec::new();
                while let Some(t) = todo.pop() {
                    if all.contains(&t) {
                        continue;
                    }
                    if let Some(d) = self.adts.traits.get(&t) {
                        todo.extend(d.supers.iter().cloned());
                    }
                    all.push(t);
                }
                declared = all;
                &declared
            }
            None => match self.param_bounds.get(name) {
                Some(b) => b,
                None => return false,
            },
        };
        bounds.iter().any(|x| {
            x == b
                || matches!(
                    (x.as_str(), b),
                    ("Ord", "PartialOrd" | "Eq" | "PartialEq")
                        | ("Eq", "PartialEq")
                        | ("PartialOrd", "PartialEq")
                        | ("Copy", "Clone")
                        | ("Display", "ToString")
                )
        })
    }

    /// Whether a concrete type meets a bound.
    fn satisfies(&self, t: &Ty, b: &str) -> bool {
        match b {
            "Sized" | "Send" | "Sync" | "Unpin" => true,
            "FromStr" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str => true,
                Ty::Adt(n, _) => self.adts.method(n, "from_str").is_some(),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            "Sum" | "Product" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => true,
                Ty::Adt(n, _) => self.adts.method(n, &b.to_lowercase()).is_some(),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            // (`From<X>`'s `X` is checked at `T::from(x)`.)
            "From" => true,
            "Display" | "ToString" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str => true,
                Ty::Adt(n, _) => self.adts.defs.get(n).is_some_and(|d| d.display),
                Ty::Extern(n) => super::is_std_error(n),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            "Add" | "Sub" | "Mul" | "Div" | "Rem" | "AddAssign" | "SubAssign" | "MulAssign"
            | "DivAssign" | "RemAssign" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => true,
                Ty::Str => matches!(b, "Add" | "AddAssign"),
                Ty::Param(n) => self.param_has(n, b),
                // `impl Add for V` (with `V` on the right, as `T: Add<Output = T>` uses it).
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                _ => false,
            },
            "BitAnd" | "BitOr" | "BitXor" | "BitAndAssign" | "BitOrAssign" | "BitXorAssign"
            | "Not" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Bool => true,
                Ty::Param(n) => self.param_has(n, b),
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                _ => false,
            },
            "Shl" | "Shr" | "ShlAssign" | "ShrAssign" => match t {
                Ty::Int(_) | Ty::IntLit => true,
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            "Neg" => match t {
                Ty::Int(k) => k.is_signed(),
                Ty::IntLit | Ty::Float { .. } => true,
                Ty::Param(n) => self.param_has(n, b),
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                _ => false,
            },
            tr if self.adts.traits.contains_key(tr) => self.implements_user_trait(t, tr),
            "Copy" => match t {
                Ty::Vec(_) | Ty::Str | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..) => false,
                other => self.implements(other, "Copy"),
            },
            other => self.implements(t, other),
        }
    }

    /// `a < b`, `a + b` on a `T`: the bound must allow it.
    pub(super) fn require_param_bound(&mut self, node: &SyntaxNode, t: &Ty, b: &str, what: &str) {
        if let Ty::Param(n) = self.shallow(t)
            && !self.param_has(&n, b)
        {
            // (An associated type's bounds are the trait's: `type Unit: Add<Output = ..>;`.)
            let msg = match n.split_once("::") {
                Some((_, a)) => format!(
                    "{what} on associated type `{n}` needs a bound: declare it in the trait (`type {a}: {b}`)"
                ),
                None => format!(
                    "{what} on type parameter `{n}` needs a bound: add `{n}: {b}` to the function's generics"
                ),
            };
            self.error(node, msg);
        }
    }

    /// Whether `adt` has an `impl {tr} for adt` (`Add`, `Neg`, `AddAssign`, ..).
    pub(super) fn has_op_impl(&self, adt: &str, tr: &str) -> bool {
        self.adts
            .op_impls
            .get(adt)
            .is_some_and(|v| v.iter().any(|i| i.trait_name == tr))
    }

    /// `a + b` (or `a += b`, `assign`) with `a: T`: needs `T: Add` (etc.); the result is `T`.
    /// The function takes `T`'s class, which says what kind of number it is (integer division
    /// truncates, results wrap) or calls a struct's own `add`: `/`, `%`, `<<` and `>>` need it;
    /// the others fall back to JS arithmetic without it.
    pub(super) fn param_arith(
        &mut self,
        b: &ast::BinExpr,
        op: ast::ArithOp,
        name: &str,
        lt: &Ty,
        rhs: &ast::Expr,
        assign: bool,
    ) -> Ty {
        let shift = matches!(op, ast::ArithOp::Shl | ast::ArithOp::Shr);
        if shift {
            // (`T: Shl<u32>`: the amount is any integer, or a `T`.)
            let rt = self.infer(rhs, None);
            if !self.is_integer(&rt) && !matches!(self.shallow(&rt), Ty::Param(_)) {
                let msg = format!(
                    "no implementation for `{} {op} {}`",
                    self.describe(lt),
                    self.describe(&rt)
                );
                self.error(rhs.syntax(), msg);
            }
        } else {
            self.check(rhs, lt);
        }
        let bound = match op {
            ast::ArithOp::Add => "Add",
            ast::ArithOp::Sub => "Sub",
            ast::ArithOp::Mul => "Mul",
            ast::ArithOp::Div => "Div",
            ast::ArithOp::Rem => "Rem",
            ast::ArithOp::BitAnd => "BitAnd",
            ast::ArithOp::BitOr => "BitOr",
            ast::ArithOp::BitXor => "BitXor",
            ast::ArithOp::Shl => "Shl",
            ast::ArithOp::Shr => "Shr",
        };
        let bound = if assign {
            format!("{bound}Assign")
        } else {
            bound.to_string()
        };
        let sym = if assign {
            format!("`{op}=`")
        } else {
            format!("`{op}`")
        };
        self.require_param_bound(b.syntax(), lt, &bound, &sym);
        // Division and shifts depend on the number kind (integer division, the width).
        let needs_kind = shift || matches!(op, ast::ArithOp::Div | ast::ArithOp::Rem);
        self.param_class(name, b.syntax(), needs_kind.then_some(sym.as_str()));
        lt.clone()
    }

    /// Methods on a value of type parameter `T`.
    pub(super) fn param_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        name: &str,
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        // A method of a user trait in `T`'s bounds.
        let bounds = self.param_bounds.get(name).cloned().unwrap_or_default();
        if let Some(sig) = bounds.iter().find_map(|b| self.trait_method(b, method))
            && sig.receiver.is_some()
        {
            return self.call_trait_method(m, recv, sig, Ty::Param(name.to_string()), args);
        }
        let t = Ty::Param(name.to_string());
        let need = |c: &mut Self, b: &str| {
            if !c.param_has(name, b) {
                c.error(
                    m.syntax(),
                    format!("`{method}` on type parameter `{name}` needs a bound: add `{name}: {b}` to the function's generics"),
                );
            }
        };
        let sig = |c: &mut Self, params: &[Ty], ret: Ty| {
            c.arg_count(m.syntax(), "method", params.len(), args.len());
            for (a, p) in args.iter().zip(params) {
                c.check(a, p);
            }
            ret
        };
        match method {
            "clone" => {
                need(self, "Clone");
                sig(self, &[], t)
            }
            "to_string" => {
                need(self, "ToString");
                sig(self, &[], Ty::Str)
            }
            "eq" | "ne" => {
                need(self, "PartialEq");
                sig(self, &[t], Ty::Bool)
            }
            "cmp" => {
                need(self, "Ord");
                sig(self, &[t], Ty::Ordering)
            }
            "partial_cmp" => {
                need(self, "PartialOrd");
                sig(self, &[t], Ty::Option(Box::new(Ty::Ordering)))
            }
            "max" | "min" => {
                need(self, "Ord");
                sig(self, &[t.clone()], t)
            }
            _ => {
                for a in args {
                    self.infer(a, None);
                }
                self.error(
                    m.syntax(),
                    format!("no method named `{method}` found for type parameter `{name}` in the current scope (only methods from its bounds are available)"),
                );
                Ty::Unknown
            }
        }
    }
}
