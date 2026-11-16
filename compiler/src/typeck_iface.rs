//! The NS bindings' interfaces as Rust structs a script can name and build: `RunOptions {
//! threads: Some(4.0), ..Default::default() }`, `Server { money_available: m, ..base }`,
//! `RunOptions::default()`, and the subtyping TS has between them: a value of an interface is
//! accepted where one it extends is expected (`Player` where a `Person` is, `ServerWithIsOnline`
//! where a `Server` is). A child module of `typeck`.

use ra_ap_syntax::{AstNode, ast};

use super::Checker;
use crate::bindings::bare;
use crate::ty::Ty;

impl Checker<'_> {
    /// Whether `name` is an interface a script can build: declared with fields only (no
    /// methods, unlike the NS namespaces).
    fn iface_buildable(&self, name: &str) -> bool {
        self.bindings.is_interface(name)
            && self
                .bindings
                .types
                .get(name)
                .is_some_and(|t| t.methods.is_empty() && !t.fields.is_empty())
    }

    /// Whether `Default::default()` makes a `name`: an interface whose fields are all optional
    /// (an empty JS object).
    pub(super) fn iface_has_default(&self, name: &str) -> bool {
        self.iface_buildable(name)
            && self.bindings.types[name]
                .fields
                .values()
                .all(|f| matches!(f.ty, Ty::Option(_)))
    }

    /// A value of type `t` where `expected` is wanted, when that's an interface `t` extends.
    pub(super) fn iface_coerces(&self, t: &Ty, expected: &Ty) -> bool {
        match (self.shallow(t), self.shallow(expected)) {
            (Ty::Extern(sub), Ty::Extern(sup)) => sub != sup && self.bindings.extends(&sub, &sup),
            _ => false,
        }
    }

    /// `RunOptions { threads: Some(t), ..Default::default() }`: None when the path isn't an
    /// interface of the bindings.
    pub(super) fn iface_record(&mut self, r: &ast::RecordExpr, segs: &[String]) -> Option<Ty> {
        let [name] = segs else {
            return None;
        };
        if !self.bindings.is_interface(name) || self.adts.defs.contains_key(name) {
            return None;
        }
        let ty = Ty::Extern(name.clone());
        let list = r.record_expr_field_list();
        let fields: Vec<ast::RecordExprField> = list.iter().flat_map(|l| l.fields()).collect();
        if !self.iface_buildable(name) {
            let msg = format!(
                "`{name}` is an NS namespace, not a struct: it can't be built with `{name} {{ .. }}`"
            );
            self.error(r.syntax(), msg);
            for f in &fields {
                if let Some(e) = f.expr() {
                    self.infer(&e, None);
                }
            }
            return Some(ty);
        }
        let b = self.bindings.clone();
        let et = &b.types[name.as_str()];
        let mut seen: Vec<String> = Vec::new();
        for f in &fields {
            let Some(fname) = f.field_name() else {
                continue;
            };
            let key = bare(fname.text());
            let Some(field) = et.fields.get(&key) else {
                let hint = crate::bindings::suggest(&key, et.fields.keys())
                    .map(|s| format!("; did you mean `{s}`?"))
                    .unwrap_or_default();
                self.error(
                    fname.syntax(),
                    format!("struct `{name}` has no field named `{key}`{hint}"),
                );
                if let Some(e) = f.expr() {
                    self.infer(&e, None);
                }
                continue;
            };
            if seen.contains(&key) {
                self.error(
                    fname.syntax(),
                    format!("field `{key}` specified more than once"),
                );
            }
            seen.push(key.clone());
            match f.expr() {
                Some(e) => {
                    self.check(&e, &field.ty);
                }
                None => match self.lookup(fname.text().to_string().as_str()).cloned() {
                    Some(local) => {
                        if !self.unify(&local.ty, &field.ty)
                            && !self.iface_coerces(&local.ty, &field.ty)
                        {
                            let msg = format!(
                                "mismatched types: expected `{}`, found `{}`",
                                self.describe(&field.ty),
                                self.describe(&local.ty)
                            );
                            self.error(f.syntax(), msg);
                        }
                    }
                    None => self.error(
                        f.syntax(),
                        format!("cannot find value `{}` in this scope", fname.text()),
                    ),
                },
            }
        }
        match list.as_ref().and_then(|l| l.spread()) {
            Some(base) => {
                self.check(&base, &ty);
            }
            None => {
                let mut missing: Vec<String> = et
                    .fields
                    .keys()
                    .filter(|k| !seen.contains(k))
                    .map(|k| format!("`{k}`"))
                    .collect();
                missing.sort();
                if !missing.is_empty() {
                    let s = if missing.len() == 1 { "" } else { "s" };
                    let msg = format!(
                        "missing field{s} {} in initializer of `{name}`",
                        super::adt_check::join_and(&missing)
                    );
                    self.error(r.syntax(), msg);
                }
            }
        }
        Some(ty)
    }

    /// `RunOptions::default()`; None for other calls.
    pub(super) fn iface_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let [name, "default"] = names else {
            return None;
        };
        if !self.bindings.is_interface(name) || self.adts.defs.contains_key(*name) {
            return None;
        }
        self.arg_count(c.syntax(), "function", 0, args.len());
        if !self.iface_has_default(name) {
            let msg = format!(
                "the trait bound `{name}: Default` is not satisfied (only NS interfaces whose fields are all optional have a default)"
            );
            self.error(c.syntax(), msg);
        }
        Some(Ty::Extern(name.to_string()))
    }
}
