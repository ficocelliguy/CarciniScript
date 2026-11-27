//! Type checking for `Result`: `Ok` / `Err`, their patterns, `?`, the combinators, `str::parse`
//! and `Option::ok_or`. A child module of `typeck`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs, HasName},
};

use super::{Checker, VarKind, strip_parens};
use crate::ty::{IntKind, Ty};

/// `Ok(..)` / `Err(..)` (also `Result::Ok`): 0 for `Ok`, 1 for `Err`.
pub fn result_variant(path: Option<ast::Path>) -> Option<usize> {
    let names: Vec<String> = path?
        .segments()
        .filter_map(|s| s.name_ref())
        .map(|n| n.text().to_string())
        .collect();
    match names.join("::").as_str() {
        "Ok" | "Result::Ok" | "std::result::Result::Ok" => Some(0),
        "Err" | "Result::Err" | "std::result::Result::Err" => Some(1),
        _ => None,
    }
}

/// `Ok(x)` / `Err(e)` as a call.
pub fn result_ctor_call(c: &ast::CallExpr) -> Option<usize> {
    let ast::Expr::PathExpr(p) = c.expr()? else {
        return None;
    };
    result_variant(p.path())
}

/// The error type of `str::parse::<T>()`.
pub fn parse_error_for(t: &Ty) -> &'static str {
    match t {
        Ty::Float { .. } => "ParseFloatError",
        Ty::Bool => "ParseBoolError",
        Ty::Char => "ParseCharError",
        Ty::Str => "Infallible",
        Ty::Adt(n, _) if n == "TokenStream" => "LexError",
        _ => "ParseIntError",
    }
}

/// Error types from std that implement `Display` (and `Debug`).
pub fn is_std_error(name: &str) -> bool {
    matches!(
        name,
        "ParseIntError"
            | "ParseFloatError"
            | "ParseBoolError"
            | "ParseCharError"
            | "fmt::Error"
            | "TryFromIntError"
            | "CharTryFromError"
            | "TryFromSliceError"
            | "TryFromCharError"
            | "TryReserveError"
            | "Utf8Error"
            | "FromUtf16Error"
            | "BorrowError"
            | "BorrowMutError"
            | "serde_json::Error"
    )
}

