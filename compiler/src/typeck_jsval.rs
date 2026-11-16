//! `JsValue` beyond its methods (which the `js` crate's bindings declare): its functions as
//! values (`.and_then(JsValue::as_string)`), `JsValue::NULL` / `UNDEFINED`, and the
//! `serde_wasm_bindgen` bridge, `from_value::<T>(v)` and `to_value(&x)`, which convert by the
//! static type as serde does. A child module of `typeck`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs},
};

use super::{Checker, VarKind};
use crate::ty::Ty;

/// The type of a `JsValue`.
pub fn js_value() -> Ty {
    Ty::Extern("JsValue".into())
}

/// The error `serde_wasm_bindgen`'s functions fail with: serde's message, as `js::JsonError`.
pub fn swb_error() -> Ty {
    super::js::json_error()
}

impl Checker<'_> {
    /// `JsValue::as_string` as a value, `JsValue::NULL`; None for other paths.
    pub(super) fn jsvalue_path(&mut self, segs: &[&str]) -> Option<Ty> {
        let [.., "JsValue", item] = segs else {
            return None;
        };
        if matches!(*item, "NULL" | "UNDEFINED") {
            return Some(js_value());
        }
        let b = self.bindings.clone();
        let sig = b.types.get("JsValue")?.methods.get(*item)?;
        let mut params: Vec<Ty> = Vec::new();
        if !sig.is_static {
            params.push(js_value());
        }
        params.extend(sig.params.iter().map(|p| p.ty.clone()));
        Some(Ty::Closure {
            params,
            ret: Box::new(sig.ret.clone()),
        })
    }

    /// `serde_wasm_bindgen::from_value(v)` / `to_value(&x)` (also through `use`); None for
    /// other calls.
    pub(super) fn swb_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        let f = match names {
            ["serde_wasm_bindgen", f] => *f,
            [f @ ("from_value" | "to_value")] if self.swb_imported(f) => *f,
            _ => return None,
        };
        if !matches!(f, "from_value" | "to_value") {
            return None;
        }
        self.arg_count(c.syntax(), "function", 1, args.len());
        if f == "to_value" {
            if let Some(a) = args.first() {
                let t = self.infer(a, None);
                self.pending_json.push((t, a.syntax().clone(), false));
            }
            return Some(Ty::Result(Box::new(js_value()), Box::new(swb_error())));
        }
        if let Some(a) = args.first() {
            self.check(a, &js_value());
        }
        let turbofish = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.generic_arg_list())
                .and_then(|l| l.generic_args().next())
                .and_then(|g| match g {
                    ast::GenericArg::TypeArg(ta) => ta.ty(),
                    _ => None,
                }),
            _ => None,
        };
        let t = match turbofish {
            Some(t) => self.lower_type(&t),
            None => match expected.map(|e| self.shallow(e)) {
                Some(Ty::Result(t, _)) => *t,
                _ => self.fresh(VarKind::General),
            },
        };
        self.pending_json
            .push((t.clone(), c.syntax().clone(), true));
        Some(Ty::Result(Box::new(t), Box::new(swb_error())))
    }

    /// Whether `use serde_wasm_bindgen::{from_value, ..}` brought in `f`.
    fn swb_imported(&self, f: &str) -> bool {
        swb_imports(self.src).iter().any(|n| n == f)
    }
}

/// The `serde_wasm_bindgen` functions a file imports by name.
pub fn swb_imports(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find("use serde_wasm_bindgen::") {
        rest = &rest[i + "use serde_wasm_bindgen::".len()..];
        let end = rest.find(';').unwrap_or(rest.len());
        for f in ["from_value", "to_value"] {
            if rest[..end].contains(f) {
                out.push(f.to_string());
            }
        }
    }
    out
}
