//! Codegen for structs, enums and `impl` blocks. A child module of `codegen`.
//!
//! A struct becomes a JS class whose constructor takes the fields in declaration order; an enum
//! becomes a class with a `$tag` (the variant's index) plus static factories (`Shape.Circle(r)`)
//! and shared instances for unit variants (`Shape.Empty`). Named fields are properties, tuple
//! fields are indices (`this[0]`). Methods from `impl` blocks are class methods (associated
//! functions are `static`), and derives generate `$clone`, `$eq`, `$cmp`, `$dbg` and `default`,
//! which the runtime's `κσ.$clone` / `$eq` / `$cmp` / `$dbg` dispatch to.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasGenericParams, HasName},
};

use super::{
    Codegen, Dest, Frag, Local, PatCode, default_value, escape_ident, is_pure, member, strip_refs,
};
use crate::adt::{AdtDef, Resolved, Shape, Variant};
use crate::bindings;
use crate::ty::Ty;

/// A property name for a field or method: Rust names are valid JS property names, except ones
/// Bitburner charges RAM for and the few a class reserves.
pub(super) fn prop(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    if bindings::is_ram_name(name) || matches!(name, "constructor" | "prototype" | "__proto__") {
        format!("{name}$")
    } else {
        name.to_string()
    }
}

/// `.x` for a named field, `[0]` for a tuple field.
fn access(v: &Variant, i: usize) -> String {
    match v.shape {
        Shape::Tuple => format!("[{i}]"),
        _ => member(&prop(&v.fields[i].js)),
    }
}

/// The segment names of a path (`Shape::Circle` → `["Shape", "Circle"]`).
pub(super) fn path_segments(path: Option<ast::Path>) -> Vec<String> {
    path.map(|p| {
        p.segments()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect()
    })
    .unwrap_or_default()
}

fn clone_of(ty: &Ty, e: &str) -> String {
    if ty.is_js_primitive() || matches!(ty, Ty::Unit) {
        e.to_string()
    } else {
        format!("κσ.$clone({e})")
    }
}

fn eq_of(ty: &Ty, a: &str, b: &str) -> String {
    if ty.is_js_primitive() || matches!(ty, Ty::Unit) {
        format!("{a} === {b}")
    } else {
        format!("κσ.$eq({a}, {b})")
    }
}

fn dbg_of(ty: &Ty, e: &str, params: &HashMap<String, String>) -> String {
    match super::debug_fn_in(ty, params) {
        Some(f) => format!("{f}({e})"),
        None => format!("String({e})"),
    }
}

/// The `self` type of an extension class's methods (`String` for `$ext$String`).
pub(super) fn ext_self_ty(key: &str) -> Ty {
    match key {
        "String" => Ty::Str,
        "Vec" => Ty::Vec(Box::new(Ty::Unknown)),
        "VecDeque" => Ty::Coll(
            crate::ty::Coll::VecDeque,
            Box::new(Ty::Vec(Box::new(Ty::Unknown))),
        ),
        "Range" => Ty::Range(Box::new(Ty::Unknown), false),
        "RangeInclusive" => Ty::Range(Box::new(Ty::Unknown), true),
        "Option" => Ty::Option(Box::new(Ty::Unknown)),
        "bool" => Ty::Bool,
        "char" => Ty::Char,
        k => Ty::from_name(k),
    }
}

/// A `&mut self` method of a trait implemented for a string, number, bool, char or `Option`: JS
/// can't change those through `this`, so `this` is a cell (`this[0]` is `self`).
pub(super) fn cell_self(sig: &crate::adt::Method, self_ty: &Ty) -> bool {
    sig.receiver == Some(crate::adt::Receiver::MutRef) && crate::typeck::ptr_cellable(self_ty)
}