impl Checker<'_> {
    /// `Ok(x)` / `Err(e)`: the other side comes from the expected type, or stays open.
    pub(super) fn result_ctor(
        &mut self,
        c: &ast::CallExpr,
        variant: usize,
        expected: Option<&Ty>,
    ) -> Ty {
        let args: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let (t, e) = match expected.map(|t| self.shallow(t)) {
            Some(Ty::Result(t, e)) => (*t, *e),
            _ => (self.fresh(VarKind::General), self.fresh(VarKind::General)),
        };
        // `Ok::<T, E>(..)`
        let turbofish: Vec<Ty> = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.generic_arg_list())
                .map(|l| {
                    l.generic_args()
                        .filter_map(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .iter()
                .map(|t| self.lower_type(t))
                .collect(),
            _ => vec![],
        };
        if let [tt, te] = turbofish.as_slice() {
            self.unify(&t, tt);
            self.unify(&e, te);
        }
        self.arg_count(c.syntax(), "enum variant", 1, args.len());
        if let Some(a) = args.first() {
            self.check(a, if variant == 0 { &t } else { &e });
        }
        Ty::Result(Box::new(t), Box::new(e))
    }

    /// `Ok(p)` / `Err(p)` patterns.
    pub(super) fn result_pat(
        &mut self,
        p: &ast::Pat,
        tp: &ast::TupleStructPat,
        variant: usize,
        expected: &Ty,
    ) {
        let (t, e) = (self.fresh(VarKind::General), self.fresh(VarKind::General));
        let want = Ty::Result(Box::new(t.clone()), Box::new(e.clone()));
        if !self.unify(expected, &want) {
            let msg = format!(
                "mismatched types: expected `{}`, found `Result<_, _>`",
                self.describe(expected)
            );
            self.error(p.syntax(), msg);
        }
        let fields: Vec<ast::Pat> = tp.fields().collect();
        if fields.len() != 1 {
            self.error(
                p.syntax(),
                format!(
                    "this pattern has {} field{}, but the corresponding tuple variant has 1 field",
                    fields.len(),
                    if fields.len() == 1 { "" } else { "s" }
                ),
            );
        }
        if let Some(f) = fields.first() {
            self.pat(f, if variant == 0 { &t } else { &e });
        }
    }

    /// Methods on `Result<T, E>`.
    pub(super) fn result_method(
        &mut self,
        m: &ast::MethodCallExpr,
        t: Ty,
        e: Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let a0 = args.first();
        let a1 = args.get(1);
        let count = |c: &mut Self, n: usize| c.arg_count(m.syntax(), "method", n, args.len());
        let res = |t: Ty, e: Ty| Ty::Result(Box::new(t), Box::new(e));
        let opt = |t: Ty| Ty::Option(Box::new(t));
        Some(match method {
            "is_ok" | "is_err" => {
                count(self, 0);
                Ty::Bool
            }
            "unwrap" => {
                count(self, 0);
                // (`r.unwrap()` of a local holding `m.get_mut(k).ok_or(e)`: the slot.)
                if m.receiver().is_some_and(|r| self.result_slot_local(&r).is_some()) {
                    self.mark_slot_yield(&ast::Expr::MethodCallExpr(m.clone()));
                }
                t
            }
            "expect" => {
                count(self, 1);
                if m.receiver().is_some_and(|r| self.result_slot_local(&r).is_some()) {
                    self.mark_slot_yield(&ast::Expr::MethodCallExpr(m.clone()));
                }
                if let Some(a) = a0 {
                    self.check(a, &Ty::Str);
                }
                t
            }
            "unwrap_err" => {
                count(self, 0);
                e
            }
            "expect_err" => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Str);
                }
                e
            }
            "unwrap_or" => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &t);
                }
                t
            }
            "unwrap_or_default" => {
                count(self, 0);
                if !self.zonk(&t).has_default() && !matches!(self.zonk(&t), Ty::Adt(..)) {
                    let msg = format!(
                        "the trait bound `{}: Default` is not satisfied",
                        self.describe(&t)
                    );
                    self.error(m.syntax(), msg);
                }
                t
            }
            "unwrap_or_else" => {
                count(self, 1);
                self.closure_arg(a0, vec![e], t)
            }
            "ok" => {
                count(self, 0);
                opt(t)
            }
            "err" => {
                count(self, 0);
                opt(e)
            }
            "map" => {
                count(self, 1);
                let u = self.fresh(VarKind::General);
                let u = self.closure_arg(a0, vec![t], u);
                res(u, e)
            }
            "map_err" => {
                count(self, 1);
                let f = self.fresh(VarKind::General);
                let f = self.closure_arg(a0, vec![e], f);
                res(t, f)
            }
            "and_then" => {
                count(self, 1);
                let u = self.fresh(VarKind::General);
                self.closure_arg(a0, vec![t], res(u.clone(), e.clone()));
                res(u, e)
            }
            "or_else" => {
                count(self, 1);
                let f = self.fresh(VarKind::General);
                self.closure_arg(a0, vec![e], res(t.clone(), f.clone()));
                res(t, f)
            }
            // `r.or(other)` / `r.and(other)`: the other `Result`, already evaluated.
            "or" => {
                count(self, 1);
                let f = self.fresh(VarKind::General);
                if let Some(a) = a0 {
                    self.check(a, &res(t.clone(), f.clone()));
                }
                res(t, f)
            }
            "and" => {
                count(self, 1);
                let u = self.fresh(VarKind::General);
                if let Some(a) = a0 {
                    self.check(a, &res(u.clone(), e.clone()));
                }
                res(u, e)
            }
            "is_ok_and" => {
                count(self, 1);
                self.closure_arg(a0, vec![t], Ty::Bool)
            }
            "is_err_and" => {
                count(self, 1);
                self.closure_arg(a0, vec![e], Ty::Bool)
            }
            "inspect" => {
                count(self, 1);
                self.closure_arg(a0, vec![t.clone()], Ty::Unit);
                res(t, e)
            }
            "inspect_err" => {
                count(self, 1);
                self.closure_arg(a0, vec![e.clone()], Ty::Unit);
                res(t, e)
            }
            // `Result<Option<T>, E>` -> `Option<Result<T, E>>`.
            "transpose" => {
                count(self, 0);
                let x = self.fresh(VarKind::General);
                if !self.unify(&t, &Ty::Option(Box::new(x.clone()))) {
                    let msg = format!(
                        "no method named `transpose` found for `{}`",
                        self.describe(&res(t, e))
                    );
                    self.error(m.syntax(), msg);
                    return Some(Ty::Unknown);
                }
                Ty::Option(Box::new(res(x, e)))
            }
            "map_or" => {
                count(self, 2);
                let u = match a0 {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                };
                self.closure_arg(a1, vec![t], u)
            }
            "as_ref" | "as_mut" | "clone" | "cloned" | "copied" => {
                count(self, 0);
                if method == "as_mut" {
                    self.mark_res_as_mut_slot(m, &t);
                }
                res(t, e)
            }
            _ => return None,
        })
    }

    /// `Option::ok_or(e)` / `ok_or_else(|| e)`.
    pub(super) fn option_to_result(
        &mut self,
        m: &ast::MethodCallExpr,
        inner: Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let a0 = args.first();
        let count = |c: &mut Self, n: usize| c.arg_count(m.syntax(), "method", n, args.len());
        let e = match method {
            "ok_or" => {
                count(self, 1);
                match a0 {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                }
            }
            "ok_or_else" => {
                count(self, 1);
                let e = self.fresh(VarKind::General);
                self.closure_arg(a0, vec![], e)
            }
            _ => return None,
        };
        // `let r = m.get_mut(k).ok_or(e)?;` / `*m.get_mut(k).ok_or(e)? += 1` of a number, string or
        // bool: the `?` (or `unwrap`) yields the slot.
        if let Some(ast::Expr::MethodCallExpr(x)) = m.receiver().map(|r| strip_parens(&r))
            && self.slot_calls.contains(&self.range(x.syntax()))
            && let Some(outer) = super::ok_or_slot(&x)
        {
            self.mark_slot_yield(&outer);
        }
        Some(Ty::Result(Box::new(inner), Box::new(e)))
    }

    /// `outer` (a `?` or an `unwrap()`) yields a slot: read through it, or keep it in a `let`.
    pub(super) fn mark_slot_yield(&mut self, outer: &ast::Expr) {
        let r = self.range(outer.syntax());
        match outer
            .syntax()
            .parent()
            .and_then(ast::LetStmt::cast)
            .and_then(|l| l.pat())
        {
            Some(ast::Pat::IdentPat(ip)) if ip.pat().is_none() => {
                if let Some(n) = ip.name() {
                    let nr = self.range(n.syntax());
                    self.slot_bindings.insert(nr);
                }
                self.slot_values.insert(r);
                self.slot_keeps.insert(r);
            }
            _ => {
                self.slot_values.insert(r);
            }
        }
    }

    /// `s.parse::<T>()`: integers, floats, `bool` and `char`.
    pub(super) fn parse_method(
        &mut self,
        m: &ast::MethodCallExpr,
        target: Option<Ty>,
        expected: Option<&Ty>,
    ) -> Ty {
        let t = target
            .or_else(|| match expected.map(|t| self.shallow(t)) {
                Some(Ty::Result(t, _)) => Some(*t),
                _ => None,
            })
            .unwrap_or_else(|| self.fresh(VarKind::General));
        // A type with `impl FromStr`: its `Err`. Not known yet with such impls around: waits.
        let z = self.shallow(&t);
        let (err, waits) = match &z {
            Ty::Adt(n, _) if self.adts.method(n, "from_str").is_some() => {
                (self.parse_err_of(n), false)
            }
            Ty::Var(_) if self.has_from_str() => (self.fresh(VarKind::General), true),
            // `s.parse::<T>()` with `T: FromStr`: by `T`'s class; its `Err` is unchecked.
            Ty::Param(p) if self.param_has(p, "FromStr") => {
                let p = p.clone();
                self.param_class(&p, m.syntax(), Some("`parse::<T>()`"));
                (Ty::Unknown, false)
            }
            _ => (Ty::Extern(parse_error_for(&z).into()), false),
        };
        let pending = waits.then(|| err.clone());
        self.pending_parses
            .push((t.clone(), m.syntax().clone(), pending));
        Ty::Result(Box::new(t), Box::new(err))
    }

    /// The `Err` of `impl FromStr for T`: the error type `from_str` returns.
    fn parse_err_of(&self, adt: &str) -> Ty {
        match self.adts.method(adt, "from_str").map(|m| m.ret.clone()) {
            Some(Ty::Result(_, e)) => *e,
            _ => Ty::Unknown,
        }
    }

    /// Whether any type in the program has `impl FromStr` (a `from_str` function).
    fn has_from_str(&self) -> bool {
        self.adts
            .methods
            .values()
            .any(|ms| ms.get("from_str").is_some_and(|m| m.receiver.is_none()))
    }

    /// `parse()` calls must end up with a type that can be parsed.
    pub(super) fn resolve_parses(&mut self) {
        for (t, node, err) in std::mem::take(&mut self.pending_parses) {
            if let Some(err) = err {
                let z = self.zonk(&t);
                let want = match &z {
                    Ty::Adt(n, _) if self.adts.method(n, "from_str").is_some() => {
                        self.parse_err_of(n)
                    }
                    _ => Ty::Extern(parse_error_for(&z).into()),
                };
                self.unify(&err, &want);
            }
            match self.zonk(&t) {
                // `impl FromStr for T`.
                Ty::Adt(n, _) if self.adts.method(&n, "from_str").is_some() => {}
                Ty::Int(k) if k.is_big() => {}
                Ty::Param(p) if self.param_has(&p, "FromStr") => {}
                // (`parse::<String>()` can't fail; it's how a proc macro makes a `TokenStream`.)
                Ty::Int(_) | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str | Ty::Unknown => {}
                // `format!(..).parse::<TokenStream>()` in a proc-macro crate.
                Ty::Adt(n, _)
                    if n == "TokenStream" && self.adts.defs.contains_key("TokenStream") => {}
                Ty::Var(_) => {
                    if self.var_kind(&t) == Some(VarKind::Int) {
                        self.unify(&t, &Ty::Int(IntKind::I32));
                    } else if self.var_kind(&t) == Some(VarKind::Float) {
                        self.unify(&t, &Ty::Float { f32: false });
                    } else {
                        self.error(&node, "type annotations needed: say what to parse into, e.g. `parse::<i32>()`");
                    }
                }
                other => {
                    let msg = format!(
                        "the trait bound `{}: FromStr` is not satisfied",
                        other.describe()
                    );
                    self.error(&node, msg);
                }
            }
        }
    }
}
