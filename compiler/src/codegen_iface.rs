//! The NS bindings' interfaces built in Rust: `RunOptions { threads: Some(t), ..base }` is a
//! plain JS object with the interface's JS names (`κσ.$iface({threads: t}, base)`, which leaves
//! out `None` fields), `RunOptions::default()` an empty one. A child module of `codegen`.

use ra_ap_syntax::{AstNode, ast};

use super::{Codegen, Frag, js_string};
use crate::bindings::bare;
use crate::ty::Ty;

impl Codegen<'_> {
    /// `RunOptions { .. }`; None when the path isn't an interface of the bindings.
    pub(super) fn emit_iface_record(
        &mut self,
        r: &ast::RecordExpr,
        segs: &[String],
    ) -> Option<(Frag, Ty)> {
        let [name] = segs else {
            return None;
        };
        let b = crate::bindings::current();
        if !b.is_interface(name) || self.types.adts.defs.contains_key(name) {
            return None;
        }
        let et = b.types.get(name.as_str())?;
        let list = r.record_expr_field_list();
        let mut f = Frag::mapped(self.off(r.syntax()), "κσ.$iface({");
        let mut first = true;
        for field in list.iter().flat_map(|l| l.fields()) {
            let Some(fname) = field.field_name() else {
                continue;
            };
            let Some(decl) = et.fields.get(&bare(fname.text())) else {
                continue;
            };
            let value = match field.expr() {
                Some(e) => {
                    let (v, t) = self.emit_expr(&e, Some(&decl.ty));
                    self.by_value(&e, v, &t)
                }
                None => {
                    let js = self
                        .lookup(fname.text().to_string().as_str())
                        .map(|l| l.js.clone())
                        .unwrap_or_else(|| "undefined".into());
                    Frag::mapped(self.off(fname.syntax()), js)
                }
            };
            if !first {
                f.push(", ");
            }
            first = false;
            f.push(format!("{}: ", js_string(&decl.js))).append(value);
        }
        f.push("}");
        if let Some(base) = list.and_then(|l| l.spread()) {
            let (v, _) = self.emit_expr(&base, None);
            f.push(", ").append(v);
        }
        f.push(")");
        Some((f, Ty::Extern(name.clone())))
    }

    /// `RunOptions::default()`: an empty object; None for other calls.
    pub(super) fn emit_iface_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[&str],
    ) -> Option<(Frag, Ty)> {
        let [name, "default"] = segs else {
            return None;
        };
        let b = crate::bindings::current();
        if !b.is_interface(name) || self.types.adts.defs.contains_key(*name) {
            return None;
        }
        Some((
            Frag::mapped(self.off(c.syntax()), "{}"),
            Ty::Extern(name.to_string()),
        ))
    }
}

/// A value read from JS (an NS field or result) of declared type `ty`: a `HashMap` is a JS
/// `Map` or object there, made the runtime's `HashMap`.
pub(super) fn from_js(f: Frag, ty: &Ty) -> Frag {
    if is_js_map(ty) {
        f.wrap("κσ.$mapOfJs(", ")")
    } else {
        f
    }
}

/// A `HashMap` (or an `Option` of one) declared by the bindings.
pub(super) fn is_js_map(ty: &Ty) -> bool {
    match ty {
        Ty::Map(..) => true,
        Ty::Option(inner) => matches!(**inner, Ty::Map(..)),
        _ => false,
    }
}