impl Codegen<'_> {
    fn adt(&self, name: &str) -> Option<AdtDef> {
        self.types.adts.defs.get(name).cloned()
    }

    pub(super) fn resolve(&self, segs: &[String]) -> Option<Resolved> {
        self.types.adts.resolve(self.self_adt.as_deref(), segs)
    }

    /// How `{:?}` names a type: as written, also when inline-module flattening renamed it.
    fn display_name<'n>(&'n self, name: &'n str) -> &'n str {
        self.display_names
            .get(name)
            .map(String::as_str)
            .unwrap_or(name)
    }

    /// The JS class for a struct or enum (`module.Name` when imported).
    pub(super) fn class(&self, adt: &str) -> String {
        match self.types.adts.defs.get(adt).and_then(|d| d.module.clone()) {
            Some(m) => format!("{m}.{}", escape_ident(adt)),
            None => escape_ident(adt),
        }
    }

    /// The type parameters whose classes a struct's values carry (`this.$T`), in order.
    pub(super) fn stored_names(&self, adt: &str) -> Vec<String> {
        let Some(pos) = self.types.stored.get(adt) else {
            return Vec::new();
        };
        let params = &self.types.adts.defs[adt].type_params;
        pos.iter().filter_map(|&i| params.get(i).cloned()).collect()
    }

    /// Everything a struct's constructor takes after its fields: const parameters, then
    /// stored classes.
    pub(super) fn carried_names(&self, adt: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .types
            .adts
            .const_params
            .get(adt)
            .map(|cps| cps.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        out.extend(self.stored_names(adt));
        out
    }

    // ------------------------------------------------------------ items

    /// `export class Point { ... }` with the type's `impl` methods and derived helpers.
    pub(super) fn emit_adt(
        &mut self,
        node: &ra_ap_syntax::SyntaxNode,
        name: &ast::Name,
        impls: &[ast::Impl],
    ) {
        let Some(def) = self.adt(name.text().to_string().as_str()) else {
            return;
        };
        let cls = self.class(&def.name);
        let mut header = Frag::mapped(self.item_start(node), "export class ");
        header
            .append(Frag::mapped(self.off(name.syntax()), cls.clone()))
            .push(" {");
        self.line(header);
        self.out.indent += 1;
        // (`impl Drop`: the runtime calls its `drop` when a value of it is dropped.)
        if def.implements("Drop") {
            self.line(Frag::text("static $dropImpl = true;"));
        }
        // (Its JSON schema, for `serde_json::from_str::<T>` with `T: DeserializeOwned`.)
        if def.type_params.is_empty()
            && (def.derives("Deserialize") || def.derives("Serialize"))
            && self.src.contains("Deserialize")
        {
            let schema = self.json_schema_expr(&Ty::Adt(def.name.clone(), Vec::new()));
            self.line(Frag::text(format!(
                "static get $jsonSchema() {{ return {schema}; }}"
            )));
        }

        // Constructor and variant factories.
        if def.is_enum {
            self.line(Frag::text("constructor($tag) {"));
            self.line(Frag::text("  this.$tag = $tag;"));
            self.line(Frag::text("}"));
            // Const parameters (`enum Buf<const N: usize>`) are kept as `$N`, passed after the
            // fields (`$with` gives a unit variant's).
            let carried = self.carried_names(&def.name);
            let set_carried: Vec<String> = carried
                .iter()
                .enumerate()
                .map(|(k, n)| format!("  $v.${n} = $c{k};"))
                .collect();
            let cs: Vec<String> = (0..carried.len()).map(|k| format!("$c{k}")).collect();
            if !carried.is_empty() {
                self.line(Frag::text(format!(
                    "static $with($v, {}) {{",
                    cs.join(", ")
                )));
                for l in &set_carried {
                    self.line(Frag::text(l.clone()));
                }
                self.line(Frag::text("  return $v;"));
                self.line(Frag::text("}"));
            }
            for (i, v) in def.variants.iter().enumerate() {
                if v.shape == Shape::Unit {
                    // A fresh instance each time: `*self = ..` replaces contents in place.
                    self.line(Frag::text(format!("static get {}() {{", v.name)));
                    self.line(Frag::text(format!("  return new {cls}({i});")));
                    self.line(Frag::text("}"));
                    continue;
                }
                let params: Vec<String> = (0..v.fields.len()).map(|k| format!("${k}")).collect();
                self.line(Frag::text(format!(
                    "static {}({}) {{",
                    v.name,
                    params
                        .iter()
                        .chain(&cs)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
                self.line(Frag::text(format!("  const $v = new {cls}({i});")));
                for (k, p) in params.iter().enumerate() {
                    self.line(Frag::text(format!("  $v{} = {p};", access(v, k))));
                }
                for l in &set_carried {
                    self.line(Frag::text(l.clone()));
                }
                self.line(Frag::text("  return $v;"));
                self.line(Frag::text("}"));
            }
        } else {
            let v = &def.variants[0];
            // Const parameters (`Buf<const N: usize>`) are kept as `$N`: passed last, or read
            // off a field (`[T; N]`'s length) when the type wasn't known.
            let cps = self
                .types
                .adts
                .const_params
                .get(&def.name)
                .cloned()
                .unwrap_or_default();
            // (Then the classes of type parameters its methods use: `this.$T`.)
            let stored = self.stored_names(&def.name);
            if !v.fields.is_empty() || !cps.is_empty() || !stored.is_empty() {
                let mut params: Vec<String> =
                    (0..v.fields.len()).map(|k| format!("${k}")).collect();
                params.extend((0..cps.len() + stored.len()).map(|k| format!("$c{k}")));
                self.line(Frag::text(format!("constructor({}) {{", params.join(", "))));
                for (k, p) in params.iter().take(v.fields.len()).enumerate() {
                    self.line(Frag::text(format!("  this{} = {p};", access(v, k))));
                }
                for (k, cp) in cps.iter().enumerate() {
                    let read = match &cp.derive {
                        Some((f, suffix)) => format!(" ?? ${f}{suffix}"),
                        None => String::new(),
                    };
                    self.line(Frag::text(format!("  this.${} = $c{k}{read};", cp.name)));
                }
                for (k, n) in stored.iter().enumerate() {
                    let k = k + cps.len();
                    self.line(Frag::text(format!("  this.${n} = $c{k};")));
                }
                self.line(Frag::text("}"));
            }
        }

        // `impl` blocks.
        let saved = self.self_adt.replace(def.name.clone());
        for imp in impls {
            // Called as `Trait::method(..)` somewhere: also as `Trait$method`, which stays apart when
            // two traits name a method alike.
            if let (Some(tr), _) = crate::adt::impl_parts(imp)
                && self.qualified_traits().contains(&tr)
            {
                for item in imp
                    .assoc_item_list()
                    .into_iter()
                    .flat_map(|l| l.assoc_items())
                {
                    if let ast::AssocItem::Fn(f) = item
                        && let Some(n) = f.name()
                        && let Some(sig) = self.types.adts.method(&def.name, n.text()).cloned()
                    {
                        let js = format!("{tr}${}", n.text());
                        self.emit_method_fn(
                            &f,
                            sig,
                            Ty::Adt(def.name.clone(), vec![]),
                            "}",
                            Some(js),
                        );
                    }
                }
            }
            for item in imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                match item {
                    ast::AssocItem::Fn(f) => self.emit_class_method(&f, &def),
                    // `static W = 3;`: read as `Grid.W`.
                    ast::AssocItem::Const(k) => {
                        let (Some(n), Some(body)) = (k.name(), k.body()) else {
                            continue;
                        };
                        let ty = k.ty().map(|t| self.lower_type(&t));
                        let (f, _) = self.emit_expr(&body, ty.as_ref());
                        let mut line = Frag::mapped(
                            self.off(k.syntax()),
                            format!("static {} = ", escape_ident(n.text())),
                        );
                        line.append(f).push(";");
                        self.line(line);
                    }
                    _ => {}
                }
            }
        }
        self.self_adt = saved;

        self.emit_derives(&def, &cls, node);
        self.out.indent -= 1;
        self.line(Frag::text("}"));
        self.line(Frag::default());
    }

    fn emit_class_method(&mut self, f: &ast::Fn, def: &AdtDef) {
        let Some(name) = f.name() else { return };
        let key = self.types.adts.method_key(
            &def.name,
            name.text().to_string().as_str(),
            self.off(name.syntax()),
        );
        let Some(sig) = self.types.adts.method(&def.name, &key).cloned() else {
            return;
        };
        let js = (key != name.text()).then_some(key);
        self.emit_method_fn(f, sig, Ty::Adt(def.name.clone(), vec![]), "}", js);
    }

    /// A method in JS method syntax (`[static] [async] name(params) { .. }`), for a class body or
    /// an object literal (`close` is then `},`). `self_ty` is the type `self` has.
    fn emit_method_fn(
        &mut self,
        f: &ast::Fn,
        sig: crate::adt::Method,
        self_ty: Ty,
        close: &str,
        js_name: Option<String>,
    ) {
        let Some(name) = f.name() else { return };
        // `T::make()` in the body: the method takes `T`'s class (as free functions do).
        let dict_key = match &self_ty {
            Ty::Adt(n, _) => Some(format!(
                "{n}::{}",
                js_name.clone().unwrap_or_else(|| name.text().to_string())
            )),
            _ => None,
        };
        let dbg_key = dict_key.clone();
        let dicts = dict_key
            .and_then(|k| self.types.dict_params.get(&k).cloned())
            .unwrap_or_default();
        let saved_tmp = self.tmp;
        let saved_names = self.name_log.len();
        self.tmp = 0;
        self.ret_ty = sig.ret.clone();
        self.in_async = sig.is_async;
        self.scopes.push(HashMap::new());
        if sig.receiver.is_some() {
            // (A `&mut self` method on a string or number: `this` is a cell.)
            let js = if cell_self(&sig, &self_ty) {
                "this[0]"
            } else {
                "this"
            };
            self.declare(
                "self",
                None,
                Local {
                    js: js.into(),
                    ty: self_ty.clone(),
                },
            );
        }
        // `fn finish(self)`: `self` is dropped when the method ends, unless moved on.
        if matches!(sig.receiver, Some(crate::adt::Receiver::Value)) && self.self_drops(f, &self_ty)
        {
            self.param_drops.push(("this".into(), self_ty.clone()));
        }
        let mut params = Vec::new();
        let mut destructured: Vec<(ast::Pat, String)> = Vec::new();
        if let Some(pl) = f.param_list() {
            for (p, ty) in pl.params().zip(sig.params.clone()) {
                match p.pat() {
                    Some(ast::Pat::IdentPat(ip)) if ip.name().is_some() => {
                        let n = ip.name().unwrap();
                        let js = self.fresh(n.text().to_string().as_str());
                        let access = if self.is_boxed(n.syntax()) {
                            format!("{js}[0]")
                        } else {
                            js.clone()
                        };
                        if self.owned_param(&p, &ty) {
                            self.param_drops.push((access.clone(), ty.clone()));
                        }
                        self.declare(
                            n.text().to_string().as_str(),
                            Some(n.syntax()),
                            Local { js: access, ty },
                        );
                        params.push(Frag::mapped(self.off(n.syntax()), js));
                    }
                    Some(ast::Pat::WildcardPat(_)) => params.push(Frag::text(self.temp())),
                    Some(other) => {
                        let t = self.temp();
                        params.push(Frag::mapped(self.off(other.syntax()), t.clone()));
                        destructured.push((other, t));
                    }
                    None => {}
                }
            }
        }
        for d in &dicts {
            params.push(Frag::text(format!("${d}")));
        }
        let mut dbgs = dbg_key
            .and_then(|k| self.types.dbg_params.get(&k).cloned())
            .unwrap_or_default();
        // A blanket impl's method (`self: T`): the caller's `{:?}` formatter for `T`, last.
        if let Ty::Param(p) = &self_ty
            && p != "Self"
            && !dbgs.contains(p)
        {
            dbgs.push(p.clone());
        }
        for d in &dbgs {
            params.push(Frag::text(format!("$dbg${}", d.replace("::", "$"))));
        }
        let saved_dbgs = super::set_dbg_scope(dbgs.clone());
        let saved_dicts = std::mem::replace(&mut self.dict_scope, dicts);
        // The `impl`'s const parameters: `this.$N` in a method, passed (`$N`) to an
        // associated function.
        let mut outer = Vec::new();
        // (An associated function called through a class view, `κσ.$cv(Buf, [4]).default()`,
        // reads them off it.)
        let mut view_lines = Vec::new();
        if let Some(imp) = f
            .syntax()
            .parent()
            .and_then(|l| l.parent())
            .and_then(ast::Impl::cast)
            && let Ty::Adt(adt, _) = &self_ty
        {
            let struct_params = self
                .types
                .adts
                .defs
                .get(adt)
                .map(|d| d.type_params.clone())
                .unwrap_or_default();
            for p in imp
                .generic_param_list()
                .into_iter()
                .flat_map(|l| l.generic_params())
            {
                if let ast::GenericParam::ConstParam(cp) = p
                    && let Some(n) = cp.name()
                {
                    let n = n.text().to_string();
                    let ty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                    let pos = sig
                        .impl_args
                        .iter()
                        .position(|a| *a == Ty::Param(n.clone()));
                    let js = if sig.receiver.is_some() {
                        match pos.and_then(|i| struct_params.get(i)) {
                            Some(x) => format!("this.${x}"),
                            None => "undefined".into(),
                        }
                    } else {
                        if let Some(i) = pos {
                            view_lines.push(format!("${n} ??= this?.$cargs?.[{i}];"));
                        }
                        format!("${n}")
                    };
                    outer.push((n, js, ty));
                }
            }
        }
        // `impl<const N: usize> Tr for [u8; N]`: `N` is read off `self` (`this?.length`).
        if let Some(imp) = f
            .syntax()
            .parent()
            .and_then(|l| l.parent())
            .and_then(ast::Impl::cast)
            && !matches!(self_ty, Ty::Adt(..))
            && sig.receiver.is_some()
            && let Some(st) = imp.self_ty()
        {
            for p in imp
                .generic_param_list()
                .into_iter()
                .flat_map(|l| l.generic_params())
            {
                if let ast::GenericParam::ConstParam(cp) = p
                    && let Some(n) = cp.name()
                {
                    let n = n.text().to_string();
                    let ty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                    let js = match crate::adt::const_path(&st, &n, &self.types.adts) {
                        Some(suffix) => format!("this{suffix}"),
                        None => "undefined".into(),
                    };
                    outer.push((n, js, ty));
                }
            }
        }
        let saved_consts = super::set_const_scope(None);
        // An `impl<T>`'s `T` whose class the values carry: `this.$T` in a method with `self`.
        if sig.receiver.is_some()
            && let Ty::Adt(adt, _) = &self_ty
            && let Some(pos) = self.types.stored.get(adt)
        {
            let struct_params = self.types.adts.defs[adt].type_params.clone();
            for &i in pos {
                if let Some(Ty::Param(n)) = sig.impl_args.get(i)
                    && let Some(x) = struct_params.get(i)
                {
                    super::CONST_SCOPE
                        .with(|s| s.borrow_mut().insert(n.clone(), format!("this.${x}")));
                }
            }
        }
        let mut const_lines = self.enter_consts(f, outer);
        const_lines.extend(view_lines);
        // A hand-written `impl<T: Debug> Debug for Wrapper<T>`: the formatters for the type's
        // arguments come in on the Formatter (`f.$fs`), as a derived `$dbg` gets them.
        if js_name.as_deref() == Some("fmt$dbg")
            && let Some(imp) = f
                .syntax()
                .parent()
                .and_then(|l| l.parent())
                .and_then(ast::Impl::cast)
            && let Some(fname) = f
                .param_list()
                .and_then(|pl| pl.params().next())
                .and_then(|p| match p.pat()? {
                    ast::Pat::IdentPat(ip) => Some(ip.name()?.text().to_string()),
                    _ => None,
                })
            && let Some(fjs) = self.lookup(&fname).map(|l| l.js.clone())
        {
            let mut scope = dbgs;
            for p in imp
                .generic_param_list()
                .into_iter()
                .flat_map(|l| l.generic_params())
            {
                if let ast::GenericParam::TypeParam(tp) = p
                    && let Some(n) = tp.name()
                    && let Some(i) = sig
                        .impl_args
                        .iter()
                        .position(|a| *a == Ty::Param(n.text().to_string()))
                    && !scope.contains(&n.text().to_string())
                {
                    const_lines.push(format!("const $dbg${} = {fjs}.$fs?.[{i}];", n.text()));
                    scope.push(n.text().to_string());
                }
            }
            super::set_dbg_scope(scope);
        }
        let mut header = Frag::mapped(self.item_start(f.syntax()), "");
        // (In an object literal, `close` is `},`: no `static` there.)
        if sig.receiver.is_none() && close == "}" {
            header.push("static ");
        }
        if sig.is_async {
            header.push("async ");
        }
        header
            .append(Frag::mapped(
                self.off(name.syntax()),
                prop(
                    js_name
                        .as_deref()
                        .unwrap_or(name.text().to_string().as_str()),
                ),
            ))
            .push("(")
            .append(Frag::join(params, ", "))
            .push(") {");
        self.line(header);
        self.out.indent += 1;
        for l in const_lines {
            self.line(Frag::text(l));
        }
        // Destructured parameters (`(r, c): (usize, usize)`): bound from their temps first.
        for (pat, t) in destructured {
            let mut pc = PatCode::default();
            self.lower_pat(&pat, &t, &mut pc);
            self.emit_binds(pc.binds);
        }
        if let Some(body) = f.body() {
            self.emit_block_body(&body, &Dest::Return, None);
        }
        self.out.indent -= 1;
        self.close_line(close, f.body().as_ref());
        self.drop_flags.remove("this");
        self.scopes.pop();
        self.dict_scope = saved_dicts;
        super::set_const_scope(Some(saved_consts));
        super::set_dbg_scope(saved_dbgs);
        self.tmp = saved_tmp;
        self.undo_names(saved_names);
    }

    /// Whether a method taking `self` by value drops it at its end: its type has a value to drop.
    /// (Fields moved out with `self.field` are skipped by the drop; a pattern taking `self`
    /// apart, `let S { a, .. } = self;`, moves fields without one, so then it's left.)
    fn self_drops(&self, f: &ast::Fn, self_ty: &Ty) -> bool {
        if !self.needs_drop(self_ty) {
            return false;
        }
        if let Ty::Adt(n, _) = self_ty
            && self.adt(n).is_some_and(|d| d.implements("Drop"))
        {
            return true;
        }
        let Some(body) = f.body() else { return false };
        let destructures = |e: Option<ast::Expr>| {
            e.is_some_and(|e| matches!(crate::typeck::strip_parens(&e), ast::Expr::PathExpr(p) if p.syntax().text() == "self"))
        };
        !body.syntax().descendants().any(|n| {
            ast::LetStmt::cast(n.clone()).is_some_and(|l| {
                !matches!(l.pat(), Some(ast::Pat::IdentPat(_))) && destructures(l.initializer())
            }) || ast::MatchExpr::cast(n.clone()).is_some_and(|m| destructures(m.expr()))
                || ast::LetExpr::cast(n).is_some_and(|l| destructures(l.expr()))
        })
    }

    /// `class $ext$f64 { .. }`: the user trait methods of a built-in type, called as
    /// `$ext$f64.prototype.money.call(x)`. Inherited default methods are compiled again here with
    /// the concrete `self` type, so their calls on `self` resolve the same way.
    pub(super) fn emit_ext_class(&mut self, key: &str, impls: &[ast::Impl]) {
        // A blanket impl's `self` is its type parameter.
        let blanket = key
            .ends_with("$all")
            .then(|| impls.first().and_then(crate::adt::blanket_impl))
            .flatten();
        let self_ty = match key {
            _ if blanket.is_some() => Ty::Param(blanket.map(|(_, t, _)| t).unwrap_or_default()),
            "String" => Ty::Str,
            "Vec" => Ty::Vec(Box::new(Ty::Unknown)),
            "Option" => Ty::Option(Box::new(Ty::Unknown)),
            "Result" => Ty::Result(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            "HashMap" => Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            "HashSet" => Ty::Set(Box::new(Ty::Unknown)),
            "BTreeMap" => Ty::Coll(
                crate::ty::Coll::BTreeMap,
                Box::new(Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))),
            ),
            "BTreeSet" => Ty::Coll(
                crate::ty::Coll::BTreeSet,
                Box::new(Ty::Set(Box::new(Ty::Unknown))),
            ),
            "VecDeque" | "Range" | "RangeInclusive" => ext_self_ty(key),
            "bool" => Ty::Bool,
            "char" => Ty::Char,
            k => Ty::from_name(k),
        };
        self.line(Frag::text(format!("class $ext${key} {{")));
        self.out.indent += 1;
        let mut given: Vec<String> = Vec::new();
        let mut traits: Vec<String> = Vec::new();
        for imp in impls {
            if let (Some(tr), _) = crate::adt::impl_parts(imp) {
                traits.push(tr);
            }
            for item in imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                // A generic trait's second impl for this type: its methods under their keys.
                if let ast::AssocItem::Fn(f) = &item
                    && let Some(n) = f.name()
                    && let Some(k) = self
                        .types
                        .adts
                        .alt_keys_at
                        .get(&self.off(n.syntax()))
                        .cloned()
                    && let Some(sig) = self.types.adts.method(key, &k).cloned()
                {
                    self.emit_method_fn(f, sig, self_ty.clone(), "}", Some(k));
                    continue;
                }
                // `impl Mul<V> for f64`'s `mul`, as its `mul$N`.
                if let ast::AssocItem::Fn(f) = &item
                    && let Some(n) = f.name()
                    && let Some(oi) = self.types.adts.op_impls.get(key).and_then(|v| {
                        let at = self.off(n.syntax());
                        v.iter().find(|i| i.at == at).cloned()
                    })
                    && let Some(sig) = self.types.adts.method(key, &oi.key).cloned()
                {
                    self.emit_method_fn(f, sig, self_ty.clone(), "}", Some(oi.key.clone()));
                    continue;
                }
                // `impl From<Id> for String`'s `from`, as its `from$N`.
                if let ast::AssocItem::Fn(f) = &item
                    && let Some(n) = f.name()
                    && n.text() == "from"
                    && let Some(fi) = self.types.adts.from_impls.get(key).and_then(|v| {
                        let at = self.off(n.syntax());
                        v.iter().find(|i| i.at == at).cloned()
                    })
                    && let Some(sig) = self.types.adts.method(key, &fi.key).cloned()
                {
                    self.emit_method_fn(f, sig, self_ty.clone(), "}", Some(fi.key.clone()));
                    continue;
                }
                if let ast::AssocItem::Fn(f) = item
                    && let Some(n) = f.name()
                    && let Some(sig) = self.types.adts.method(key, n.text()).cloned()
                {
                    given.push(n.text().to_string());
                    self.emit_method_fn(&f, sig, self_ty.clone(), "}", None);
                }
            }
        }
        // Default methods the impls don't override, from the trait declarations in this file.
        let defaults: Vec<ast::Fn> = self
            .file_traits
            .iter()
            .filter(|t| {
                t.name()
                    .is_some_and(|n| traits.contains(&n.text().to_string()))
            })
            .flat_map(|t| {
                t.assoc_item_list()
                    .into_iter()
                    .flat_map(|l| l.assoc_items())
            })
            .filter_map(|i| match i {
                ast::AssocItem::Fn(f) if f.body().is_some() => Some(f),
                _ => None,
            })
            .filter(|f| {
                f.name()
                    .is_some_and(|n| !given.contains(&n.text().to_string()))
            })
            .collect();
        for f in defaults {
            let Some(n) = f.name() else { continue };
            if let Some(sig) = self.types.adts.method(key, n.text()).cloned() {
                self.emit_method_fn(&f, sig, self_ty.clone(), "}", None);
            }
        }
        self.out.indent -= 1;
        self.line(Frag::text("}"));
        // The `&mut self` methods, which `κσ.$dcallMut` calls with the receiver's cell.
        let muts: Vec<String> = self
            .types
            .adts
            .methods
            .get(key)
            .map(|ms| {
                let mut v: Vec<String> = ms
                    .iter()
                    .filter(|(_, s)| cell_self(s, &self_ty))
                    .map(|(n, _)| super::js_string(&prop(n)))
                    .collect();
                v.sort();
                v
            })
            .unwrap_or_default();
        if !muts.is_empty() {
            self.line(Frag::text(format!(
                "$ext${key}.$mut = new Set([{}]);",
                muts.join(", ")
            )));
        }
        // `Box<dyn Shape>`'s `from`s: called by name only.
        if key.starts_with("dyn$") {
            self.line(Frag::default());
            return;
        }
        // A blanket impl's class: `κσ.$dcall` tries it for any value.
        if key.ends_with("$all") {
            self.line(Frag::text(format!("κσ.$blanketReg($extRegs, $ext${key});")));
            self.line(Frag::default());
            return;
        }
        // Found by `κσ.$dcall` when a generic function calls the method on a built-in value.
        let (kind, integral) = match key {
            "String" | "char" => ("string", false),
            "bool" => ("boolean", false),
            "Vec" => ("array", false),
            // (An `Option` is `null` or its value: only called directly, never found here.)
            "Option" => ("option", false),
            "Result" => ("result", false),
            "HashMap" => ("map", false),
            "HashSet" => ("set", false),
            "BTreeMap" => ("btreemap", false),
            "BTreeSet" => ("btreeset", false),
            "VecDeque" => ("deque", false),
            // (A range value is an `Iter` over a `RangeIt`.)
            "Range" => ("range", false),
            "RangeInclusive" => ("rangeincl", false),
            "f32" | "f64" => ("number", false),
            // (The 64- and 128-bit kinds are BigInts.)
            "i64" | "u64" | "i128" | "u128" => ("bigint", true),
            _ => ("number", true),
        };
        self.line(Frag::text(format!(
            "κσ.$extReg($extRegs, \"{kind}\", $ext${key}, {integral});"
        )));
        self.line(Frag::default());
    }

    /// `export const Trait$defaults = { method() { .. }, .. };`: a trait's default methods, which
    /// implementing classes mix in (`κσ.$mixin`) unless they define their own.
    pub(super) fn emit_trait_defaults(&mut self, t: &ast::Trait) {
        let Some(name) = t.name() else { return };
        let tr = name.text().to_string();
        let Some(def) = self.types.adts.traits.get(&tr).cloned() else {
            return;
        };
        let mut header = Frag::mapped(self.item_start(t.syntax()), "export const ");
        header
            .append(Frag::mapped(
                self.off(name.syntax()),
                format!("{}$defaults", escape_ident(&tr)),
            ))
            .push(" = {");
        self.line(header);
        self.out.indent += 1;
        self.in_trait_default = true;
        // Methods here; functions without `self` in `Trait$statics`, mixed onto the class.
        let defaults = |statics: bool| {
            let mut out = Vec::new();
            for item in t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                if let ast::AssocItem::Fn(f) = item
                    && f.body().is_some()
                    && let Some(n) = f.name()
                    && let Some(tm) = def.methods.iter().find(|m| m.name == n.text())
                    && tm.sig.receiver.is_none() == statics
                {
                    out.push((f, tm.sig.clone()));
                }
            }
            out
        };
        for (f, sig) in defaults(false) {
            self.emit_method_fn(&f, sig, Ty::Param("Self".into()), "},", None);
        }
        self.out.indent -= 1;
        self.line(Frag::text("};"));
        self.line(Frag::text(format!(
            "export const {}$statics = {{",
            escape_ident(&tr)
        )));
        self.out.indent += 1;
        for (f, sig) in defaults(true) {
            self.emit_method_fn(&f, sig, Ty::Param("Self".into()), "},", None);
        }
        // Default consts as getters on the class (`this` is the class: `Self::N` is `this.N`);
        // an impl's own `static N` stays.
        for item in t
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
        {
            if let ast::AssocItem::Const(k) = item
                && let (Some(n), Some(body)) = (k.name(), k.body())
            {
                let ty = k.ty().map(|t| self.lower_type(&t));
                let (f, _) = self.emit_expr(&body, ty.as_ref());
                let mut line = Frag::mapped(
                    self.off(k.syntax()),
                    format!("get {}() {{ return ", escape_ident(n.text())),
                );
                line.append(f).push("; },");
                self.line(line);
            }
        }
        self.out.indent -= 1;
        self.in_trait_default = false;
        self.line(Frag::text("};"));
        self.line(Frag::default());
    }

    /// Traits named by a qualified call in this file (`Trait::m(..)`, `<T as Trait>::m(..)`).
    fn qualified_traits(&self) -> std::collections::HashSet<String> {
        self.types
            .qualified_calls
            .values()
            .map(|q| q.trait_name.clone())
            .collect()
    }

    /// `κσ.$mixin(Class, Trait$defaults)` for each user trait a local type implements.
    pub(super) fn emit_mixins(&mut self) {
        let mut impls: Vec<(String, Vec<String>)> = self
            .types
            .adts
            .trait_impls
            .iter()
            .filter(|(adt, _)| self.local_adts.contains(*adt))
            .map(|(a, ts)| (a.clone(), ts.clone()))
            .collect();
        impls.sort();
        for (adt, traits) in impls {
            for tr in traits {
                let prefix = match self
                    .types
                    .adts
                    .traits
                    .get(&tr)
                    .and_then(|d| d.module.clone())
                {
                    Some(m) => format!("{m}.{}", escape_ident(&tr)),
                    None => escape_ident(&tr),
                };
                let cls = self.class(&adt);
                self.line(Frag::text(format!(
                    "κσ.$mixin({cls}, {prefix}$defaults, {}, {prefix}$statics);",
                    super::js_string(&tr)
                )));
            }
        }
    }

    /// `$clone`, `$eq`, `$cmp`, `$dbg` and `static default()` for the derives.
    fn emit_derives(&mut self, def: &AdtDef, cls: &str, node: &ra_ap_syntax::SyntaxNode) {
        let lines = |cg: &mut Self, ls: Vec<String>| {
            for l in ls {
                cg.line(Frag::text(l));
            }
        };
        // Hand-written `impl Ord` / `PartialOrd` / `PartialEq` / `Clone`: the runtime's
        // comparisons and copies call `$cmp` / `$eq` / `$clone`, which forward to them.
        let manual = |t: &str| def.manual.iter().any(|m| m == t);
        let call = |m: &str| format!("this{}", super::member(&prop(m)));
        if manual("Ord") {
            lines(
                self,
                vec![format!("$cmp(o) {{ return {}(o); }}", call("cmp"))],
            );
        } else if manual("PartialOrd") {
            // `None` (incomparable) makes every `<`/`>` false, like NaN.
            lines(
                self,
                vec![format!(
                    "$cmp(o) {{ return {}(o) ?? NaN; }}",
                    call("partial_cmp")
                )],
            );
        }
        if manual("PartialEq") {
            lines(
                self,
                vec![format!("$eq(o) {{ return {}(o); }}", call("eq"))],
            );
        }
        // (A `Copy` type's copies are field by field, as Rust's are bitwise, even with a
        // hand-written `clone`, which often is `*self`.)
        if manual("Clone") && !def.derives("Copy") {
            lines(
                self,
                vec![format!("$clone() {{ return {}(); }}", call("clone"))],
            );
        }
        // `impl IntoIterator`: `for...of` goes through its `into_iter()`.
        if self.types.adts.into_iters.contains(&def.name) {
            lines(
                self,
                vec![format!(
                    "[Symbol.iterator]() {{ return this{}()[Symbol.iterator](); }}",
                    super::member(&prop("into_iter"))
                )],
            );
        } else if self.types.adts.iter_items.contains_key(&def.name) {
            // `impl Iterator`: `for...of` and the runtime's iterators drive its `next`.
            lines(
                self,
                vec![format!(
                    "[Symbol.iterator]() {{ return {{ next: () => {{ const v = this{}(); return v == null ? {{ done: true, value: undefined }} : {{ done: false, value: v }}; }} }}; }}",
                    super::member(&prop("next"))
                )],
            );
        }
        // `impl Future`: awaiting it (JS's `await` takes any thenable) polls it until it's ready,
        // with a context whose waker resumes the polling (the task prelude's).
        if self.types.adts.future_outputs.contains_key(&def.name) {
            lines(
                self,
                vec![format!(
                    "then(ok, err) {{ return κσ.$drive(this, \"{}\", __task_context).then(ok, err); }}",
                    prop("poll")
                )],
            );
        }
        // A hand-written `Hash` with a hand-written `PartialEq`: `HashMap` / `HashSet` keys are
        // what `hash` writes (`JSON.stringify` uses `toJSON`), so values `eq` calls equal (by
        // the same fields) are one key.
        if manual("Hash") && manual("PartialEq") {
            lines(
                self,
                vec![format!(
                    "toJSON() {{ const h = new κσ.Hasher(); {}(h); return h.parts; }}",
                    call("hash")
                )],
            );
        }
        // `impl Add for V` (with `V` on the right) and friends as `$op$add`: what generic
        // arithmetic on a `T` calls (`κσ.$gop`).
        let ops: Vec<(String, String)> = self
            .types
            .adts
            .op_impls
            .get(&def.name)
            .map(|v| {
                v.iter()
                    .filter(|i| {
                        matches!(&i.rhs, Ty::Adt(n, _) if *n == def.name)
                            || matches!(i.trait_name.as_str(), "Neg" | "Not")
                    })
                    .filter_map(|i| {
                        Some((
                            crate::adt::op_method(&i.trait_name)?.to_string(),
                            i.key.clone(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Each `From<X>` impl as `$from$X` (what `T::from(x)` with `T: From<X>` calls).
        let froms: Vec<(String, String)> = self
            .types
            .adts
            .from_impls
            .get(&def.name)
            .map(|v| {
                v.iter()
                    .map(|i| (i.src.describe(), i.key.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (src, key) in froms {
            let js = format!("this{}", super::member(&prop(&key)));
            lines(
                self,
                vec![format!(
                    "static [{}](x) {{ return {js}(x); }}",
                    super::js_string(&format!("$from${src}"))
                )],
            );
        }
        // ... and each `TryFrom<X>` impl's `try_from` as `$tryfrom$X` (for `T: TryFrom<X>`).
        let adts = &self.types.adts;
        let try_keys = adts
            .method_alts
            .get(&format!("{}::try_from", def.name))
            .cloned()
            .unwrap_or_else(|| vec!["try_from".to_string()]);
        let tries: Vec<(String, String)> = try_keys
            .into_iter()
            .filter_map(|k| {
                let sig = adts.method(&def.name, &k)?;
                (sig.receiver.is_none() && sig.params.len() == 1)
                    .then(|| (sig.params[0].describe(), k))
            })
            .collect();
        for (src, key) in tries {
            let js = format!("this{}", super::member(&prop(&key)));
            lines(
                self,
                vec![format!(
                    "static [{}](x) {{ return {js}(x); }}",
                    super::js_string(&format!("$tryfrom${src}"))
                )],
            );
        }
        let mut seen: Vec<String> = Vec::new();
        for (m, key) in ops {
            if seen.contains(&m) {
                continue;
            }
            let js = format!("this{}", super::member(&prop(&key)));
            lines(self, vec![format!("$op${m}(o) {{ return {js}(o); }}")]);
            seen.push(m);
        }
        // `impl fmt::Debug`: `{:?}` writes through its `fmt` (emitted as `fmt$dbg`).
        if manual("Debug") {
            lines(
                self,
                vec![
                    "$dbg(...$fs) { const f = { s: \"\", $fs }; this.fmt$dbg(f); return f.s; }"
                        .to_string(),
                ],
            );
        }
        // Per-variant bodies, switched on `$tag` for enums.
        let per_variant = |def: &AdtDef,
                           body: &dyn Fn(usize, &Variant) -> String,
                           fallback: &str|
         -> Vec<String> {
            if !def.is_enum {
                return vec![format!("  {}", body(0, &def.variants[0]))];
            }
            let mut out = vec!["  switch (this.$tag) {".to_string()];
            for (i, v) in def.variants.iter().enumerate() {
                out.push(format!("    case {i}: {}", body(i, v)));
            }
            out.push("  }".into());
            out.push(format!("  {fallback}"));
            out
        };
        if def.derives("Clone") || (def.derives("Copy") && manual("Clone")) {
            let mut ls = vec!["$clone() {".to_string()];
            if def.is_enum && !def.has_data() {
                ls.push("  return this;".into());
            } else {
                ls.extend(per_variant(
                    def,
                    &|_, v| {
                        let fields: Vec<String> = v
                            .fields
                            .iter()
                            .enumerate()
                            .map(|(k, f)| clone_of(&f.ty, &format!("this{}", access(v, k))))
                            .collect();
                        if def.is_enum {
                            if v.shape == Shape::Unit {
                                "return this;".to_string()
                            } else {
                                let mut fields = fields;
                                fields.extend(
                                    self.carried_names(&def.name)
                                        .iter()
                                        .map(|n| format!("this.${n}")),
                                );
                                format!("return {cls}.{}({});", v.name, fields.join(", "))
                            }
                        } else {
                            let mut fields = fields;
                            fields.extend(
                                self.carried_names(&def.name)
                                    .iter()
                                    .map(|n| format!("this.${n}")),
                            );
                            format!("return new {cls}({});", fields.join(", "))
                        }
                    },
                    "return this;",
                ));
            }
            ls.push("}".into());
            lines(self, ls);
        }
        if def.derives("PartialEq") {
            let mut ls = vec!["$eq(o) {".to_string()];
            if def.is_enum {
                ls.push("  if (this.$tag !== o.$tag) return false;".into());
            }
            ls.extend(per_variant(
                def,
                &|_, v| {
                    let parts: Vec<String> = (0..v.fields.len())
                        .map(|k| {
                            eq_of(
                                &v.fields[k].ty,
                                &format!("this{}", access(v, k)),
                                &format!("o{}", access(v, k)),
                            )
                        })
                        .collect();
                    if parts.is_empty() {
                        "return true;".into()
                    } else {
                        format!("return {};", parts.join(" && "))
                    }
                },
                "return true;",
            ));
            ls.push("}".into());
            lines(self, ls);
        }
        if def.derives("PartialOrd") {
            let mut ls = vec!["$cmp(o) {".to_string()];
            if def.is_enum {
                ls.push("  if (this.$tag !== o.$tag) return this.$tag < o.$tag ? -1 : 1;".into());
            }
            ls.extend(per_variant(
                def,
                &|_, v| {
                    let parts: Vec<String> = (0..v.fields.len())
                        .map(|k| format!("κσ.$cmp(this{}, o{})", access(v, k), access(v, k)))
                        .collect();
                    if parts.is_empty() {
                        "return 0;".into()
                    } else {
                        partial_chain(&parts)
                    }
                },
                "return 0;",
            ));
            ls.push("}".into());
            lines(self, ls);
        }
        if def.derives("Debug") {
            // `$fs[i]` formats type parameter `i` (callers that know the type pass them).
            let params: HashMap<String, String> = def
                .type_params
                .iter()
                .enumerate()
                .map(|(i, p)| (p.clone(), format!("($fs[{i}] ?? κσ.$dbgAny)")))
                .collect();
            let mut ls = vec!["$dbg(...$fs) {".to_string()];
            ls.extend(per_variant(
                def,
                &|_, v| {
                    let vals: Vec<String> = v
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(k, f)| dbg_of(&f.ty, &format!("this{}", access(v, k)), &params))
                        .collect();
                    match v.shape {
                        Shape::Unit => {
                            format!("return {};", super::js_string(self.display_name(&v.name)))
                        }
                        Shape::Tuple => format!(
                            "return κσ.$dbgStruct({}, null, [{}]);",
                            super::js_string(self.display_name(&v.name)),
                            vals.join(", ")
                        ),
                        Shape::Named => {
                            let names: Vec<String> =
                                v.fields.iter().map(|f| super::js_string(&f.name)).collect();
                            format!(
                                "return κσ.$dbgStruct({}, [{}], [{}]);",
                                super::js_string(self.display_name(&v.name)),
                                names.join(", "),
                                vals.join(", ")
                            )
                        }
                    }
                },
                "return \"?\";",
            ));
            ls.push("}".into());
            lines(self, ls);
        }
        if def.derives("Default") {
            let body = if def.is_enum {
                let carried: Vec<String> = self
                    .carried_names(&def.name)
                    .iter()
                    .map(|c| {
                        let i = def.type_params.iter().position(|p| p == c).unwrap_or(0);
                        format!("this?.$cargs?.[{i}]")
                    })
                    .collect();
                match def.default_variant {
                    Some(i) if !carried.is_empty() => format!(
                        "return {cls}.$with({cls}.{}, {});",
                        def.variants[i].name,
                        carried.join(", ")
                    ),
                    Some(i) => format!("return {cls}.{};", def.variants[i].name),
                    None => "return undefined;".into(),
                }
            } else {
                let v = &def.variants[0];
                // (A `[T; N]` field: N elements, by its written type.)
                let resolved: Vec<Ty> = v.fields.iter().map(|f| f.ty.clone()).collect();
                let arrays = self.field_array_defaults(node, &resolved);
                let mut vals: Vec<String> = v
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(k, f)| {
                        arrays
                            .get(k)
                            .cloned()
                            .flatten()
                            .or_else(|| default_value(&f.ty))
                            .unwrap_or_else(|| "undefined".into())
                    })
                    .collect();
                for c in self.carried_names(&def.name) {
                    let i = def.type_params.iter().position(|p| *p == c).unwrap_or(0);
                    vals.push(format!("this?.$cargs?.[{i}]"));
                }
                format!("return new {cls}({});", vals.join(", "))
            };
            lines(
                self,
                vec!["static default() {".into(), format!("  {body}"), "}".into()],
            );
        }
    }

    // ------------------------------------------------------------ expressions

    /// Construct variant `v` of `adt` from field values in declaration order.
    pub(super) fn construct(&self, adt: &str, v: usize, args: Vec<Frag>) -> Frag {
        let def = &self.types.adts.defs[adt];
        let cls = self.class(adt);
        let variant = &def.variants[v];
        if def.is_enum {
            if variant.shape == Shape::Unit {
                if !args.is_empty() {
                    // (Its const parameters: `Buf.$with(Buf.Empty, 4)`.)
                    let mut f = Frag::text(format!("{cls}.$with({cls}.{}, ", variant.name));
                    f.append(Frag::join(args, ", ")).push(")");
                    return f;
                }
                return Frag::text(format!("{cls}.{}", variant.name));
            }
            let mut f = Frag::text(format!("{cls}.{}(", variant.name));
            f.append(Frag::join(args, ", ")).push(")");
            f
        } else {
            let mut f = Frag::text(format!("new {cls}("));
            f.append(Frag::join(args, ", ")).push(")");
            f
        }
    }

    /// `Point { y: b, x: a }` → `new Point(a, b)`. When reordering would change the order side
    /// effects happen in, or a non-trivial `..base` is read, the values are passed through an
    /// arrow function so they still evaluate in source order.
    pub(super) fn emit_record(&mut self, r: &ast::RecordExpr) -> (Frag, Ty) {
        let segs = path_segments(r.path());
        let Some(Resolved::Ctor(adt, v)) = self.resolve(&segs) else {
            if let Some(r) = self.emit_iface_record(r, &segs) {
                return r;
            }
            self.error(r.syntax(), "cannot find this struct or variant");
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let variant = self.types.adts.defs[&adt].variants[v].clone();
        let list = r.record_expr_field_list();
        let mut given: Vec<(usize, Frag, bool)> = Vec::new();
        for f in list.iter().flat_map(|l| l.fields()) {
            let Some(fname) = f.field_name() else {
                continue;
            };
            let Some((i, field)) = variant.field(fname.text().to_string().as_str()) else {
                continue;
            };
            let (frag, pure, ty) = match f.expr() {
                Some(e) => {
                    let (frag, ty) = self.emit_expr(&e, Some(&field.ty));
                    let frag = self.by_value(&e, frag, &ty);
                    (frag, is_pure(&e), ty)
                }
                None => {
                    let js = self
                        .lookup(fname.text().to_string().as_str())
                        .map(|l| l.js.clone())
                        .unwrap_or_else(|| "undefined".into());
                    (
                        Frag::mapped(self.off(fname.syntax()), js),
                        true,
                        field.ty.clone(),
                    )
                }
            };
            let _ = ty;
            given.push((i, frag, pure));
        }
        let base = list.and_then(|l| l.spread());
        let in_order = given.windows(2).all(|w| w[0].0 < w[1].0);
        let impure = given.iter().filter(|g| !g.2).count();
        let base_pure = base.as_ref().is_none_or(is_pure);
        let start = self.off(r.syntax());
        if (in_order || impure <= 1) && base_pure {
            let base_js = base.map(|b| self.emit_expr(&b, None).0.plain());
            let mut args: Vec<Frag> = (0..variant.fields.len())
                .map(|i| match given.iter().find(|g| g.0 == i) {
                    Some(g) => g.1.clone(),
                    None => {
                        let b = base_js.clone().unwrap_or_else(|| "undefined".into());
                        Frag::text(clone_of(
                            &variant.fields[i].ty,
                            &format!("{b}{}", access(&variant, i)),
                        ))
                    }
                })
                .collect();
            args.extend(self.const_args_of(r.syntax(), &adt));
            return (
                self.construct(&adt, v, args).at(start),
                Ty::Adt(adt, vec![]),
            );
        }
        // `((a, b, base) => new Point(b, a, base.z))(exprA, exprB, base)`
        let params: Vec<String> = (0..given.len()).map(|k| format!("$f{k}")).collect();
        let mut call_args: Vec<Frag> = given.iter().map(|g| g.1.clone()).collect();
        let base_param = if let Some(b) = &base {
            call_args.push(self.emit_expr(b, None).0);
            Some("$base".to_string())
        } else {
            None
        };
        let mut args: Vec<Frag> = (0..variant.fields.len())
            .map(|i| match given.iter().position(|g| g.0 == i) {
                Some(k) => Frag::text(params[k].clone()),
                None => Frag::text(clone_of(
                    &variant.fields[i].ty,
                    &format!(
                        "{}{}",
                        base_param.clone().unwrap_or_else(|| "undefined".into()),
                        access(&variant, i)
                    ),
                )),
            })
            .collect();
        let mut all_params = params.clone();
        all_params.extend(base_param);
        let mut f = Frag::text(format!("(({}) => ", all_params.join(", ")));
        args.extend(self.const_args_of(r.syntax(), &adt));
        f.append(self.construct(&adt, v, args))
            .push(")(")
            .append(Frag::join(call_args, ", "))
            .push(")");
        (f.at(start), Ty::Adt(adt, vec![]))
    }

    /// A path naming a unit struct or variant, a tuple constructor, or an associated function
    /// used as a value.
    pub(super) fn emit_adt_path(&mut self, segs: &[String]) -> Option<(Frag, Ty)> {
        Some(match self.resolve(segs)? {
            Resolved::Ctor(adt, v) => {
                let def = self.types.adts.defs[&adt].clone();
                let cls = self.class(&adt);
                let variant = &def.variants[v];
                match (variant.shape, def.is_enum) {
                    (Shape::Unit, true) => (
                        Frag::text(format!("{cls}.{}", variant.name)),
                        Ty::Adt(adt, vec![]),
                    ),
                    (Shape::Unit, false) => {
                        (Frag::text(format!("new {cls}()")), Ty::Adt(adt, vec![]))
                    }
                    (_, true) => (Frag::text(format!("{cls}.{}", variant.name)), Ty::Unknown),
                    (_, false) => (
                        Frag::text(format!("((...$a) => new {cls}(...$a))")),
                        Ty::Unknown,
                    ),
                }
            }
            Resolved::Assoc(adt, m)
                if self.types.adts.method(&adt, &m).is_none()
                    && self
                        .types
                        .adts
                        .assoc_consts
                        .get(&adt)
                        .is_some_and(|c| c.contains_key(&m)) =>
            {
                let ty = self.types.adts.assoc_consts[&adt][&m].clone();
                (
                    Frag::text(format!("{}.{}", self.class(&adt), escape_ident(&m))),
                    ty,
                )
            }
            Resolved::Assoc(adt, m) => {
                let cls = self.class(&adt);
                let js = prop(&m);
                match self.types.adts.method(&adt, &m).map(|s| s.receiver) {
                    Some(None) => (Frag::text(format!("{cls}.{js}")), Ty::Unknown),
                    Some(Some(_)) => (
                        Frag::text(format!("(($s, ...$a) => $s.{js}(...$a))")),
                        Ty::Unknown,
                    ),
                    None => return None,
                }
            }
        })
    }

    /// `Point(1, 2)`, `Shape::Circle(r)`, `Point::new(..)`, `Point::norm(&p)`.
    pub(super) fn emit_adt_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[String],
    ) -> Option<(Frag, Ty)> {
        let start = self.off(c.syntax());
        Some(match self.resolve(segs)? {
            Resolved::Ctor(adt, v) => {
                let tys: Vec<Ty> = self.types.adts.defs[&adt].variants[v]
                    .fields
                    .iter()
                    .map(|f| f.ty.clone())
                    .collect();
                let mut args = self.emit_args(c.arg_list(), &tys);
                args.extend(self.const_args_of(c.syntax(), &adt));
                (
                    self.construct(&adt, v, args).at(start),
                    Ty::Adt(adt, vec![]),
                )
            }
            Resolved::Assoc(adt, m)
                if m == "from" && self.types.adts.method(&adt, &m).is_none() =>
            {
                let arg = c.arg_list().and_then(|l| l.args().next())?;
                let (f, ty) = self.emit_expr(&arg, None);
                let f = self.by_value(&arg, f, &ty);
                // Without a conversion the type checker has already reported the call.
                let conv = self
                    .conv_of(c.syntax())
                    .unwrap_or(crate::adt::Conv::Identity);
                (self.convert(conv, f).at(start), Ty::Adt(adt, vec![]))
            }
            Resolved::Assoc(adt, m) => {
                // (One of several impls of a generic trait: `Name::try_from(bytes)`.)
                let m = self
                    .types
                    .method_keys
                    .get(&self.range_of(c.syntax()))
                    .cloned()
                    .unwrap_or(m);
                let sig = self.types.adts.method(&adt, &m)?.clone();
                let cls = self.class(&adt);
                let js = prop(&m);
                let ret = if sig.is_async {
                    Ty::Future(Box::new(sig.ret.clone()))
                } else {
                    sig.ret.clone()
                };
                match sig.receiver {
                    None => {
                        let mut args = self.emit_args(c.arg_list(), &sig.params);
                        // The classes for `T::make()` in its body.
                        if let Some(tys) = self
                            .types
                            .dict_args
                            .get(&self.range_of(c.syntax()))
                            .cloned()
                        {
                            for t in &tys {
                                args.push(Frag::text(self.dict_of(t)));
                            }
                        }
                        args.extend(self.dbg_arg_frags(c.syntax()));
                        // A derived `default()` reads its const arguments and classes off a
                        // class view (`κσ.$cv(Parser, [κσ.$numT("u8")]).default()`).
                        let cls = match self.ty_of(c.syntax()) {
                            Some(t @ Ty::Adt(..))
                                if m == "default"
                                    && self.types.adts.defs[&adt].derives("Default")
                                    && !self.carried_names(&adt).is_empty() =>
                            {
                                self.dict_of(&t)
                            }
                            _ => cls,
                        };
                        let mut f = Frag::mapped(start, format!("{cls}.{js}("));
                        f.append(Frag::join(args, ", ")).push(")");
                        (f, ret)
                    }
                    Some(_) => {
                        // UFCS: the first argument is the receiver.
                        let all: Vec<ast::Expr> =
                            c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
                        let Some(first) = all.first() else {
                            return Some((Frag::text("undefined"), Ty::Unknown));
                        };
                        let (rf, _) = self.receiver(&strip_refs(first));
                        let rest: Vec<Frag> = all[1..]
                            .iter()
                            .zip(sig.params.iter())
                            .map(|(a, t)| {
                                let (f, ty) = self.emit_expr(a, Some(t));
                                self.by_value(a, f, &ty)
                            })
                            .collect();
                        let mut f = rf;
                        f.push(format!(".{js}("))
                            .append(Frag::join(rest, ", "))
                            .push(")");
                        (f.at(start), ret)
                    }
                }
            }
        })
    }

    pub(super) fn emit_adt_field(
        &mut self,
        rf: Frag,
        adt: &str,
        name: &ast::NameRef,
    ) -> (Frag, Ty) {
        let Some(def) = self.adt(adt) else {
            return (rf, Ty::Unknown);
        };
        let v = &def.variants[0];
        let key = name.text().to_string();
        let Some((i, field)) = v.field(&key) else {
            return (rf, Ty::Unknown);
        };
        let mut f = rf;
        f.append(Frag::mapped(self.off(name.syntax()), access(v, i)));
        (f, field.ty.clone())
    }

    /// Methods on a struct or enum value: `impl` methods and derived ones.
    pub(super) fn emit_adt_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: &Frag,
        adt: &str,
        method: &str,
    ) -> Option<(Frag, Ty)> {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        // (One of several impls of a generic trait: its key.)
        let picked = self
            .types
            .method_keys
            .get(&self.range_of(m.syntax()))
            .cloned();
        let method = picked.as_deref().unwrap_or(method);
        if let Some(sig) = self.types.adts.method(adt, method).cloned()
            && sig.receiver.is_some()
        {
            // `c.bump(&mut self.n)` with a method that can't keep the borrow: the place goes in
            // a write-back temp (`write_back_call`; the receiver is evaluated inside, so it
            // must be pure).
            if self
                .wb_methods
                .contains(&(adt.to_string(), method.to_string()))
                && m.receiver().is_some_and(|r| is_pure(&strip_refs(&r)))
                && let Some(places) = self.write_back_places(m.arg_list())
            {
                let mut callee = rf.clone();
                callee
                    .push(".")
                    .append(Frag::mapped(name_off, prop(method)));
                let f = self.write_back_call(m.syntax(), m.arg_list(), callee, &sig.params, places);
                let ret = if sig.is_async {
                    Ty::Future(Box::new(sig.ret))
                } else {
                    sig.ret
                };
                return Some((f, ret));
            }
            let mut args = self.emit_args(m.arg_list(), &sig.params);
            // The classes for the method's `T::make()` calls.
            if let Some(tys) = self
                .types
                .dict_args
                .get(&self.range_of(m.syntax()))
                .cloned()
            {
                for t in &tys {
                    args.push(Frag::text(self.dict_of(t)));
                }
            }
            args.extend(self.dbg_arg_frags(m.syntax()));
            let mut f = rf.clone();
            f.push(".")
                .append(Frag::mapped(name_off, prop(method)))
                .push("(");
            f.append(Frag::join(args, ", ")).push(")");
            let ret = if sig.is_async {
                Ty::Future(Box::new(sig.ret))
            } else {
                sig.ret
            };
            return Some((f, ret));
        }
        let arg = |cg: &mut Self| -> Frag {
            m.arg_list()
                .and_then(|a| a.args().next())
                .map(|a| cg.emit_expr(&a, None).0)
                .unwrap_or_else(|| Frag::text("undefined"))
        };
        let helper = |name: &str, other: Frag| {
            let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
            f.append(rf.clone()).push(", ").append(other).push(")");
            f
        };
        let t = Ty::Adt(adt.to_string(), vec![]);
        // (`Pin` is erased: `pinned.as_mut()` is the future itself.)
        if crate::typeck::pin_identity(method) && self.types.adts.future_outputs.contains_key(adt) {
            let t = self.ty_of(m.syntax()).unwrap_or(t);
            return Some((rf.clone(), t));
        }
        Some(match method {
            // Without `#[derive(Clone)]` the value must be an `Rc` (erased): clones share it.
            "clone" if self.adt(adt).is_some_and(|d| !d.derives("Clone")) => (rf.clone(), t),
            "clone" | "to_owned" => {
                let mut f = rf.clone();
                f.push(".").append(Frag::mapped(name_off, "$clone()"));
                (f, t)
            }
            "eq" => (helper("eq", arg(self)), Ty::Bool),
            "ne" => (helper("eq", arg(self)).wrap("!", ""), Ty::Bool),
            "cmp" => (helper("cmp", arg(self)), Ty::Ordering),
            "partial_cmp" => (
                helper("cmp", arg(self)).wrap("κσ.$ordOpt(", ")"),
                Ty::Option(Box::new(Ty::Ordering)),
            ),
            "max" => (helper("max", arg(self)), t),
            "min" => (helper("min", arg(self)), t),
            "lt" => (helper("cmp", arg(self)).wrap("(", " < 0)"), Ty::Bool),
            "le" => (helper("cmp", arg(self)).wrap("(", " <= 0)"), Ty::Bool),
            "gt" => (helper("cmp", arg(self)).wrap("(", " > 0)"), Ty::Bool),
            "ge" => (helper("cmp", arg(self)).wrap("(", " >= 0)"), Ty::Bool),
            "to_string" => {
                let mut f = Frag::mapped(name_off, "κσ.$display(");
                f.append(rf.clone()).push(")");
                (f, Ty::Str)
            }
            _ => return None,
        })
    }

    /// `shape as i32` for an enum without data.
    pub(super) fn enum_discriminant(&self, f: Frag, adt: &str) -> Frag {
        let Some(def) = self.types.adts.defs.get(adt) else {
            return f;
        };
        let identity = def
            .variants
            .iter()
            .enumerate()
            .all(|(i, v)| v.discr == i as i64);
        if identity {
            f.wrap("", ".$tag")
        } else {
            let ds: Vec<String> = def.variants.iter().map(|v| v.discr.to_string()).collect();
            Frag::text(format!("[{}][", ds.join(", ")))
                .append(f)
                .push(".$tag]")
                .clone()
        }
    }

    // ------------------------------------------------------------ patterns

    /// Struct, tuple-struct, unit and variant patterns. Returns false when `p` isn't one.
    pub(super) fn lower_adt_pat(&mut self, p: &ast::Pat, subject: &str, out: &mut PatCode) -> bool {
        let segs = match p {
            ast::Pat::RecordPat(rp) => path_segments(rp.path()),
            ast::Pat::TupleStructPat(tp) => path_segments(tp.path()),
            ast::Pat::PathPat(pp) => path_segments(pp.path()),
            ast::Pat::IdentPat(ip) => match self.unit_struct_pat(ip) {
                Some(n) => vec![n],
                None => return false,
            },
            _ => return false,
        };
        let Some(Resolved::Ctor(adt, v)) = self.resolve(&segs) else {
            return false;
        };
        let def = self.types.adts.defs[&adt].clone();
        let variant = &def.variants[v];
        if def.is_enum {
            out.conds
                .push(Frag::text(format!("{subject}.$tag === {v}")));
        }
        match p {
            ast::Pat::TupleStructPat(tp) => {
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let rest = fields
                    .iter()
                    .position(|f| matches!(f, ast::Pat::RestPat(_)));
                let n = variant.fields.len();
                for (i, f) in fields.iter().enumerate() {
                    let index = match rest {
                        Some(r) if i == r => continue,
                        Some(r) if i > r => n.saturating_sub(fields.len() - i),
                        _ => i,
                    };
                    if index < n {
                        self.lower_pat(f, &format!("{subject}{}", access(variant, index)), out);
                    }
                }
            }
            ast::Pat::RecordPat(rp) => {
                for f in rp
                    .record_pat_field_list()
                    .into_iter()
                    .flat_map(|l| l.fields())
                {
                    let (Some(fname), Some(sub)) = (f.field_name(), f.pat()) else {
                        continue;
                    };
                    if let Some((i, _)) = variant.field(fname.text().to_string().as_str()) {
                        self.lower_pat(&sub, &format!("{subject}{}", access(variant, i)), out);
                    }
                }
            }
            _ => {}
        }
        true
    }

    /// A bare identifier pattern naming a unit struct matches it instead of binding.
    pub(super) fn unit_struct_pat(&self, ip: &ast::IdentPat) -> Option<String> {
        if ip.pat().is_some() || ip.mut_token().is_some() || ip.ref_token().is_some() {
            return None;
        }
        let n = ip.name()?.text().to_string();
        match self.resolve(std::slice::from_ref(&n))? {
            Resolved::Ctor(adt, 0)
                if self.types.adts.defs[&adt].variants[0].shape == Shape::Unit =>
            {
                Some(adt)
            }
            _ => None,
        }
    }

    /// Whether values of `ty` are copied when read from a place (tuples, `Copy` structs).
    pub(super) fn copied_on_read(&self, ty: &Ty) -> bool {
        match ty {
            // A generic value may be a tuple or `Copy` struct; `κσ.$clone` copies those and leaves
            // numbers and strings alone.
            Ty::Tuple(_) | Ty::Param(_) => true,
            Ty::Adt(n, _) => self
                .types
                .adts
                .defs
                .get(n)
                .is_some_and(|d| d.derives("Copy") && !(d.is_enum && !d.has_data())),
            _ => false,
        }
    }
}

/// A derived `$cmp` over fields: the first that isn't equal decides. A NaN (incomparable) stops
/// the chain too, so `partial_cmp` gives `None` and every `<`/`>` is false, like Rust's.
fn partial_chain(parts: &[String]) -> String {
    let mut out = String::from("{ let $c; return ");
    for p in parts {
        out.push_str(&format!("($c = {p}) !== 0 ? $c : "));
    }
    out.push_str("0; }");
    out
}
