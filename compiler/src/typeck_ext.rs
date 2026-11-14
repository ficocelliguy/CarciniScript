//! Type checking for closures, iterators, `Ordering`, and the std methods that take closures.
//! A child module of `typeck`, so it works directly on the checker's state.

use std::collections::HashMap;

use ra_ap_syntax::{AstNode, SyntaxNode, ast};

use super::{Checker, Mutation, USIZE, VarKind};
use crate::ty::Ty;

impl Checker<'_> {
    // ------------------------------------------------------------ closures

    /// `|params| body`. Parameter and return types come from annotations, then from the expected
    /// closure type (so `v.iter().map(|x| x.len())` knows `x`), then from fresh variables.
    pub(super) fn closure(&mut self, c: &ast::ClosureExpr, expected: Option<&Ty>) -> Ty {
        if c.async_token().is_some() {
            self.error(c.syntax(), "async closures are not supported yet");
        }
        let params: Vec<ast::Param> = c
            .param_list()
            .map(|l| l.params().collect())
            .unwrap_or_default();
        let hint = match expected.map(|e| self.shallow(e)) {
            Some(Ty::Closure { params: ps, ret }) if ps.len() == params.len() => Some((ps, *ret)),
            _ => None,
        };
        self.scopes.push(HashMap::new());
        let mut ptys = Vec::new();
        for (i, p) in params.iter().enumerate() {
            let annotated = p.ty().map(|t| self.lower_type(&t));
            let t = match (annotated, &hint) {
                (Some(t), Some((ps, _))) => {
                    self.unify(&t, &ps[i]);
                    t
                }
                (Some(t), None) => t,
                (None, Some((ps, _))) => ps[i].clone(),
                (None, None) => self.fresh(VarKind::General),
            };
            if let Some(pat) = p.pat() {
                self.pat(&pat, &t);
            }
            ptys.push(t);
        }
        // Closure parameters are usually references (`&T`, `&mut T` from iter_mut/retain/
        // and_modify); writing through them with `*x = ..` is allowed.
        if let Some(scope) = self.scopes.last_mut() {
            for local in scope.values_mut() {
                local.reference.get_or_insert(true);
            }
        }
        let ret = match (c.ret_type().and_then(|r| r.ty()), &hint) {
            (Some(t), _) => self.lower_type(&t),
            (None, Some((_, r))) => r.clone(),
            (None, None) => self.fresh(VarKind::General),
        };
        let saved_ret = std::mem::replace(&mut self.ret_ty, ret.clone());
        let saved_async = std::mem::replace(&mut self.in_async, false);
        // A closure body can't `break` out of the loop around it.
        let saved_loops = std::mem::take(&mut self.loops);
        if let Some(body) = c.body() {
            let found = self.infer(&body, Some(&ret));
            if !self.unify(&found, &ret) {
                let msg = format!(
                    "mismatched types: expected `{}`, found `{}`",
                    self.describe(&ret),
                    self.describe(&found)
                );
                self.error(body.syntax(), msg);
            }
        }
        self.ret_ty = saved_ret;
        self.in_async = saved_async;
        self.loops = saved_loops;
        self.scopes.pop();
        Ty::Closure {
            params: ptys,
            ret: Box::new(ret),
        }
    }

    /// Check `arg` as a closure (or function item) of the given shape; returns its return type.
    pub(super) fn closure_arg(&mut self, arg: Option<&ast::Expr>, params: Vec<Ty>, ret: Ty) -> Ty {
        if let Some(a) = arg {
            let expected = Ty::Closure {
                params,
                ret: Box::new(ret.clone()),
            };
            self.check(a, &expected);
        }
        ret
    }

    /// `impl Fn(A) -> R`, `&dyn FnMut(A)`, `fn(A) -> R`.
    pub(super) fn lower_fn_type(&mut self, t: &ast::Type) -> Option<Ty> {
        let (params, ret): (Vec<ast::Type>, Option<ast::Type>) = match t {
            ast::Type::FnPtrType(f) => (
                f.param_list()?.params().filter_map(|p| p.ty()).collect(),
                f.ret_type().and_then(|r| r.ty()),
            ),
            ast::Type::ImplTraitType(i) => return self.lower_fn_bounds(&i.type_bound_list()?),
            ast::Type::DynTraitType(d) => return self.lower_fn_bounds(&d.type_bound_list()?),
            _ => return None,
        };
        let params = params.iter().map(|p| self.lower_type(p)).collect();
        let ret = ret.map(|r| self.lower_type(&r)).unwrap_or(Ty::Unit);
        Some(Ty::Closure {
            params,
            ret: Box::new(ret),
        })
    }

    /// `Fn(A, B) -> R` (or `FnMut` / `FnOnce`) among `bounds`, as a closure type.
    pub(super) fn lower_fn_bounds(&mut self, bounds: &ast::TypeBoundList) -> Option<Ty> {
        let seg = bounds.bounds().find_map(|b| match b.ty()? {
            ast::Type::PathType(p) => {
                let seg = p.path()?.segment()?;
                let name = seg.name_ref()?.text().to_string();
                matches!(name.as_str(), "Fn" | "FnMut" | "FnOnce").then_some(seg)
            }
            _ => None,
        })?;
        let params: Vec<ast::Type> = seg
            .parenthesized_arg_list()?
            .type_args()
            .filter_map(|a| a.ty())
            .collect();
        let ret = seg.ret_type().and_then(|r| r.ty());
        let params = params.iter().map(|p| self.lower_type(p)).collect();
        let ret = ret.map(|r| self.lower_type(&r)).unwrap_or(Ty::Unit);
        Some(Ty::Closure {
            params,
            ret: Box::new(ret),
        })
    }

    // ------------------------------------------------------------ helpers

    /// The element type of something a `for` loop / `zip` / `extend` / `flat_map` accepts.
    pub(super) fn iter_elem(&self, t: &Ty) -> Option<Ty> {
        match self.shallow(t) {
            Ty::Adt(n, targs) if self.adts.iter_items.contains_key(&n) => {
                let map = self.adt_subst(&n, &targs);
                Some(super::generic::subst(&self.adts.iter_items[&n], &map))
            }
            Ty::Coll(_, view) => self.iter_elem(&view),
            Ty::Vec(e) | Ty::Set(e) | Ty::Option(e) => Some(*e),
            Ty::Iter { elem, .. } => Some(*elem),
            Ty::Map(k, v) => Some(Ty::Tuple(vec![*k, *v])),
            Ty::Unknown => Some(Ty::Unknown),
            _ => None,
        }
    }

    /// Reports a missing `Ord` (floats anywhere in the type) for sorting and min/max.
    pub(super) fn require_ord(&mut self, node: &SyntaxNode, t: &Ty, what: &str) {
        let z = self.zonk(t);
        if z.contains_float() || self.var_kind(t) == Some(VarKind::Float) {
            let msg = format!(
                "the trait bound `{}: Ord` is not satisfied (floats aren't totally ordered; use `{what}_by(|a, b| a.partial_cmp(b).unwrap())`)",
                self.describe(t)
            );
            self.error(node, msg);
        }
    }

    pub(super) fn turbofish(&mut self, m: &ast::MethodCallExpr) -> Option<Ty> {
        use ra_ap_syntax::ast::HasGenericArgs;
        match m.generic_arg_list()?.generic_args().next()? {
            ast::GenericArg::TypeArg(ta) => ta.ty().map(|t| self.lower_type(&t)),
            _ => None,
        }
    }

    /// Whether `collect` can build `target` from elements of `elem`. `None` means the target isn't
    /// known yet.
    fn collect_into(&mut self, target: &Ty, elem: &Ty) -> Option<bool> {
        Some(match self.shallow(target) {
            Ty::Vec(x) | Ty::Set(x) => self.unify(&x, elem),
            Ty::Map(k, v) => self.unify(&Ty::Tuple(vec![*k, *v]), elem),
            Ty::Coll(_, view) => return self.collect_into(&view, elem),
            // `Result<C, E>` from `Result<T, E>` items, collecting the `T`s into `C`.
            Ty::Result(inner, err) => {
                let item = self.fresh(VarKind::General);
                if !self.unify(elem, &Ty::Result(Box::new(item.clone()), err)) {
                    return Some(false);
                }
                return self.collect_into(&inner, &item);
            }
            Ty::Option(inner) => {
                let item = self.fresh(VarKind::General);
                if !self.unify(elem, &Ty::Option(Box::new(item.clone()))) {
                    return Some(false);
                }
                return self.collect_into(&inner, &item);
            }
            Ty::Str => matches!(
                self.shallow(elem),
                Ty::Char | Ty::Str | Ty::Unknown | Ty::Var(_)
            ),
            Ty::Unknown => true,
            Ty::Var(_) => return None,
            _ => false,
        })
    }

    /// Resolve `collect()` calls whose target type was only known later in the function.
    pub(super) fn resolve_collects(&mut self) {
        let saved = self.base;
        for (target, elem, node, base) in std::mem::take(&mut self.pending_collects) {
            self.base = base;
            match self.collect_into(&target, &elem) {
                Some(true) => {}
                Some(false) => {
                    let msg = format!(
                        "a value of type `{}` cannot be built from an iterator over `{}`",
                        self.describe(&target),
                        self.describe(&elem)
                    );
                    self.error(&node, msg);
                }
                None => self.error(
                    &node,
                    "type annotations needed: say what to collect into, e.g. `collect::<Vec<_>>()`",
                ),
            }
        }
        self.base = saved;
    }

    // ------------------------------------------------------------ methods

    /// Methods that take closures or produce iterators. `None` when the method isn't one of them.
    pub(super) fn method_ext(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        rt: &Ty,
        method: &str,
        args: &[ast::Expr],
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        let a0 = args.first();
        let a1 = args.get(1);
        let count = |c: &mut Self, n: usize| c.arg_count(m.syntax(), "method", n, args.len());
        let fresh = |c: &mut Self| c.fresh(VarKind::General);
        let iter = |elem: Ty| Ty::Iter {
            elem: Box::new(elem),
            mutable: false,
        };
        if let Some(t) = self.std_method(m, recv, rt, method, args) {
            return Some(t);
        }
        match self.shallow(rt) {
            Ty::Result(t, e) => return self.result_method(m, *t, *e, method, args),
            Ty::Option(inner) if matches!(method, "ok_or" | "ok_or_else") => {
                return self.option_to_result(m, *inner, method, args);
            }
            Ty::Formatter if method == "write_str" => {
                self.arg_count(m.syntax(), "method", 1, args.len());
                if let Some(a) = args.first() {
                    self.check(a, &Ty::Str);
                }
                return Some(Ty::Result(
                    Box::new(Ty::Unit),
                    Box::new(Ty::Extern("fmt::Error".into())),
                ));
            }
            Ty::Str if method == "parse" => {
                let target = self.turbofish(m);
                return Some(self.parse_method(m, target, expected));
            }
            _ => {}
        }
        Some(match (self.shallow(rt), method) {
            // ---- iterator sources
            (Ty::Str, "chars") => {
                count(self, 0);
                iter(Ty::Char)
            }
            (Ty::Str, "split") => {
                count(self, 1);
                if let Some(a) = a0 {
                    let at = self.infer(a, None);
                    if !matches!(self.shallow(&at), Ty::Str | Ty::Char | Ty::Unknown) {
                        let msg = format!(
                            "expected a `&str` or `char` pattern, found `{}`",
                            self.describe(&at)
                        );
                        self.error(a.syntax(), msg);
                    }
                }
                iter(Ty::Str)
            }
            (Ty::Str, "split_whitespace" | "lines") => {
                count(self, 0);
                iter(Ty::Str)
            }
            // `s.split_at(mid)`: both halves, by byte offset.
            (Ty::Str, "split_at") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                Ty::Tuple(vec![Ty::Str, Ty::Str])
            }
            (Ty::Vec(e), "windows" | "chunks" | "chunks_exact") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                iter(Ty::Vec(e))
            }

            // ---- Vec methods taking closures
            (Ty::Vec(e), "sort_by" | "sort_unstable_by") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                self.closure_arg(a0, vec![(*e).clone(), *e], Ty::Ordering);
                Ty::Unit
            }
            (Ty::Vec(e), "sort_by_key" | "sort_unstable_by_key" | "sort_by_cached_key") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                let k = fresh(self);
                self.closure_arg(a0, vec![*e], k.clone());
                self.require_ord(m.syntax(), &k, "sort");
                Ty::Unit
            }
            (Ty::Vec(e), "retain") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                self.closure_arg(a0, vec![*e], Ty::Bool);
                Ty::Unit
            }
            (Ty::Vec(_), "dedup") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 0);
                Ty::Unit
            }
            (Ty::Vec(e), "dedup_by_key") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                let k = fresh(self);
                self.closure_arg(a0, vec![*e], k);
                Ty::Unit
            }
            (Ty::Vec(e) | Ty::Set(e), "extend") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                self.extend_arg(a0, &e);
                Ty::Unit
            }

            // ---- HashSet / HashMap
            (Ty::Set(e), "retain") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                self.closure_arg(a0, vec![*e], Ty::Bool);
                Ty::Unit
            }
            (Ty::Set(e), "union" | "intersection" | "difference" | "symmetric_difference") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Set(e.clone()));
                }
                iter(*e)
            }
            (Ty::Map(k, v), "retain") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                self.closure_arg(a0, vec![*k, *v], Ty::Bool);
                Ty::Unit
            }
            (Ty::Map(k, v), "extend") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                self.extend_arg(a0, &Ty::Tuple(vec![*k, *v]));
                Ty::Unit
            }
            (Ty::Entry(k, v), "and_modify") => {
                count(self, 1);
                self.closure_arg(a0, vec![(*v).clone()], Ty::Unit);
                Ty::Entry(k, v)
            }
            (Ty::Entry(_, v), "or_insert_with")
                if matches!(a0, Some(ast::Expr::ClosureExpr(_))) =>
            {
                count(self, 1);
                self.closure_arg(a0, vec![], (*v).clone());
                *v
            }
            (Ty::Entry(k, v), "or_insert_with_key") => {
                count(self, 1);
                self.closure_arg(a0, vec![*k], (*v).clone());
                *v
            }

            // ---- Option methods taking closures
            (Ty::Option(t), m2) if is_option_closure_method(m2) => {
                let t = *t;
                let opt = |x: Ty| Ty::Option(Box::new(x));
                match m2 {
                    "map" => {
                        count(self, 1);
                        let r = fresh(self);
                        opt(self.closure_arg(a0, vec![t], r))
                    }
                    "and_then" => {
                        count(self, 1);
                        let r = fresh(self);
                        self.closure_arg(a0, vec![t], opt(r.clone()));
                        opt(r)
                    }
                    "filter" => {
                        count(self, 1);
                        self.closure_arg(a0, vec![t.clone()], Ty::Bool);
                        opt(t)
                    }
                    "unwrap_or_else" => {
                        count(self, 1);
                        self.closure_arg(a0, vec![], t)
                    }
                    "map_or" => {
                        count(self, 2);
                        let r = a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown);
                        self.closure_arg(a1, vec![t], r)
                    }
                    "map_or_else" => {
                        count(self, 2);
                        let r = fresh(self);
                        self.closure_arg(a0, vec![], r.clone());
                        self.closure_arg(a1, vec![t], r)
                    }
                    "is_some_and" | "is_none_or" => {
                        count(self, 1);
                        self.closure_arg(a0, vec![t], Ty::Bool)
                    }
                    "or" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &opt(t.clone()));
                        }
                        opt(t)
                    }
                    "or_else" => {
                        count(self, 1);
                        self.closure_arg(a0, vec![], opt(t))
                    }
                    "zip" => {
                        count(self, 1);
                        let u = fresh(self);
                        if let Some(a) = a0 {
                            self.check(a, &opt(u.clone()));
                        }
                        opt(Ty::Tuple(vec![t, u]))
                    }
                    "inspect" => {
                        count(self, 1);
                        self.closure_arg(a0, vec![t.clone()], Ty::Unit);
                        opt(t)
                    }
                    _ => unreachable!(),
                }
            }

            // ---- bool
            (Ty::Bool, "then") => {
                count(self, 1);
                let r = fresh(self);
                Ty::Option(Box::new(self.closure_arg(a0, vec![], r)))
            }
            (Ty::Bool, "then_some") => {
                count(self, 1);
                let r = a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown);
                Ty::Option(Box::new(r))
            }

            // ---- char
            (Ty::Char, m2) if char_predicate(m2).is_some() => {
                count(self, 0);
                Ty::Bool
            }
            (Ty::Char, "to_ascii_uppercase" | "to_ascii_lowercase") => {
                count(self, 0);
                Ty::Char
            }
            // An iterator of chars (`'ß'.to_uppercase()` is "SS").
            (Ty::Char, "to_uppercase" | "to_lowercase") => {
                count(self, 0);
                iter(Ty::Char)
            }
            (Ty::Char, "to_digit" | "is_digit") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Int(crate::ty::IntKind::U32));
                }
                if method == "is_digit" {
                    Ty::Bool
                } else {
                    Ty::Option(Box::new(Ty::Int(crate::ty::IntKind::U32)))
                }
            }

            // ---- comparisons
            (t, "cmp") if !matches!(t, Ty::Float { .. } | Ty::Iter { .. } | Ty::Closure { .. }) => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &t);
                }
                self.require_ord(m.syntax(), &t, "max");
                Ty::Ordering
            }
            (t, "partial_cmp") if !matches!(t, Ty::Iter { .. } | Ty::Closure { .. }) => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &t);
                }
                Ty::Option(Box::new(Ty::Ordering))
            }
            (Ty::Ordering, m2) => match m2 {
                "reverse" => {
                    count(self, 0);
                    Ty::Ordering
                }
                "then" => {
                    count(self, 1);
                    if let Some(a) = a0 {
                        self.check(a, &Ty::Ordering);
                    }
                    Ty::Ordering
                }
                "then_with" => {
                    count(self, 1);
                    self.closure_arg(a0, vec![], Ty::Ordering)
                }
                "is_lt" | "is_le" | "is_gt" | "is_ge" | "is_eq" | "is_ne" => {
                    count(self, 0);
                    Ty::Bool
                }
                _ => return None,
            },

            // ---- iterators
            (Ty::Iter { elem, mutable }, m2) => {
                if m2 == "next" {
                    // Advancing an iterator held in a variable needs `let mut`.
                    self.check_mutable(recv, super::Mutation::Borrow);
                    self.arg_count(m.syntax(), "method", 0, args.len());
                    return Some(Ty::Option(elem));
                }
                return self.iter_method(m, m2, *elem, mutable, args, expected);
            }
            _ => return None,
        })
    }

    /// `v.extend(iter)`: anything iterable whose elements match.
    fn extend_arg(&mut self, arg: Option<&ast::Expr>, elem: &Ty) {
        let Some(a) = arg else { return };
        let t = self.infer(a, None);
        match self.iter_elem(&t) {
            Some(e) => {
                if !self.unify(&e, elem) {
                    let msg = format!(
                        "mismatched types: expected items of type `{}`, found `{}`",
                        self.describe(elem),
                        self.describe(&e)
                    );
                    self.error(a.syntax(), msg);
                }
            }
            None => {
                let msg = format!("`{}` is not an iterator", self.describe(&t));
                self.error(a.syntax(), msg);
            }
        }
    }

    fn iter_method(
        &mut self,
        m: &ast::MethodCallExpr,
        method: &str,
        e: Ty,
        mutable: bool,
        args: &[ast::Expr],
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        let a0 = args.first();
        let count = |c: &mut Self, n: usize| c.arg_count(m.syntax(), "method", n, args.len());
        let fresh = |c: &mut Self| c.fresh(VarKind::General);
        let iter = |elem: Ty| Ty::Iter {
            elem: Box::new(elem),
            mutable: false,
        };
        let opt = |x: Ty| Ty::Option(Box::new(x));
        // Adapters that hand on the same items keep an `iter_mut()` mutable.
        let keep = |elem: Ty| Ty::Iter {
            elem: Box::new(elem),
            mutable,
        };
        // `iter_mut()` over JS objects (structs, tuples, ..) yields the objects themselves, so
        // writes through any adapter reach them; over numbers and strings they'd be copies.
        let objects = {
            let z = self.zonk(&e);
            !z.is_js_primitive() && z != Ty::Unknown
        };
        if mutable && !objects && !matches!(method, "for_each" | "enumerate") {
            self.error(
                m.syntax(),
                format!("`iter_mut()` followed by `.{method}()` isn't supported yet (only `.for_each(..)`); use a `for` loop"),
            );
            for a in args {
                self.infer(a, None);
            }
            return Some(Ty::Unknown);
        }
        let usize_arg = |c: &mut Self| {
            count(c, 1);
            if let Some(a) = a0 {
                c.check(a, &USIZE);
            }
        };
        Some(match method {
            "map" => {
                count(self, 1);
                let r = fresh(self);
                iter(self.closure_arg(a0, vec![e], r))
            }
            "filter" | "take_while" | "skip_while" => {
                count(self, 1);
                self.closure_arg(a0, vec![e.clone()], Ty::Bool);
                keep(e)
            }
            "filter_map" => {
                count(self, 1);
                let r = fresh(self);
                self.closure_arg(a0, vec![e], opt(r.clone()));
                iter(r)
            }
            "flat_map" => {
                count(self, 1);
                let r = fresh(self);
                let produced = self.closure_arg(a0, vec![e], r);
                match self.iter_elem(&produced) {
                    Some(x) => iter(x),
                    None => {
                        let msg = format!(
                            "`flat_map` needs a closure returning something iterable, found `{}`",
                            self.describe(&produced)
                        );
                        self.error(m.syntax(), msg);
                        iter(Ty::Unknown)
                    }
                }
            }
            "flatten" => {
                count(self, 0);
                match self.iter_elem(&e) {
                    Some(x) => iter(x),
                    None => {
                        let msg = format!(
                            "`flatten` needs items that are iterable, found `{}`",
                            self.describe(&e)
                        );
                        self.error(m.syntax(), msg);
                        iter(Ty::Unknown)
                    }
                }
            }
            "inspect" => {
                count(self, 1);
                self.closure_arg(a0, vec![e.clone()], Ty::Unit);
                iter(e)
            }
            "enumerate" => {
                count(self, 0);
                Ty::Iter {
                    elem: Box::new(Ty::Tuple(vec![USIZE, e])),
                    mutable,
                }
            }
            "zip" => {
                count(self, 1);
                let other = a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown);
                let u = self.iter_elem(&other).unwrap_or_else(|| {
                    let msg = format!("`{}` is not an iterator", self.describe(&other));
                    self.error(m.syntax(), msg);
                    Ty::Unknown
                });
                keep(Ty::Tuple(vec![e, u]))
            }
            "chain" => {
                count(self, 1);
                self.extend_arg(a0, &e);
                keep(e)
            }
            "rev" | "cycle" => {
                count(self, 0);
                keep(e)
            }
            "cloned" | "copied" => {
                count(self, 0);
                iter(e)
            }
            "take" | "skip" | "step_by" => {
                usize_arg(self);
                keep(e)
            }
            "count" => {
                count(self, 0);
                USIZE
            }
            // `chars.as_str()` (the rest), `c.to_uppercase().to_string()`.
            "as_str" | "to_string" if self.shallow(&e) == Ty::Char => {
                count(self, 0);
                Ty::Str
            }
            "last" => {
                count(self, 0);
                opt(e)
            }
            "nth" => {
                usize_arg(self);
                opt(e)
            }
            "sum" | "product" => {
                count(self, 0);
                let target = self
                    .turbofish(m)
                    .or_else(|| expected.cloned())
                    .unwrap_or_else(|| e.clone());
                if !self.unify(&target, &e) {
                    let msg = format!(
                        "`{}` can't be the {method} of `{}` items",
                        self.describe(&target),
                        self.describe(&e)
                    );
                    self.error(m.syntax(), msg);
                }
                let z = self.zonk(&target);
                if !(z.is_numeric() || z == Ty::Unknown || self.var_kind(&target).is_some()) {
                    let msg = format!("`{}` can't be summed or multiplied", self.describe(&target));
                    self.error(m.syntax(), msg);
                }
                target
            }
            "min" | "max" => {
                count(self, 0);
                self.require_ord(m.syntax(), &e, method);
                opt(e)
            }
            "min_by_key" | "max_by_key" => {
                count(self, 1);
                let k = fresh(self);
                self.closure_arg(a0, vec![e.clone()], k.clone());
                self.require_ord(m.syntax(), &k, &method[..3]);
                opt(e)
            }
            "min_by" | "max_by" => {
                count(self, 1);
                self.closure_arg(a0, vec![e.clone(), e.clone()], Ty::Ordering);
                opt(e)
            }
            "any" | "all" => {
                count(self, 1);
                self.closure_arg(a0, vec![e], Ty::Bool)
            }
            "find" => {
                count(self, 1);
                self.closure_arg(a0, vec![e.clone()], Ty::Bool);
                opt(e)
            }
            "position" | "rposition" => {
                count(self, 1);
                self.closure_arg(a0, vec![e], Ty::Bool);
                opt(USIZE)
            }
            // `(Vec<T>, Vec<T>)`: the items the closure accepts, then the rest.
            "partition" => {
                count(self, 1);
                self.closure_arg(a0, vec![e.clone()], Ty::Bool);
                Ty::Tuple(vec![Ty::Vec(Box::new(e.clone())), Ty::Vec(Box::new(e))])
            }
            "find_map" => {
                count(self, 1);
                let r = fresh(self);
                self.closure_arg(a0, vec![e], opt(r.clone()));
                opt(r)
            }
            "fold" => {
                count(self, 2);
                let acc = args
                    .first()
                    .map(|a| self.infer(a, expected))
                    .unwrap_or(Ty::Unknown);
                self.closure_arg(args.get(1), vec![acc.clone(), e], acc)
            }
            "reduce" => {
                count(self, 1);
                self.closure_arg(a0, vec![e.clone(), e.clone()], e.clone());
                opt(e)
            }
            "for_each" => {
                count(self, 1);
                self.closure_arg(a0, vec![e], Ty::Unit);
                Ty::Unit
            }
            "collect" => {
                count(self, 0);
                let target = match self.turbofish(m) {
                    Some(t) => t,
                    None => match expected.map(|x| self.shallow(x)) {
                        Some(
                            t @ (Ty::Vec(_)
                            | Ty::Set(_)
                            | Ty::Map(..)
                            | Ty::Str
                            | Ty::Result(..)
                            | Ty::Option(_)
                            | Ty::Coll(..)),
                        ) => t,
                        _ => fresh(self),
                    },
                };
                match self.collect_into(&target, &e) {
                    Some(true) => {}
                    Some(false) => {
                        let msg = format!(
                            "a value of type `{}` cannot be built from an iterator over `{}`",
                            self.describe(&target),
                            self.describe(&e)
                        );
                        self.error(m.syntax(), msg);
                    }
                    None => self.pending_collects.push((
                        target.clone(),
                        e,
                        m.syntax().clone(),
                        self.base,
                    )),
                }
                target
            }
            _ => return None,
        })
    }
}

