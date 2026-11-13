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
    "Neg",
    "AddAssign",
    "SubAssign",
    "MulAssign",
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
        Ty::Param(n) => m.get(n).cloned().unwrap_or_else(|| t.clone()),
        Ty::Vec(e) => Ty::Vec(s(e)),
        Ty::Option(e) => Ty::Option(s(e)),
        Ty::Set(e) => Ty::Set(s(e)),
        Ty::Future(e) => Ty::Future(s(e)),
        Ty::Map(k, v) => Ty::Map(s(k), s(v)),
        Ty::Entry(k, v) => Ty::Entry(s(k), s(v)),
        Ty::Result(k, v) => Ty::Result(s(k), s(v)),
        Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| subst(e, m)).collect()),
        Ty::Iter { elem, mutable } => Ty::Iter {
            elem: s(elem),
            mutable: *mutable,
        },
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
        let mut params = Vec::new();
        for (name, lists, node) in type_params(f) {
            if let Some(closure) = lists.iter().find_map(|l| self.lower_fn_bounds(l)) {
                self.generics.insert(name, closure);
                continue;
            }
            let mut bounds = Vec::new();
            for (b, bnode) in bound_names(&lists) {
                if KNOWN_BOUNDS.contains(&b.as_str()) || self.adts.traits.contains_key(&b) {
                    bounds.push(b);
                } else if report {
                    self.error(
                        &bnode,
                        format!("the trait bound `{b}` is not supported yet on type parameters (supported: Copy, Clone, Debug, Display, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Add, Sub, Mul)"),
                    );
                }
            }
            let _ = node;
            self.generics.insert(name.clone(), Ty::Param(name.clone()));
            self.param_bounds.insert(name.clone(), bounds.clone());
            params.push((name, bounds));
        }
        params
    }

    pub(super) fn leave_generics(&mut self) {
        self.generics.clear();
        self.param_bounds.clear();
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
        for (i, (name, bounds)) in type_params.iter().enumerate() {
            let v = self.fresh(VarKind::General);
            if let Some(t) = turbofish.get(i) {
                self.unify(&v, t);
            }
            for b in bounds {
                self.pending_bounds
                    .push((v.clone(), b.clone(), node.clone()));
            }
            m.insert(name.clone(), v);
        }
        (
            params.iter().map(|p| subst(p, &m)).collect(),
            subst(ret, &m),
        )
    }

    /// Check the bounds of every generic call in the function.
    pub(super) fn resolve_bounds(&mut self) {
        for (t, b, node) in std::mem::take(&mut self.pending_bounds) {
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
    }

    /// Whether type parameter `name` (in the function being checked) has bound `b`, directly or
    /// through a subtrait (`Ord` gives `PartialOrd`, `Copy` gives `Clone`, ...).
    pub(super) fn param_has(&self, name: &str, b: &str) -> bool {
        let Some(bounds) = self.param_bounds.get(name) else {
            return false;
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
            "Sized" => true,
            "Display" | "ToString" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str => true,
                Ty::Adt(n) => self.adts.defs.get(n).is_some_and(|d| d.display),
                Ty::Extern(n) => super::is_std_error(n),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            "Add" | "Sub" | "Mul" | "AddAssign" | "SubAssign" | "MulAssign" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => true,
                Ty::Str => matches!(b, "Add" | "AddAssign"),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            "Neg" => match t {
                Ty::Int(k) => k.is_signed(),
                Ty::IntLit | Ty::Float { .. } => true,
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            tr if self.adts.traits.contains_key(tr) => self.implements_user_trait(t, tr),
            "Copy" => match t {
                Ty::Vec(_) | Ty::Str | Ty::Set(_) | Ty::Map(..) => false,
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
            self.error(
                node,
                format!("{what} on type parameter `{n}` needs a bound: add `{n}: {b}` to the function's generics"),
            );
        }
    }

    /// `a + b` with `a: T`: needs `T: Add` (etc.); the result is `T`. Integer division can't be
    /// told from float division once types are erased, so `/` and `%` aren't allowed.
    pub(super) fn param_arith(
        &mut self,
        b: &ast::BinExpr,
        op: ast::ArithOp,
        name: &str,
        lt: &Ty,
        rhs: &ast::Expr,
    ) -> Ty {
        self.check(rhs, lt);
        let bound = match op {
            ast::ArithOp::Add => "Add",
            ast::ArithOp::Sub => "Sub",
            ast::ArithOp::Mul => "Mul",
            _ => {
                self.error(
                    b.syntax(),
                    format!("`{op}` on type parameter `{name}` isn't supported (generic code can't tell integer from float division once types are erased)"),
                );
                return lt.clone();
            }
        };
        self.require_param_bound(b.syntax(), lt, bound, &format!("`{op}`"));
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
