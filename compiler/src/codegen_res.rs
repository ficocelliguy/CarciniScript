//! Codegen for `Result`: `κσ.Ok(v)` / `κσ.Err(e)` (a `$tag` of 0 / 1 and the value at `[0]`),
//! `?`, patterns, the combinators, `str::parse` and `Option::ok_or`. A child module of `codegen`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

use super::{Codegen, Frag, PatCode, debug_fn, default_value, js_string};
use crate::ty::{IntKind, Ty};

/// A formatter function for `{:?}` of `ty`.
fn fmt_of(ty: &Ty) -> String {
    debug_fn(ty).unwrap_or_else(|| "String".into())
}

impl Codegen<'_> {
    /// `Ok(x)` / `Err(e)`.
    pub(super) fn emit_result_ctor(&mut self, c: &ast::CallExpr, variant: usize) -> (Frag, Ty) {
        let args = self.emit_args(c.arg_list(), &[]);
        let arg = args
            .into_iter()
            .next()
            .unwrap_or_else(|| Frag::text("undefined"));
        let name = if variant == 0 { "Ok" } else { "Err" };
        let mut f = Frag::mapped(self.off(c.syntax()), format!("κσ.{name}("));
        f.append(arg).push(")");
        let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        (f, ty)
    }

    /// `Ok(p)` / `Err(p)`.
    pub(super) fn lower_result_pat(
        &mut self,
        tp: &ast::TupleStructPat,
        variant: usize,
        subject: &str,
        out: &mut PatCode,
    ) {
        out.conds
            .push(Frag::text(format!("{subject}.$tag === {variant}")));
        if let Some(f) = tp.fields().next() {
            self.lower_pat(&f, &format!("{subject}[0]"), out);
        }
    }

    /// Methods on `Result<T, E>`.
    pub(super) fn emit_result_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: &Frag,
        t: &Ty,
        e: &Ty,
        method: &str,
    ) -> Option<(Frag, Ty)> {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        // rustc's `#[track_caller]` location for a method call is the method name.
        let loc = js_string(&self.location_at(name_off));
        let result_ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let args: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let arg = |cg: &mut Self, i: usize| -> Frag {
            match args.get(i) {
                Some(a) => {
                    let (f, ty) = cg.emit_expr(a, None);
                    cg.by_value(a, f, &ty)
                }
                None => Frag::text("undefined"),
            }
        };
        let helper = |name: &str, rest: Vec<Frag>| {
            let mut all = vec![rf.clone()];
            all.extend(rest);
            let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
            f.append(Frag::join(all, ", ")).push(")");
            f
        };
        let f = match method {
            "is_ok" => rf.clone().wrap("(", ".$tag === 0)"),
            "is_err" => rf.clone().wrap("(", ".$tag === 1)"),
            "unwrap" => helper("resUnwrap", vec![Frag::text(fmt_of(e)), Frag::text(loc)]),
            "expect" => {
                let msg = arg(self, 0);
                helper(
                    "resExpect",
                    vec![msg, Frag::text(fmt_of(e)), Frag::text(loc)],
                )
            }
            "unwrap_err" => helper("resUnwrapErr", vec![Frag::text(fmt_of(t)), Frag::text(loc)]),
            "expect_err" => {
                let msg = arg(self, 0);
                helper(
                    "resExpectErr",
                    vec![msg, Frag::text(fmt_of(t)), Frag::text(loc)],
                )
            }
            "unwrap_or" => {
                let d = arg(self, 0);
                helper("resOr", vec![d])
            }
            "unwrap_or_default" => {
                let d = default_value(t).unwrap_or_else(|| "undefined".into());
                helper("resOr", vec![Frag::text(d)])
            }
            "unwrap_or_else" => {
                let f = arg(self, 0);
                helper("resOrElse", vec![f])
            }
            "ok" => helper("resOk", vec![]),
            "err" => helper("resErr", vec![]),
            "map" => {
                let f = arg(self, 0);
                helper("resMap", vec![f])
            }
            "map_err" => {
                let f = arg(self, 0);
                helper("resMapErr", vec![f])
            }
            "and_then" => {
                let f = arg(self, 0);
                helper("resAndThen", vec![f])
            }
            "or_else" => {
                let f = arg(self, 0);
                helper("resOrElseRes", vec![f])
            }
            "is_ok_and" => {
                let f = arg(self, 0);
                helper("resIsOkAnd", vec![f])
            }
            "is_err_and" => {
                let f = arg(self, 0);
                helper("resIsErrAnd", vec![f])
            }
            "inspect" => {
                let f = arg(self, 0);
                helper("resInspect", vec![f])
            }
            "inspect_err" => {
                let f = arg(self, 0);
                helper("resInspectErr", vec![f])
            }
            "transpose" => helper("resTranspose", vec![]),
            "map_or" => {
                let d = arg(self, 0);
                let f = arg(self, 1);
                helper("resMapOr", vec![d, f])
            }
            "as_ref" | "as_mut" => rf.clone(),
            "clone" | "cloned" | "copied" => {
                let mut f = Frag::mapped(name_off, "κσ.$clone(");
                f.append(rf.clone()).push(")");
                f
            }
            _ => return None,
        };
        Some((f, result_ty))
    }

    /// `opt.ok_or(e)` / `opt.ok_or_else(|| e)`.
    pub(super) fn emit_ok_or(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: &Frag,
        method: &str,
    ) -> (Frag, Ty) {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        let arg = m
            .arg_list()
            .and_then(|a| a.args().next())
            .map(|a| self.emit_expr(&a, None).0)
            .unwrap_or_else(|| Frag::text("undefined"));
        let name = if method == "ok_or" {
            "okOr"
        } else {
            "okOrElse"
        };
        let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
        f.append(rf.clone()).push(", ").append(arg).push(")");
        (f, self.ty_of(m.syntax()).unwrap_or(Ty::Unknown))
    }

    /// `s.parse::<T>()`.
    pub(super) fn emit_parse(&mut self, m: &ast::MethodCallExpr, rf: &Frag) -> (Frag, Ty) {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let target = match &ty {
            Ty::Result(t, _) => (**t).clone(),
            _ => Ty::Unknown,
        };
        let (name, extra) = match &target {
            Ty::Float { .. } => ("parseFloat", String::new()),
            Ty::Bool => ("parseBool", String::new()),
            Ty::Char => ("parseChar", String::new()),
            Ty::Int(k @ (IntKind::I128 | IntKind::U128)) => {
                ("parseInt128", format!(", {}", k.is_signed()))
            }
            Ty::Int(k) => {
                let (min, max) = k.min_max();
                ("parseInt", format!(", {min}, {max}"))
            }
            _ => ("parseInt", ", -2147483648, 2147483647".to_string()),
        };
        let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
        f.append(rf.clone()).push(format!("{extra})"));
        (f, ty)
    }
}