pub fn is_option_closure_method(m: &str) -> bool {
    matches!(
        m,
        "map"
            | "and_then"
            | "filter"
            | "unwrap_or_else"
            | "map_or"
            | "map_or_else"
            | "is_some_and"
            | "is_none_or"
            | "or"
            | "or_else"
            | "zip"
            | "inspect"
    )
}

/// `char` classification methods, as a JS regex the (one-char) string must match.
pub fn char_predicate(m: &str) -> Option<&'static str> {
    Some(match m {
        "is_alphabetic" => r"/^\p{Alphabetic}$/u",
        "is_numeric" => r"/^\p{N}$/u",
        "is_alphanumeric" => r"/^[\p{Alphabetic}\p{N}]$/u",
        "is_whitespace" => r"/^\p{White_Space}$/u",
        "is_uppercase" => r"/^\p{Uppercase}$/u",
        "is_lowercase" => r"/^\p{Lowercase}$/u",
        "is_control" => r"/^\p{Cc}$/u",
        "is_ascii" => r"/^[\0-\x7f]$/",
        "is_ascii_digit" => r"/^[0-9]$/",
        "is_ascii_hexdigit" => r"/^[0-9a-fA-F]$/",
        "is_ascii_alphabetic" => r"/^[a-zA-Z]$/",
        "is_ascii_alphanumeric" => r"/^[a-zA-Z0-9]$/",
        "is_ascii_uppercase" => r"/^[A-Z]$/",
        "is_ascii_lowercase" => r"/^[a-z]$/",
        "is_ascii_punctuation" => r"/^[!-\/:-@\[-`{-~]$/",
        "is_ascii_whitespace" => r"/^[ \t\n\f\r]$/",
        "is_ascii_graphic" => r"/^[!-~]$/",
        "is_ascii_control" => r"/^[\0-\x1f\x7f]$/",
        _ => return None,
    })
}
