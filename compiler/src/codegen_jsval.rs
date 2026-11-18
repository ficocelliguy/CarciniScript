//! `JsValue` beyond its methods: its functions as values (`JsValue::as_string` is the runtime's
//! `κσ.$jsv.$asString`), `JsValue::NULL` / `UNDEFINED`, and `serde_wasm_bindgen::from_value` /
//! `to_value`, which go through JSON with a schema of the static type (as `JSON::parse` does).
//! A child module of `codegen`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

use super::{Codegen, Frag};
use crate::ty::Ty;

impl Codegen<'_> {
    /// `JsValue::as_string` as a value, `JsValue::NULL`; None for other paths.
    pub(super) fn emit_jsvalue_path(&mut self, segs: &[&str]) -> Option<(Frag, Ty)> {
        let [.., owner, item] = segs else {
            return None;
        };
        let b = crate::bindings::current();
        if *owner != "JsValue" && !b.is_enum(owner) {
            return None;
        }
        match *item {
            "NULL" if *owner == "JsValue" => {
                return Some((Frag::text("null"), Ty::Extern("JsValue".into())));
            }
            "UNDEFINED" if *owner == "JsValue" => {
                return Some((Frag::text("undefined"), Ty::Extern("JsValue".into())));
            }
            _ => {}
        }
        let sig = b.types.get(*owner)?.methods.get(*item)?;
        let js = if sig.is_static {
            sig.js.clone()
        } else {
            format!("κσ.$jsv.${}", sig.js)
        };
        Some((Frag::text(js), Ty::Fn))
    }

    /// `serde_wasm_bindgen::from_value(v)` / `to_value(&x)`; None for other calls.
    pub(super) fn emit_swb_call(&mut self, c: &ast::CallExpr, segs: &[&str]) -> Option<(Frag, Ty)> {
        let f = match segs {
            ["serde_wasm_bindgen", f @ ("from_value" | "to_value")] => *f,
            [f @ ("from_value" | "to_value")]
                if crate::typeck::swb_imports(self.src).iter().any(|n| n == f) =>
            {
                *f
            }
            _ => return None,
        };
        let ty = self.raw_ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        let a = c.arg_list()?.args().next()?;
        let start = self.off(c.syntax());
        let (schema_ty, head) = if f == "from_value" {
            let target = match &ty {
                Ty::Result(t, _) => (**t).clone(),
                _ => Ty::Unknown,
            };
            (target, "κσ.$swbFrom(")
        } else {
            let t = self.raw_ty_of(a.syntax()).unwrap_or(Ty::Unknown);
            (t, "κσ.$swbTo(")
        };
        let (v, _) = self.emit_expr(&a, None);
        let schema = self.json_schema_expr(&schema_ty);
        let mut out = Frag::mapped(start, head);
        out.append(v).push(format!(", {schema})"));
        Some((out, ty))
    }
}
