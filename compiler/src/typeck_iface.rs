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
        // (`JsValue::default()` is `undefined`.)
        if name == "JsValue" {
            return true;
        }
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
                Some(e) => match &field.ty {
                    // (A union-typed field takes any variant's payload: `desired_sell_price: 5.0`.)
                    Ty::Extern(n) if self.bindings.is_enum(n) => {
                        let n = n.clone();
                        self.check_enum_arg(&e, &n);
                    }
                    _ => {
                        self.check(&e, &field.ty);
                    }
                },
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

// ------------------------------------------------------------ bindings enums (TS unions)

impl Checker<'_> {
    /// A bindings enum named by a two-segment path (`ScriptArg::Str`), when the file doesn't
    /// declare a type of that name itself.
    fn extern_enum<'b>(&self, b: &'b std::rc::Rc<crate::bindings::Bindings>, segs: &[String]) -> Option<(&'b crate::bindings::ExternType, String, String)> {
        let [e, v] = segs else { return None };
        if self.adts.defs.contains_key(e) {
            return None;
        }
        let et = b.enum_of(e)?;
        Some((et, e.clone(), v.clone()))
    }

    /// `ScriptArg::Str(s)` as a pattern: the subject is the enum, the field the variant's
    /// payload. False when the pattern names no bindings enum.
    pub(super) fn extern_enum_pat(&mut self, p: &ast::Pat, expected: &Ty) -> bool {
        let ast::Pat::TupleStructPat(tp) = p else {
            return false;
        };
        let b = self.bindings.clone();
        let Some((et, e, v)) = self.extern_enum(&b, &super::adt_check::path_segs(tp.path())) else {
            return false;
        };
        let Some((_, variant)) = et.variant(&v) else {
            let hint = crate::bindings::suggest(&v, et.variants.iter().map(|x| &x.name))
                .map(|s| format!("; did you mean `{s}`?"))
                .unwrap_or_default();
            self.error(
                p.syntax(),
                format!("no variant named `{v}` found for enum `{e}`{hint}"),
            );
            for f in tp.fields() {
                self.pat(&f, &Ty::Unknown);
            }
            return true;
        };
        if !self.unify(expected, &Ty::Extern(e.clone())) {
            let msg = format!(
                "mismatched types: expected `{}`, found `{e}`",
                self.describe(expected)
            );
            self.error(p.syntax(), msg);
        }
        let fields: Vec<ast::Pat> = tp.fields().collect();
        match fields.as_slice() {
            [f] if !matches!(f, ast::Pat::RestPat(_)) => {
                let payload = variant.payload.clone();
                self.pat(f, &payload);
            }
            [] | [ast::Pat::RestPat(_)] => {}
            _ => {
                let msg = format!(
                    "this pattern has {} fields, but the corresponding tuple variant `{e}::{v}` has 1 field",
                    fields.len()
                );
                self.error(p.syntax(), msg);
                for f in &fields {
                    self.pat(f, &Ty::Unknown);
                }
            }
        }
        true
    }

    /// Exhaustiveness: a bindings enum's variant pattern as a constructor over its payload.
    pub(super) fn extern_enum_abstract(&self, p: &ast::Pat) -> Option<Vec<super::Ap>> {
        let ast::Pat::TupleStructPat(tp) = p else {
            return None;
        };
        let b = self.bindings.clone();
        let (et, _, v) = self.extern_enum(&b, &super::adt_check::path_segs(tp.path()))?;
        let (idx, _) = et.variant(&v)?;
        let sub = match tp.fields().next() {
            Some(f) if !matches!(f, ast::Pat::RestPat(_)) => self.abstract_pat(&f),
            _ => vec![super::Ap::Wild],
        };
        Some(sub.into_iter().map(|a| super::Ap::Ctor(idx, vec![a])).collect())
    }

    /// `ScriptArg::Str("x")` (the payload is the value), `ScriptArg::try_from(v)` (a
    /// `Result<ScriptArg, JsValue>`: `Err` when the JS value fits no variant).
    pub(super) fn extern_enum_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[String],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let b = self.bindings.clone();
        let (et, e, v) = self.extern_enum(&b, segs)?;
        if v == "try_from" {
            self.arg_count(c.syntax(), "function", 1, args.len());
            if let Some(a) = args.first() {
                self.infer(a, None);
            }
            return Some(Ty::Result(
                Box::new(Ty::Extern(e)),
                Box::new(Ty::Extern("JsValue".into())),
            ));
        }
        // `FlagValue::from("home")`: any variant's payload, as an NS argument takes it.
        if v == "from" {
            self.arg_count(c.syntax(), "function", 1, args.len());
            if let Some(a) = args.first() {
                self.check_enum_arg(a, &e);
            }
            return Some(Ty::Extern(e));
        }
        let Some((_, variant)) = et.variant(&v) else {
            let hint = crate::bindings::suggest(&v, et.variants.iter().map(|x| &x.name))
                .map(|s| format!("; did you mean `{s}`?"))
                .unwrap_or_default();
            self.error(
                c.syntax(),
                format!("no variant or associated item named `{v}` found for enum `{e}`{hint}"),
            );
            for a in args {
                self.infer(a, None);
            }
            return Some(Ty::Unknown);
        };
        self.arg_count(c.syntax(), "function", 1, args.len());
        if let Some(a) = args.first() {
            let payload = variant.payload.clone();
            self.check_payload(a, &payload);
        }
        Some(Ty::Extern(e))
    }

    /// An argument for a variant's payload: as `check`, with an integer literal for an `f64`
    /// (every NS number is `f64`).
    fn check_payload(&mut self, a: &ast::Expr, payload: &Ty) {
        if matches!(payload, Ty::Float { .. }) && super::is_int_literal(a) {
            self.record(a.syntax(), payload.clone());
        } else {
            self.check(a, payload);
        }
    }

    /// An argument (or field value) where a bindings enum is expected: the enum itself, or any
    /// variant's payload directly (`ns.run("x.rs", 4, "n00dles")`: the representation is the
    /// payload, so nothing converts). Reports a mismatch otherwise.
    pub(super) fn check_enum_arg(&mut self, a: &ast::Expr, name: &str) {
        let b = self.bindings.clone();
        let Some(et) = b.enum_of(name) else {
            self.check(a, &Ty::Extern(name.to_string()));
            return;
        };
        let has_float = et.variants.iter().any(|v| matches!(v.payload, Ty::Float { .. }));
        if has_float && super::is_int_literal(a) {
            self.record(a.syntax(), Ty::Float { f32: false });
            return;
        }
        let t = self.infer(a, None);
        let found = self.shallow(&t);
        if matches!(&found, Ty::Extern(n) if n == name || n == "JsValue") || matches!(found, Ty::Unknown | Ty::Never) {
            return;
        }
        // (Deliberate extension, as for `f64` parameters: an integer that is a JS number, `ns.exec(..,
        // threads, batch_id)`, is a number to the game, for an `f64` payload and for an `i32` one
        // (`RunThreadOrOptions::Num(i32)` takes a `usize` thread count as it is).)
        let has_int = et.variants.iter().any(|v| matches!(v.payload, Ty::Int(k) if !k.is_big()));
        if (has_float || has_int) && matches!(found, Ty::Int(k) if !k.is_big()) {
            return;
        }
        // (An unsettled literal: a number for an `f64` payload, else the first payload it fits.)
        if let Ty::Var(_) = found {
            let kind = self.var_kind(&t);
            if kind == Some(super::VarKind::Int) || kind == Some(super::VarKind::Float) {
                if has_float {
                    self.unify(&t, &Ty::Float { f32: false });
                    return;
                }
            } else {
                return;
            }
        }
        for v in &et.variants {
            if self.could_be(&found, &v.payload) || self.iface_coerces(&found, &v.payload) {
                self.unify(&t, &v.payload);
                return;
            }
        }
        let shown: Vec<String> = et.variants.iter().map(|v| format!("`{}`", v.shown)).collect();
        let msg = format!(
            "mismatched types: expected `{name}` (a {}), found `{}`",
            super::adt_check::join_or(&shown),
            self.describe(&found)
        );
        self.error(a.syntax(), msg);
    }
}
