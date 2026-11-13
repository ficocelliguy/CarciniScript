//! Type checking for `Result`: `Ok` / `Err`, their patterns, `?`, the combinators, `str::parse`
//! and `Option::ok_or`. A child module of `typeck`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericArgs},
};

use super::{Checker, VarKind};
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
        _ => "ParseIntError",
    }
}

/// Error types from std that implement `Display` (and `Debug`).
pub fn is_std_error(name: &str) -> bool {
    matches!(
        name,
        "ParseIntError" | "ParseFloatError" | "ParseBoolError" | "ParseCharError" | "fmt::Error"
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
        let name = if variant == 0 { "Ok" } else { "Err" };
        if fields.len() != 1 {
            self.error(
                p.syntax(),
                format!(
                    "`{name}` has 1 field, but this pattern has {}",
                    fields.len()
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
                t
            }
            "expect" => {
                count(self, 1);
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
                if !self.zonk(&t).has_default() && !matches!(self.zonk(&t), Ty::Adt(_)) {
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
            "is_ok_and" => {
                count(self, 1);
                self.closure_arg(a0, vec![t], Ty::Bool)
            }
            "is_err_and" => {
                count(self, 1);
                self.closure_arg(a0, vec![e], Ty::Bool)
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
        Some(Ty::Result(Box::new(inner), Box::new(e)))
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
        self.pending_parses.push((t.clone(), m.syntax().clone()));
        // The error type is fixed once `T` is known; an integer is the common case.
        let err = Ty::Extern(parse_error_for(&self.zonk(&t)).into());
        Ty::Result(Box::new(t), Box::new(err))
    }

    /// `parse()` calls must end up with a type that can be parsed.
    pub(super) fn resolve_parses(&mut self) {
        for (t, node) in std::mem::take(&mut self.pending_parses) {
            match self.zonk(&t) {
                Ty::Int(k) if k.is_wide_unsupported() => {}
                Ty::Int(_) | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Unknown => {}
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
