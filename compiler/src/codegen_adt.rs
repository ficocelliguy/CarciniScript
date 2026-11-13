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
    ast::{self, HasArgList, HasName},
};

use super::{
    Codegen, Dest, Frag, Local, PatCode, debug_fn, default_value, escape_ident, is_pure, member,
    strip_refs,
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
        _ => member(&prop(&v.fields[i].name)),
    }
}

/// The segment names of a path (`Shape::Circle` → `["Shape", "Circle"]`).
fn path_segments(path: Option<ast::Path>) -> Vec<String> {
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

fn dbg_of(ty: &Ty, e: &str) -> String {
    match debug_fn(ty) {
        Some(f) => format!("{f}({e})"),
        None => format!("String({e})"),
    }
}

impl Codegen<'_> {
    fn adt(&self, name: &str) -> Option<AdtDef> {
        self.types.adts.defs.get(name).cloned()
    }

    pub(super) fn resolve(&self, segs: &[String]) -> Option<Resolved> {
        self.types.adts.resolve(self.self_adt.as_deref(), segs)
    }

    /// The JS class for a struct or enum (`module.Name` when imported).
    pub(super) fn class(&self, adt: &str) -> String {
        match self.types.adts.defs.get(adt).and_then(|d| d.module.clone()) {
            Some(m) => format!("{m}.{}", escape_ident(adt)),
            None => escape_ident(adt),
        }
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

        // Constructor and variant factories.
        if def.is_enum {
            self.line(Frag::text("constructor($tag) {"));
            self.line(Frag::text("  this.$tag = $tag;"));
            self.line(Frag::text("}"));
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
                    params.join(", ")
                )));
                self.line(Frag::text(format!("  const $v = new {cls}({i});")));
                for (k, p) in params.iter().enumerate() {
                    self.line(Frag::text(format!("  $v{} = {p};", access(v, k))));
                }
                self.line(Frag::text("  return $v;"));
                self.line(Frag::text("}"));
            }
        } else {
            let v = &def.variants[0];
            if !v.fields.is_empty() {
                let params: Vec<String> = (0..v.fields.len()).map(|k| format!("${k}")).collect();
                self.line(Frag::text(format!("constructor({}) {{", params.join(", "))));
                for (k, p) in params.iter().enumerate() {
                    self.line(Frag::text(format!("  this{} = {p};", access(v, k))));
                }
                self.line(Frag::text("}"));
            }
        }

        // `impl` blocks.
        let saved = self.self_adt.replace(def.name.clone());
        for imp in impls {
            for item in imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                if let ast::AssocItem::Fn(f) = item {
                    self.emit_class_method(&f, &def);
                }
            }
        }
        self.self_adt = saved;

        self.emit_derives(&def, &cls);
        self.out.indent -= 1;
        self.line(Frag::text("}"));
        self.line(Frag::default());
    }

    fn emit_class_method(&mut self, f: &ast::Fn, def: &AdtDef) {
        let Some(name) = f.name() else { return };
        let Some(sig) = self
            .types
            .adts
            .method(&def.name, name.text().to_string().as_str())
            .cloned()
        else {
            return;
        };
        self.emit_method_fn(f, sig, Ty::Adt(def.name.clone()), "}");
    }

    /// A method in JS method syntax (`[static] [async] name(params) { .. }`), for a class body or
    /// an object literal (`close` is then `},`). `self_ty` is the type `self` has.
    fn emit_method_fn(&mut self, f: &ast::Fn, sig: crate::adt::Method, self_ty: Ty, close: &str) {
        let Some(name) = f.name() else { return };
        let saved_tmp = self.tmp;
        let saved_names = self.used_names.clone();
        self.tmp = 0;
        self.ret_ty = sig.ret.clone();
        self.in_async = sig.is_async;
        self.scopes.push(HashMap::new());
        if sig.receiver.is_some() {
            self.declare(
                "self",
                None,
                Local {
                    js: "this".into(),
                    ty: self_ty,
                },
            );
        }
        let mut params = Vec::new();
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
                        self.declare(
                            n.text().to_string().as_str(),
                            Some(n.syntax()),
                            Local { js: access, ty },
                        );
                        params.push(Frag::mapped(self.off(n.syntax()), js));
                    }
                    Some(ast::Pat::WildcardPat(_)) => params.push(Frag::text(self.temp())),
                    Some(other) => self.unsupported(other.syntax(), "destructuring parameters", 1),
                    None => {}
                }
            }
        }
        let mut header = Frag::mapped(self.item_start(f.syntax()), "");
        if sig.receiver.is_none() {
            header.push("static ");
        }
        if sig.is_async {
            header.push("async ");
        }
        header
            .append(Frag::mapped(
                self.off(name.syntax()),
                prop(name.text().to_string().as_str()),
            ))
            .push("(")
            .append(Frag::join(params, ", "))
            .push(") {");
        self.line(header);
        self.out.indent += 1;
        if let Some(body) = f.body() {
            self.emit_block_body(&body, &Dest::Return, None);
        }
        self.out.indent -= 1;
        self.close_line(close, f.body().as_ref());
        self.scopes.pop();
        self.tmp = saved_tmp;
        self.used_names = saved_names;
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
        for item in t
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
        {
            let ast::AssocItem::Fn(f) = item else {
                continue;
            };
            if f.body().is_none() {
                continue;
            }
            let Some(n) = f.name() else { continue };
            let Some(tm) = def.methods.iter().find(|m| m.name == n.text()) else {
                continue;
            };
            self.emit_method_fn(&f, tm.sig.clone(), Ty::Param("Self".into()), "},");
        }
        self.out.indent -= 1;
        self.line(Frag::text("};"));
        self.line(Frag::default());
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
                let defaults = match self
                    .types
                    .adts
                    .traits
                    .get(&tr)
                    .and_then(|d| d.module.clone())
                {
                    Some(m) => format!("{m}.{}$defaults", escape_ident(&tr)),
                    None => format!("{}$defaults", escape_ident(&tr)),
                };
                let cls = self.class(&adt);
                self.line(Frag::text(format!("κσ.$mixin({cls}, {defaults});")));
            }
        }
    }

    /// `$clone`, `$eq`, `$cmp`, `$dbg` and `static default()` for the derives.
    fn emit_derives(&mut self, def: &AdtDef, cls: &str) {
        let lines = |cg: &mut Self, ls: Vec<String>| {
            for l in ls {
                cg.line(Frag::text(l));
            }
        };
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
        if def.derives("Clone") {
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
                                format!("return {cls}.{}({});", v.name, fields.join(", "))
                            }
                        } else {
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
                        format!("return {} || 0;", parts.join(" || "))
                    }
                },
                "return 0;",
            ));
            ls.push("}".into());
            lines(self, ls);
        }
        if def.derives("Debug") {
            let mut ls = vec!["$dbg() {".to_string()];
            ls.extend(per_variant(
                def,
                &|_, v| {
                    let vals: Vec<String> = v
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(k, f)| dbg_of(&f.ty, &format!("this{}", access(v, k))))
                        .collect();
                    match v.shape {
                        Shape::Unit => format!("return {};", super::js_string(&v.name)),
                        Shape::Tuple => format!(
                            "return κσ.$dbgStruct({}, null, [{}]);",
                            super::js_string(&v.name),
                            vals.join(", ")
                        ),
                        Shape::Named => {
                            let names: Vec<String> =
                                v.fields.iter().map(|f| super::js_string(&f.name)).collect();
                            format!(
                                "return κσ.$dbgStruct({}, [{}], [{}]);",
                                super::js_string(&v.name),
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
                match def.default_variant {
                    Some(i) => format!("return {cls}.{};", def.variants[i].name),
                    None => "return undefined;".into(),
                }
            } else {
                let v = &def.variants[0];
                let vals: Vec<String> = v
                    .fields
                    .iter()
                    .map(|f| default_value(&f.ty).unwrap_or_else(|| "undefined".into()))
                    .collect();
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
    fn construct(&self, adt: &str, v: usize, args: Vec<Frag>) -> Frag {
        let def = &self.types.adts.defs[adt];
        let cls = self.class(adt);
        let variant = &def.variants[v];
        if def.is_enum {
            if variant.shape == Shape::Unit {
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
            let args = (0..variant.fields.len())
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
            return (self.construct(&adt, v, args).at(start), Ty::Adt(adt));
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
        let args = (0..variant.fields.len())
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
        f.append(self.construct(&adt, v, args))
            .push(")(")
            .append(Frag::join(call_args, ", "))
            .push(")");
        (f.at(start), Ty::Adt(adt))
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
                    (Shape::Unit, true) => {
                        (Frag::text(format!("{cls}.{}", variant.name)), Ty::Adt(adt))
                    }
                    (Shape::Unit, false) => (Frag::text(format!("new {cls}()")), Ty::Adt(adt)),
                    (_, true) => (Frag::text(format!("{cls}.{}", variant.name)), Ty::Unknown),
                    (_, false) => (
                        Frag::text(format!("((...$a) => new {cls}(...$a))")),
                        Ty::Unknown,
                    ),
                }
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
                let args = self.emit_args(c.arg_list(), &tys);
                (self.construct(&adt, v, args).at(start), Ty::Adt(adt))
            }
            Resolved::Assoc(adt, m) => {
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
                        let args = self.emit_args(c.arg_list(), &sig.params);
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
        if let Some(sig) = self.types.adts.method(adt, method).cloned()
            && sig.receiver.is_some()
        {
            let args = self.emit_args(m.arg_list(), &sig.params);
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
        let t = Ty::Adt(adt.to_string());
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
            "partial_cmp" => (helper("cmp", arg(self)), Ty::Option(Box::new(Ty::Ordering))),
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
            Ty::Adt(n) => self
                .types
                .adts
                .defs
                .get(n)
                .is_some_and(|d| d.derives("Copy") && !(d.is_enum && !d.has_data())),
            _ => false,
        }
    }
}
