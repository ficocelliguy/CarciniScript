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

/// `clone()` of a value of type `ty`, with `dyn_clones` from `dyn_clones`.
fn clone_of(dyn_clones: &[(Ty, String)], ty: &Ty, e: &str) -> String {
    if ty.is_js_primitive() || matches!(ty, Ty::Unit) {
        e.to_string()
    } else if let Some((_, c)) = dyn_clones.iter().find(|(t, _)| t == ty) {
        format!("{c}.call({e})")
    } else {
        format!("κσ.$clone({e})")
    }
}

/// The `clone` of `impl Clone for Box<dyn Tr>` when `ty` is that `dyn Tr` (it typically calls
/// `self.clone_box()`), as a JS function taking the value as `this`.
pub(super) fn dyn_clone(adts: &crate::adt::Adts, ty: &Ty) -> Option<String> {
    let Ty::Dyn(tr, _) = ty else {
        return None;
    };
    let key = format!("dyn${tr}");
    adts.method(&key, "clone")?;
    Some(format!("$ext${key}.prototype.clone"))
}

/// Each `dyn Tr` with an `impl Clone for Box<dyn Tr>`, and that `clone` (`dyn_clone`).
fn dyn_clones(adts: &crate::adt::Adts) -> Vec<(Ty, String)> {
    adts.ext_types
        .iter()
        .filter_map(|k| {
            let t = Ty::Dyn(k.strip_prefix("dyn$")?.to_string(), Vec::new());
            Some((t.clone(), dyn_clone(adts, &t)?))
        })
        .collect()
}

fn eq_of(adts: &crate::adt::Adts, ty: &Ty, a: &str, b: &str) -> String {
    if ty.is_js_primitive() || matches!(ty, Ty::Unit) {
        format!("{a} === {b}")
    } else if let Some(f) = dyn_eq_fn(adts, ty) {
        format!("{f}({a}, {b})")
    } else {
        format!("κσ.$eq({a}, {b})")
    }
}

/// `==` on a `ty` that is or holds a `dyn Tr` with `impl PartialEq for Box<dyn Tr>`: a JS
/// function of the two sides calling that `eq` (the boxed values' own `$eq` would compare them
/// field by field); `None` when `ty` holds no such `dyn`.
pub(super) fn dyn_eq_fn(adts: &crate::adt::Adts, ty: &Ty) -> Option<String> {
    match ty {
        Ty::Dyn(tr, _) => {
            let key = format!("dyn${tr}");
            adts.method(&key, "eq")?;
            Some(format!("((a, b) => $ext${key}.prototype.eq.call(a, b))"))
        }
        Ty::Vec(e) => Some(format!("((a, b) => κσ.$eqBy(a, b, {}))", dyn_eq_fn(adts, e)?)),
        Ty::Option(e) => Some(format!("((a, b) => κσ.$eqOptBy(a, b, {}))", dyn_eq_fn(adts, e)?)),
        _ => None,
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
        k if let Some(r) = crate::ty::RangeKind::from_name(k) => {
            Ty::Range(Box::new(Ty::Unknown), r)
        }
        "Option" => Ty::Option(Box::new(Ty::Unknown)),
        "bool" => Ty::Bool,
        "char" => Ty::Char,
        k if let Some(tr) = k.strip_prefix("dyn$") => Ty::Dyn(tr.to_string(), Vec::new()),
        k => Ty::from_name(k),
    }
}

/// A `&mut self` method of a trait implemented for a string, number, bool, char or `Option`: JS
/// can't change those through `this`, so `this` is a cell (`this[0]` is `self`).
/// So is one on an immutable enum (`Adts::is_atom`).
pub(super) fn cell_self(
    sig: &crate::adt::Method,
    self_ty: &Ty,
    adts: &crate::adt::Adts,
) -> bool {
    sig.receiver == Some(crate::adt::Receiver::MutRef)
        && (crate::typeck::ptr_cellable(self_ty) || adts.atom_ty(self_ty) || sig.cell_self)
}

impl Codegen<'_> {
    /// `cell_self` for a method stored under `key` (a struct or enum, or a built-in type's
    /// `ext_self_ty`).
    pub(super) fn key_cell_self(&self, sig: &crate::adt::Method, key: &str) -> bool {
        let adts = &self.types.adts;
        let self_ty = if adts.defs.contains_key(key) {
            Ty::Adt(key.to_string(), Vec::new())
        } else {
            ext_self_ty(key)
        };
        cell_self(sig, &self_ty, adts)
    }

    /// `s.read(&mut buf)` on `s: &[u8]`: the io prelude's `impl IoRead for Vec<u8>` (see the
    /// checker's `slice_read`).
    pub(super) fn slice_read(&self, key: &str, method: &str, rt: &Ty) -> bool {
        key == "Vec"
            && matches!(rt, Ty::Vec(e) if matches!(**e, Ty::Int(crate::ty::IntKind::U8)))
            && (self.types.adts.traits.get("IoRead"))
                .is_some_and(|t| t.methods.iter().any(|m| m.name == method))
    }

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
            Some(m) => {
                let js = self.types.adts.js_names.get(adt).map_or(adt, String::as_str);
                format!("{}.{}", super::module_js(&m), escape_ident(js))
            }
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
            let atom = self.types.adts.is_atom(&def.name);
            // (Its `&mut self` methods, which a generic call (`κσ.$dcallMut`) hands the cell.)
            let muts: Vec<String> = match self.types.adts.methods.get(&def.name) {
                Some(ms) if atom => {
                    let mut v: Vec<String> = ms
                        .iter()
                        .filter(|(_, s)| s.receiver == Some(crate::adt::Receiver::MutRef))
                        .map(|(n, _)| super::js_string(&prop(n)))
                        .collect();
                    v.sort();
                    v
                }
                _ => Vec::new(),
            };
            if !muts.is_empty() {
                self.line(Frag::text(format!(
                    "static $mut = new Set([{}]);",
                    muts.join(", ")
                )));
            }
            for (i, v) in def.variants.iter().enumerate() {
                // One shared instance of an immutable enum (`Adts::is_atom`), frozen so a write
                // that misses its cell fails loudly.
                if atom {
                    self.line(Frag::text(format!(
                        "static {} = Object.freeze(new {cls}({i}));",
                        v.name
                    )));
                    continue;
                }
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
                    let js = |n: &ast::Name| format!("{tr}${}", n.text());
                    if let ast::AssocItem::Fn(f) = item
                        && let Some(n) = f.name()
                        && let key = self.types.adts.method_key(
                            &def.name,
                            n.text().to_string().as_str(),
                            self.off(n.syntax()),
                        )
                        // (Stored as `Trait$method` already: emitted under that name below.)
                        && key != js(&n)
                        // (Impls the trait arguments pick share the dispatcher instead.)
                        && !self.types.adts.picked_by_trait_args(&tr, n.text().as_ref())
                        && let Some(sig) = self.types.adts.method(&def.name, &key).cloned()
                    {
                        let js = js(&n);
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
                        let name = escape_ident(n.text());
                        // `impl<T: Zero> Zero for W<T> { const ZERO: Self = W(T::ZERO); }`: a
                        // getter reading `T`'s class off the class view it's read through
                        // (`κσ.$tv(W, [cls]).ZERO`, see `tparam_consts`).
                        if let Some(read) = self
                            .tparam_consts
                            .get(&(def.name.clone(), n.text().to_string()))
                            .cloned()
                        {
                            let saved_consts = super::set_const_scope(None);
                            for (p, i) in &read {
                                super::CONST_SCOPE.with(|s| {
                                    s.borrow_mut().insert(p.clone(), format!("this.$targs?.[{i}]"))
                                });
                            }
                            let (f, _) = self.emit_expr(&body, ty.as_ref());
                            super::set_const_scope(Some(saved_consts));
                            let mut line = Frag::mapped(
                                self.off(k.syntax()),
                                format!("static get {name}() {{ return "),
                            );
                            line.append(f).push("; }");
                            self.line(line);
                            continue;
                        }
                        // `impl<const P: u128> U<P> { const PER: u128 = P * 1000; }`: a getter
                        // reading `P` off the class view it's read through (`κσ.$cv(U, [7]).PER`,
                        // see `param_consts`).
                        if self
                            .param_consts
                            .contains(&(def.name.clone(), n.text().to_string()))
                        {
                            use ra_ap_syntax::ast::HasGenericArgs;
                            let st_args: Vec<String> = match imp.self_ty() {
                                Some(ast::Type::PathType(p)) => p
                                    .path()
                                    .and_then(|p| p.segment())
                                    .and_then(|s| s.generic_arg_list())
                                    .into_iter()
                                    .flat_map(|l| l.generic_args())
                                    .map(|a| a.syntax().text().to_string())
                                    .collect(),
                                _ => Vec::new(),
                            };
                            let saved_consts = super::set_const_scope(None);
                            self.scopes.push(HashMap::new());
                            for p in imp
                                .generic_param_list()
                                .into_iter()
                                .flat_map(|l| l.generic_params())
                            {
                                if let ast::GenericParam::ConstParam(cp) = p
                                    && let Some(pn) = cp.name()
                                    && let Some(i) =
                                        st_args.iter().position(|a| *a == pn.text().to_string())
                                {
                                    let js = format!("this.$cargs?.[{i}]");
                                    super::CONST_SCOPE
                                        .with(|s| s.borrow_mut().insert(pn.text().to_string(), js.clone()));
                                    let pty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                                    // (A 128-bit parameter is a BigInt in the body.)
                                    let js = match pty {
                                        Ty::Int(k) if k.is_big() => format!("BigInt({js})"),
                                        _ => js,
                                    };
                                    self.declare(pn.text().to_string().as_str(), None, Local { js, ty: pty });
                                }
                            }
                            let (f, _) = self.emit_expr(&body, ty.as_ref());
                            self.scopes.pop();
                            super::set_const_scope(Some(saved_consts));
                            let mut line = Frag::mapped(
                                self.off(k.syntax()),
                                format!("static get {name}() {{ return "),
                            );
                            line.append(f).push("; }");
                            self.line(line);
                            continue;
                        }
                        let (f, _) = self.emit_expr(&body, ty.as_ref());
                        // A value naming other items (`W * 2` with `const W` below the class,
                        // another class's const) is computed on first read: they may not be
                        // initialized yet when the class is.
                        let lazy = body
                            .syntax()
                            .descendants()
                            .filter_map(ast::PathExpr::cast)
                            .any(|p| {
                                p.path()
                                    .and_then(|p| p.first_segment())
                                    .is_none_or(|s| s.syntax().text() != "Self")
                            });
                        let name = escape_ident(n.text());
                        let mut line = Frag::mapped(
                            self.off(k.syntax()),
                            if lazy {
                                format!("static get {name}() {{ const v = ")
                            } else {
                                format!("static {name} = ")
                            },
                        );
                        line.append(f);
                        if lazy {
                            line.push(format!(
                                "; Object.defineProperty(this, \"{name}\", {{ value: v }}); return v; }}"
                            ));
                        } else {
                            line.push(";");
                        }
                        self.line(line);
                    }
                    // `type Error = MyError;` read as a class by a trait's default method
                    // (`Self::Error::custom(..)`): `this.constructor.$at$Error`.
                    ast::AssocItem::TypeAlias(ta) => {
                        let (Some(n), Some(t)) = (ta.name(), ta.ty()) else {
                            continue;
                        };
                        if !self.types.self_assoc_dicts.contains(&n.text().to_string()) {
                            continue;
                        }
                        let ty = self.lower_type(&t);
                        let mut d = self.dict_of(&ty);
                        // (`type Seed = [u8; 4];`: its `default()` has four elements.)
                        if let ast::Type::ArrayType(a) = &t
                            && let Some(ast::Expr::Literal(l)) = a.const_arg().and_then(|c| c.expr())
                            && let ast::LiteralKind::IntNumber(n) = l.kind()
                            && let Ok(n) = n.value()
                            && let Ty::Vec(e) = &ty
                        {
                            d = format!("κσ.$arrT({n}, {})", self.dict_of(e));
                        }
                        self.line(Frag::mapped(
                            self.off(ta.syntax()),
                            format!("static get $at${}() {{ return {d}; }}", n.text()),
                        ));
                    }
                    _ => {}
                }
            }
        }
        self.self_adt = saved;

        self.emit_derives(&def, &cls, node);
        self.out.indent -= 1;
        self.line(Frag::text("}"));
        self.emit_alt_dispatch(&def.name, &cls);
        // (A file's own `Duration`, renamed `Duration__file` by `ownstd.rs`: the files that
        // import it ask for `Duration`.)
        if let Some(orig) = def.name.strip_suffix("__file")
            && self.display_names.get(&def.name).is_some_and(|o| o == orig)
        {
            self.line(Frag::text(format!("export {{ {cls} as {orig} }};")));
        }
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
    /// The methods that several impls of one generic trait give type `key`, where the impls'
    /// arguments tell them apart (`Adts::alt_dispatch`): the method's own name becomes a
    /// dispatcher to the impl whose parameter types the arguments fit (`κσ.$altDispatch`), so
    /// a call through a type parameter or `dyn` reaches the right one.
    fn emit_alt_dispatch(&mut self, key: &str, cls: &str) {
        let mut names: Vec<String> = self
            .types
            .adts
            .method_alts
            .keys()
            .filter_map(|k| k.strip_prefix(&format!("{key}::")).map(str::to_string))
            .collect();
        names.sort();
        for m in names {
            // (Through a bound the trait arguments' classes come too, which tell apart the impls
            // whose arguments look alike: `Scale<f64>` and `Scale<i32>`.)
            let by_class = self.types.adts.alt_trait_args(key, &m);
            let sigs: Vec<(String, Vec<Ty>, Vec<Ty>)> =
                match (self.types.adts.alt_dispatch(key, &m), by_class) {
                    (_, Some(s)) => s,
                    (Some(s), None) => s.into_iter().map(|(k, ps)| (k, ps, Vec::new())).collect(),
                    (None, None) => continue,
                };
            let statik = self
                .types
                .adts
                .method(key, &m)
                .is_some_and(|s| s.receiver.is_none());
            let class = |p: &Ty| {
                let mut generic = false;
                p.walk(&mut |t| generic |= matches!(t, Ty::Param(_)));
                match self.dict_of(p) {
                    d if generic || d == "undefined" => "null".to_string(),
                    d => d,
                }
            };
            // (Those classes follow the arguments and the classes of the method's own type
            // parameters that the call passes: `Tr::m`'s.)
            let own = self
                .types
                .adts
                .traits
                .iter()
                .filter(|(_, d)| !d.type_params.is_empty() && d.methods.iter().any(|t| t.name == m))
                .find_map(|(n, _)| self.types.dict_params.get(&format!("{n}::{m}")))
                .map_or(0, |l| l.iter().filter(|x| !x.contains("::")).count());
            let entries: Vec<String> = sigs
                .iter()
                .map(|(k, ps, ts)| {
                    let ds: Vec<String> = ps.iter().map(class).collect();
                    let ts: Vec<String> = ts.iter().map(class).collect();
                    format!(
                        "[{}, [{}], [{}], {own}]",
                        super::js_string(&prop(k)),
                        ds.join(", "),
                        ts.join(", ")
                    )
                })
                .collect();
            let target = if statik {
                cls.to_string()
            } else {
                format!("{cls}.prototype")
            };
            self.line(Frag::text(format!(
                "κσ.$altDispatch({target}, {}, () => [{}]);",
                super::js_string(&prop(&m)),
                entries.join(", ")
            )));
        }
        // `Y::from(x)` with `x: T` in a function `where Y: From<T>`: a `from` that picks among
        // the `From` impls (`from$0`, `from$1`, ..) by the value.
        let froms = self.types.adts.from_impls.get(key).cloned().unwrap_or_default();
        if !froms.is_empty()
            && froms.iter().all(|f| f.key != "from")
            && self.types.adts.method(key, "from").is_none()
        {
            let entries: Vec<String> = froms
                .iter()
                .map(|f| {
                    let mut generic = f.via_into || f.via_as_ref;
                    f.src.walk(&mut |t| generic |= matches!(t, Ty::Param(_)));
                    let d = match self.dict_of(&f.src) {
                        d if generic || d == "undefined" => "null".to_string(),
                        d => d,
                    };
                    format!("[{}, [{d}]]", super::js_string(&prop(&f.key)))
                })
                .collect();
            self.line(Frag::text(format!(
                "κσ.$altDispatch({cls}, \"from\", () => [{}]);",
                entries.join(", ")
            )));
        }
    }

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
            // (A trait's default method: its own type parameters' classes, as `Tr::m`'s.)
            Ty::Param(p) if p == "Self" && self.in_trait_default => {
                Some(format!("{}::{}", self.default_trait, name.text()))
            }
            // (An impl for a built-in type, `impl<T: Decode> Decode for Vec<T>`: `Vec::decode`.)
            _ if !self.in_trait_default => self.ext_class.as_ref().map(|k| {
                format!(
                    "{k}::{}",
                    js_name.clone().unwrap_or_else(|| name.text().to_string())
                )
            }),
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
        // `fn bump(mut self)` on a `Copy` type: changes to `self` mustn't reach the caller's
        // value, so the method works on a copy.
        let copy_self = matches!(sig.receiver, Some(crate::adt::Receiver::Value))
            && f.param_list()
                .and_then(|pl| pl.self_param())
                .is_some_and(|sp| sp.mut_token().is_some() && sp.amp_token().is_none())
            && matches!(self_ty, Ty::Adt(..))
            && (self.copied_on_read(&self_ty) || self.types.adts.atom_ty(&self_ty));
        if sig.receiver.is_some() {
            // (A `&mut self` method on a string or number: `this` is a cell.)
            let js = if cell_self(&sig, &self_ty, &self.types.adts) {
                "this[0]"
            } else if copy_self {
                "$self"
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
        // A blanket impl's associated function: its `T`'s class is `this`.
        let blanket_this = match &self_ty {
            Ty::Param(p) if p != "Self" && sig.receiver.is_none() => Some(p.clone()),
            _ => None,
        };
        let saved_blanket = std::mem::replace(&mut self.blanket_this, blanket_this);
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
                if let Some(a) = sig.impl_args.get(i)
                    && let Some(n) = crate::typeck::class_param(a)
                    && let Some(x) = struct_params.get(i)
                {
                    // (`impl<T> S<Vec<T>>`: on the `Vec`'s class.)
                    let js = match a {
                        Ty::Param(_) => format!("this.${x}"),
                        _ => format!("this.${x}?.elem"),
                    };
                    super::CONST_SCOPE.with(|s| s.borrow_mut().insert(n.to_string(), js));
                }
            }
        }
        let mut const_lines = self.enter_consts(f, outer);
        const_lines.extend(view_lines);
        // `impl<T> Tr for Option<T>` (or `Vec<T>`): `T`'s class, handed over by a caller that
        // knows it; `impl<A, B> Tr for (A, B)`: the elements' classes, in an array.
        let elem_key = match &self_ty {
            Ty::Option(_) => Some("Option"),
            Ty::Vec(_) => Some("Vec"),
            Ty::RefCell(_) => Some("RefCell"),
            Ty::Tuple(es) => crate::adt::tuple_key(es.len()),
            // (A `&T` impl's methods: `T`'s.)
            Ty::Param(_) if self.ext_class.as_deref().is_some_and(crate::adt::is_wrapper_key) => {
                self.ext_class.as_deref()
            }
            _ => None,
        };
        if let Some(key) = elem_key
            && self.types.elem_methods.contains(&format!(
                "{key}::{}",
                js_name.clone().unwrap_or_else(|| name.text().to_string())
            ))
        {
            let tuple = matches!(self_ty, Ty::Tuple(_));
            for (i, a) in sig.impl_args.iter().enumerate() {
                if let Ty::Param(p) = a {
                    let js = if tuple { format!("$elem?.[{i}]") } else { "$elem".into() };
                    super::CONST_SCOPE.with(|s| s.borrow_mut().insert(p.clone(), js));
                }
            }
            const_lines.insert(0, "const $elem = κσ.$takeElem();".into());
        }
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
        if copy_self {
            self.line(Frag::text("let $self = this.$clone();"));
        }
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
        self.blanket_this = saved_blanket;
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
            // (A `&T` impl's `self` is its `T`.)
            k if crate::adt::is_wrapper_key(k) => Ty::Param(
                impls.first().and_then(crate::adt::wrapper_impl).map(|(_, t)| t).unwrap_or_default(),
            ),
            "String" => Ty::Str,
            "Vec" => Ty::Vec(Box::new(Ty::Unknown)),
            "Option" => Ty::Option(Box::new(Ty::Unknown)),
            "Result" => Ty::Result(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            "HashMap" => Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            "HashSet" => Ty::Set(Box::new(Ty::Unknown)),
            "RefCell" => Ty::RefCell(Box::new(Ty::Unknown)),
            "BTreeMap" => Ty::Coll(
                crate::ty::Coll::BTreeMap,
                Box::new(Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))),
            ),
            "BTreeSet" => Ty::Coll(
                crate::ty::Coll::BTreeSet,
                Box::new(Ty::Set(Box::new(Ty::Unknown))),
            ),
            "VecDeque" => ext_self_ty(key),
            k if crate::ty::RangeKind::from_name(k).is_some() => ext_self_ty(k),
            "bool" => Ty::Bool,
            "char" => Ty::Char,
            k if let Some(tr) = k.strip_prefix("dyn$") => Ty::Dyn(tr.to_string(), Vec::new()),
            k => Ty::from_name(k),
        };
        // (`Self::make()` in its methods is the class's own function.)
        let saved_ext = std::mem::replace(
            &mut self.ext_class,
            (!key.ends_with("$all")).then(|| key.to_string()),
        );
        self.line(Frag::text(format!("class $ext${key} {{")));
        self.out.indent += 1;
        let mut given: Vec<String> = Vec::new();
        let mut given_consts: Vec<ast::Const> = Vec::new();
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
                // `impl Debug for Box<dyn Tr>`'s `fmt`, as `fmt$dbg` (next to `Display`'s).
                if let ast::AssocItem::Fn(f) = &item
                    && let Some(n) = f.name()
                    && self.types.adts.debug_fmt.get(key) == Some(&self.off(n.syntax()))
                    && let Some(sig) = self.types.adts.method(key, "fmt$dbg").cloned()
                {
                    self.emit_method_fn(f, sig, self_ty.clone(), "}", Some("fmt$dbg".into()));
                    continue;
                }
                // `const ZERO: u32 = 0;`: `$ext$u32.ZERO`, which `T::ZERO` reads off the class
                // passed for `T`.
                if let ast::AssocItem::Const(k) = &item {
                    given_consts.push(k.clone());
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
        // (And the trait's default consts the impls leave out.)
        let named: Vec<String> = given_consts
            .iter()
            .filter_map(|k| Some(k.name()?.text().to_string()))
            .collect();
        let default_consts: Vec<ast::Const> = self
            .file_traits
            .iter()
            .filter(|t| t.name().is_some_and(|n| traits.contains(&n.text().to_string())))
            .flat_map(|t| t.assoc_item_list().into_iter().flat_map(|l| l.assoc_items()))
            .filter_map(|i| match i {
                ast::AssocItem::Const(k) if k.body().is_some() => Some(k),
                _ => None,
            })
            .filter(|k| k.name().is_some_and(|n| !named.contains(&n.text().to_string())))
            .collect();
        for k in given_consts.iter().chain(&default_consts) {
            let (Some(n), Some(body)) = (k.name(), k.body()) else {
                continue;
            };
            let ty = k
                .ty()
                .filter(|t| !t.syntax().text().to_string().contains("Self"))
                .map(|t| self.lower_type(&t))
                .or_else(|| Some(self_ty.clone()).filter(|_| !key.ends_with("$all")));
            let (f, _) = self.emit_expr(&body, ty.as_ref());
            let name = escape_ident(n.text());
            let mut line = Frag::mapped(
                self.off(k.syntax()),
                format!("static get {name}() {{ const v = "),
            );
            line.append(f);
            line.push(format!(
                "; Object.defineProperty(this, \"{name}\", {{ value: v }}); return v; }}"
            ));
            self.line(line);
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
            // (`trait Cap<const N: usize>`: `N` is this impl's argument, `impl Cap<3> for i32`.)
            let saved_extra = std::mem::take(&mut self.extra_consts);
            if let Some(t) = f.syntax().ancestors().find_map(ast::Trait::cast)
                && let Some(tn) = t.name()
                && let Some(acc) = self
                    .trait_const_args
                    .get(&(key.to_string(), tn.text().to_string()))
                    .cloned()
            {
                let cps: Vec<ast::ConstParam> = t
                    .generic_param_list()
                    .into_iter()
                    .flat_map(|l| l.generic_params())
                    .filter_map(|p| match p {
                        ast::GenericParam::ConstParam(cp) => Some(cp),
                        _ => None,
                    })
                    .collect();
                let mut extra = Vec::new();
                for (cp, a) in cps.iter().zip(acc) {
                    let Some(cn) = cp.name() else { continue };
                    let ty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                    extra.push((cn.text().to_string(), format!("({a})(this)"), ty));
                }
                self.extra_consts = extra;
            }
            // (Impls of the trait for two tuples of this length: a copy for each, with its own
            // elements as `self`'s, so its calls on `self` reach that impl's methods.)
            let alts = self
                .types
                .adts
                .method_alts
                .get(&format!("{key}::{}", n.text()))
                .filter(|_| crate::adt::tuple_arity(key).is_some())
                .cloned();
            if let Some(alts) = alts {
                for k in alts {
                    if let Some(sig) = self.types.adts.method(key, &k).cloned() {
                        let own = Ty::Tuple(sig.impl_args.clone());
                        let k = (k != n.text()).then_some(k);
                        self.emit_method_fn(&f, sig, own, "}", k);
                    }
                }
            } else if let Some(sig) = self.types.adts.method(key, n.text()).cloned() {
                self.emit_method_fn(&f, sig, self_ty.clone(), "}", None);
            }
            self.extra_consts = saved_extra;
        }
        self.ext_class = saved_ext;
        self.out.indent -= 1;
        self.line(Frag::text("}"));
        self.emit_alt_dispatch(key, &format!("$ext${key}"));
        // The `&mut self` methods, which `κσ.$dcallMut` calls with the receiver's cell.
        let muts: Vec<String> = self
            .types
            .adts
            .methods
            .get(key)
            .map(|ms| {
                let mut v: Vec<String> = ms
                    .iter()
                    .filter(|(_, s)| cell_self(s, &self_ty, &self.types.adts))
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
        // The methods that take `T`'s class (`κσ.$dcallT` hands it over).
        let mut elems: Vec<String> = self
            .types
            .elem_methods
            .iter()
            .filter_map(|m| m.strip_prefix(&format!("{key}::")))
            .map(|n| super::js_string(&prop(n)))
            .collect();
        if !elems.is_empty() {
            elems.sort();
            self.line(Frag::text(format!(
                "$ext${key}.$elem = new Set([{}]);",
                elems.join(", ")
            )));
        }
        // Impls of one trait for two tuples of this length (`(u8, u8)`, `(String, String)`): each
        // method's keys with the impl's element classes, which `κσ.$dcall` / `$dcallT` match
        // against the elements' classes or values. (A function: those classes may come later.)
        // A slice's too (`[u8]` next to `[T]`), by the element's.
        if crate::adt::array_alts_key(key) {
            let mut alts: Vec<String> = Vec::new();
            let mut names: Vec<&String> = self
                .types
                .adts
                .method_alts
                .keys()
                .filter(|k| {
                    k.strip_prefix(&format!("{key}::")).is_some()
                        && self.types.adts.inherent_alts.contains(*k)
                })
                .collect();
            names.sort();
            for k in names {
                let m = &k[key.len() + 2..];
                let entries: Vec<String> = self.types.adts.method_alts[k]
                    .iter()
                    .filter_map(|alt| {
                        let sig = self.types.adts.method(key, alt)?;
                        let ds: Vec<String> = sig
                            .impl_args
                            .iter()
                            .map(|a| match self.dict_of(a) {
                                d if d == "undefined" => "null".to_string(),
                                d => d,
                            })
                            .collect();
                        Some(format!(
                            "[{}, [{}]]",
                            super::js_string(&prop(alt)),
                            ds.join(", ")
                        ))
                    })
                    .collect();
                alts.push(format!(
                    "{}: [{}]",
                    super::js_string(&prop(m)),
                    entries.join(", ")
                ));
            }
            if !alts.is_empty() {
                self.line(Frag::text(format!(
                    "$ext${key}.$alts = () => ({{ {} }});",
                    alts.join(", ")
                )));
            }
        }
        // `Box<dyn Shape>`'s `from`s: called by name only.
        if key.starts_with("dyn$") || key == "Formatter" {
            self.line(Frag::default());
            return;
        }
        // Traits from `mod` files: their default methods (which this file doesn't compile again)
        // are mixed in, and the class joins the module's registry as well as this file's, so the
        // module's generic functions find it (`κσ.$extRegIn`).
        let mut modules: Vec<String> = Vec::new();
        for tr in &traits {
            let Some(m) = self
                .types
                .adts
                .traits
                .get(tr)
                .and_then(|d| d.module.clone())
            else {
                continue;
            };
            if m == "κσ"
                || self
                    .file_traits
                    .iter()
                    .any(|t| t.name().is_some_and(|n| n.text() == tr.as_str()))
            {
                continue;
            }
            let prefix = format!("{}.{}", super::module_js(&m), escape_ident(tr));
            // (A trait with const parameters: its defaults are a factory over this impl's
            // arguments, see `emit_trait_defaults`.)
            let (defaults, statics) = match self.trait_const_args.get(&(key.to_string(), tr.clone())) {
                Some(acc) => (
                    format!("{prefix}$defaults?.({}) ?? {{}}", acc.join(", ")),
                    format!("{prefix}$statics?.({})", acc.join(", ")),
                ),
                None => (format!("{prefix}$defaults ?? {{}}"), format!("{prefix}$statics")),
            };
            self.line(Frag::text(format!(
                "κσ.$mixin($ext${key}, {defaults}, {}, {statics});",
                super::js_string(tr)
            )));
            if !modules.contains(&m) {
                modules.push(m);
            }
        }
        // A `&T` impl's class: `κσ.$dcallT` takes it for a reference's class (`κσ.$wrapT`).
        if let Some((_, kind)) = key.rsplit_once('$')
            && crate::adt::is_wrapper_key(key)
        {
            // (Its associated functions that take `T`'s class: where it goes among their
            // arguments, for `T::f()` through the reference's class.)
            let mut statics: Vec<String> = self
                .types
                .adts
                .methods
                .get(key)
                .into_iter()
                .flatten()
                .filter(|(m, s)| {
                    s.receiver.is_none()
                        && self
                            .types
                            .dict_params
                            .get(&format!("{key}::{m}"))
                            .is_some_and(|ps| !ps.is_empty())
                })
                .map(|(m, s)| format!("{}: {}", super::js_string(&prop(m)), s.params.len()))
                .collect();
            statics.sort();
            self.line(Frag::text(format!(
                "κσ.$wrapReg($extRegs, \"{kind}\", $ext${key}, {{{}}});",
                statics.join(", ")
            )));
            self.line(Frag::default());
            return;
        }
        // A blanket impl's class: `κσ.$dcall` tries it for any value.
        if key.ends_with("$all") {
            self.line(Frag::text(format!("κσ.$blanketReg($extRegs, $ext${key});")));
            for m in &modules {
                self.line(Frag::text(format!("κσ.$blanketRegIn({m}, $ext${key});")));
            }
            self.line(Frag::default());
            return;
        }
        // Found by `κσ.$dcall` when a generic function calls the method on a built-in value. (A
        // tuple is an array of its length.)
        if let Some(n) = crate::adt::tuple_arity(key) {
            self.line(Frag::text(format!(
                "κσ.$extReg($extRegs, \"array\", $ext${key}, false, {n});"
            )));
            for m in &modules {
                self.line(Frag::text(format!(
                    "κσ.$extRegIn({m}, \"array\", $ext${key}, false, {n});"
                )));
            }
            self.line(Frag::default());
            return;
        }
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
            k if let Some(r) = crate::ty::RangeKind::from_name(k) => (r.rt_kind(), false),
            // (`impl Tr for fn() -> i32`: any JS function.)
            "FnPtr" => ("function", false),
            "RefCell" => ("refcell", false),
            "Iter" => ("iter", false),
            "f32" | "f64" => ("number", false),
            // (The 64- and 128-bit kinds are BigInts.)
            "i64" | "u64" | "i128" | "u128" => ("bigint", true),
            _ => ("number", true),
        };
        self.line(Frag::text(format!(
            "κσ.$extReg($extRegs, \"{kind}\", $ext${key}, {integral});"
        )));
        for m in &modules {
            self.line(Frag::text(format!(
                "κσ.$extRegIn({m}, \"{kind}\", $ext${key}, {integral});"
            )));
        }
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
        // `trait Buf<const N: usize>`: the defaults are a factory over `N`'s accessor, called per
        // impl (`Buf$defaults(() => 4)`; `(s) => s.$M` for `impl<const M: usize> Buf<M> for
        // Arr<M>`), so each class's copy reads its own `N` (`$N(this)`).
        let cps = def.const_params.clone();
        let (open, close) = if cps.is_empty() {
            (" = {".to_string(), "};".to_string())
        } else {
            let params: Vec<String> = cps.iter().map(|n| format!("${n}")).collect();
            (format!(" = ({}) => ({{", params.join(", ")), "});".to_string())
        };
        let saved_consts = super::set_const_scope(None);
        for n in &cps {
            super::CONST_SCOPE.with(|s| s.borrow_mut().insert(n.clone(), format!("${n}(this)")));
        }
        self.extra_consts = t
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
            .filter_map(|p| match p {
                ast::GenericParam::ConstParam(cp) => {
                    let n = cp.name()?.text().to_string();
                    let ty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                    Some((n.clone(), format!("${n}(this)"), ty))
                }
                _ => None,
            })
            .collect();
        let mut header = Frag::mapped(self.item_start(t.syntax()), "export const ");
        header
            .append(Frag::mapped(
                self.off(name.syntax()),
                format!("{}$defaults", escape_ident(&tr)),
            ))
            .push(&open);
        self.line(header);
        self.out.indent += 1;
        self.in_trait_default = true;
        self.default_trait = tr.clone();
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
        self.line(Frag::text(close.clone()));
        self.line(Frag::text(format!(
            "export const {}$statics{open}",
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
        self.line(Frag::text(close));
        self.extra_consts.clear();
        super::set_const_scope(Some(saved_consts));
        self.line(Frag::default());
    }

    /// Traits named by a qualified call in this file (`Trait::m(..)`, `<T as Trait>::m(..)`), or
    /// with a method some type has as `Trait$method` (see `Adts::split_methods`).
    fn qualified_traits(&self) -> std::collections::HashSet<String> {
        self.types
            .qualified_calls
            .values()
            .map(|q| q.trait_name.clone())
            .chain(
                self.types
                    .adts
                    .split_methods()
                    .into_iter()
                    .map(|(tr, _)| tr),
            )
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
                    Some(m) => format!("{}.{}", super::module_js(&m), escape_ident(&tr)),
                    None => escape_ident(&tr),
                };
                let cls = self.class(&adt);
                // (A trait with const parameters: its defaults are a factory over this impl's
                // arguments, see `emit_trait_defaults`.)
                let call = match self.trait_const_args.get(&(adt.clone(), tr.clone())) {
                    Some(acc) => format!("({})", acc.join(", ")),
                    None => String::new(),
                };
                self.line(Frag::text(format!(
                    "κσ.$mixin({cls}, {prefix}$defaults{call}, {}, {prefix}$statics{call});",
                    super::js_string(&tr)
                )));
            }
        }
    }

    /// For each impl of a trait with const parameters, the JS accessors of its const arguments
    /// (`trait_const_args`); and the associated consts whose value reads the impl's const
    /// parameters (`param_consts`). Before any code is emitted: both are read at use sites.
    pub(super) fn note_const_impls(&mut self, file: &ast::SourceFile) {
        use ra_ap_syntax::ast::HasGenericArgs;
        for item in crate::adt::all_items(file) {
            let ast::Item::Impl(imp) = item else { continue };
            let (trait_name, type_name) = crate::adt::impl_parts(&imp);
            let Some(type_name) = type_name.or_else(|| crate::adt::impl_self_name(&imp)) else {
                continue;
            };
            let impl_consts: Vec<String> = imp
                .generic_param_list()
                .into_iter()
                .flat_map(|l| l.generic_params())
                .filter_map(|p| match p {
                    ast::GenericParam::ConstParam(cp) => Some(cp.name()?.text().to_string()),
                    _ => None,
                })
                .collect();
            // `impl Buf<4> for A` / `impl<const M: usize> Buf<M> for Arr<M>`.
            if let Some(tr) = &trait_name
                && self
                    .types
                    .adts
                    .traits
                    .get(tr)
                    .is_some_and(|d| !d.const_params.is_empty())
                && let Some(ast::Type::PathType(p)) = imp.trait_()
                && let Some(seg) = p.path().and_then(|p| p.segment())
            {
                let n_types = self.types.adts.traits[tr].type_params.len();
                let accessors: Vec<String> = seg
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|l| l.generic_args())
                    .skip(n_types)
                    .map(|a| {
                        let text: String = a.syntax().text().to_string().split_whitespace().collect();
                        let text = text.trim_matches(|c| c == '{' || c == '}').to_string();
                        if impl_consts.contains(&text) {
                            match imp
                                .self_ty()
                                .and_then(|st| crate::adt::const_path(&st, &text, &self.types.adts))
                            {
                                Some(suffix) => format!("(s) => s{suffix}"),
                                None => "() => undefined".to_string(),
                            }
                        } else {
                            format!("() => {}", self.const_js(&text))
                        }
                    })
                    .collect();
                let key = (
                    crate::adt::ext_key_any(&type_name)
                        .filter(|k| self.types.adts.ext_types.contains(k))
                        .unwrap_or_else(|| type_name.clone()),
                    tr.clone(),
                );
                self.trait_const_args.insert(key, accessors);
            }
            // `impl<T: Zero> Zero for W<T> { const ZERO: Self = W(T::ZERO); }`
            for a in imp.assoc_item_list().into_iter().flat_map(|l| l.assoc_items()) {
                let ast::AssocItem::Const(k) = a else { continue };
                let (Some(n), Some(body)) = (k.name(), k.body()) else { continue };
                let mut read: Vec<(String, usize)> = Vec::new();
                for p in body.syntax().descendants().filter_map(ast::Path::cast) {
                    let Some(first) = p.first_segment().and_then(|s| s.name_ref()) else {
                        continue;
                    };
                    let name = first.text().to_string();
                    if p.qualifier().is_none() || read.iter().any(|(x, _)| *x == name) {
                        continue;
                    }
                    if let Some(i) = crate::adt::impl_type_param_pos(&imp, &name) {
                        read.push((name, i));
                    }
                }
                if !read.is_empty() {
                    self.tparam_consts
                        .insert((type_name.clone(), n.text().to_string()), read);
                }
            }
            // `impl<const P: u128> U<P> { const PER: u128 = P * 1000; }`
            if !impl_consts.is_empty() {
                for a in imp.assoc_item_list().into_iter().flat_map(|l| l.assoc_items()) {
                    if let ast::AssocItem::Const(k) = a
                        && let (Some(n), Some(body)) = (k.name(), k.body())
                        && body.syntax().descendants().filter_map(ast::PathExpr::cast).any(|p| {
                            p.path()
                                .and_then(|p| p.as_single_name_ref())
                                .is_some_and(|r| impl_consts.contains(&r.text().to_string()))
                        })
                    {
                        self.param_consts
                            .insert((type_name.clone(), n.text().to_string()));
                    }
                }
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
        // arithmetic on a `T` calls (`κσ.$gop`). One with a number on the right (`impl Shl<u32> for
        // Bits`, `impl Mul<f64> for V2`, for `T: Mul<f64>`) serves too: with both, the right
        // operand's JS type picks. So does the only impl of its trait (`Add<Complex<R>>`).
        let ops: Vec<(String, String, bool)> = self
            .types
            .adts
            .op_impls
            .get(&def.name)
            .map(|v| {
                v.iter()
                    .filter_map(|i| {
                        let own = matches!(&i.rhs, Ty::Adt(n, _) if *n == def.name)
                            || matches!(i.trait_name.as_str(), "Neg" | "Not");
                        let int = matches!(i.rhs, Ty::Int(_) | Ty::Float { .. });
                        let sole = v.iter().filter(|j| j.trait_name == i.trait_name).count() == 1;
                        if !(own || int || sole) {
                            return None;
                        }
                        Some((
                            crate::adt::op_method(&i.trait_name)?.to_string(),
                            i.key.clone(),
                            int,
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
        for (m, key, int) in &ops {
            if seen.contains(m) {
                continue;
            }
            let call = |k: &String| format!("this{}(o)", super::member(&prop(k)));
            let mut body = call(key);
            if let Some((_, other, _)) = ops.iter().find(|(m2, _, i2)| m2 == m && i2 != int) {
                let (num, obj) = if *int { (key, other) } else { (other, key) };
                body = format!("typeof o === \"object\" ? {} : {}", call(obj), call(num));
            }
            lines(self, vec![format!("$op${m}(o) {{ return {body}; }}")]);
            seen.push(m.clone());
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
        // (`impl Clone for Box<dyn Tr>`'s, for fields holding one.)
        let dyn_clones = dyn_clones(&self.types.adts);
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
            // A unit variant is a new object too: `*r = ..` through a `&mut` to a copy rewrites
            // the copy in place (`κσ.$replace`), which mustn't reach the original.
            let carried = self.carried_names(&def.name);
            let unit = |tag: &str| {
                if carried.is_empty() {
                    format!("return new {cls}({tag});")
                } else {
                    let cs: Vec<String> = carried.iter().map(|n| format!("this.${n}")).collect();
                    format!("return {cls}.$with(new {cls}({tag}), {});", cs.join(", "))
                }
            };
            if self.types.adts.is_atom(&def.name) {
                ls.push("  return this;".into());
            } else if def.is_enum && !def.has_data() {
                ls.push(format!("  {}", unit("this.$tag")));
            } else {
                ls.extend(per_variant(
                    def,
                    &|_, v| {
                        let fields: Vec<String> = v
                            .fields
                            .iter()
                            .enumerate()
                            .map(|(k, f)| clone_of(&dyn_clones, &f.ty, &format!("this{}", access(v, k))))
                            .collect();
                        if def.is_enum {
                            if v.shape == Shape::Unit {
                                unit("this.$tag")
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
                                &self.types.adts,
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
                        .map(|(k, f)| {
                            // (A `&mut i32` field holds a slot: its value is `[1]`.)
                            let slot = if self
                                .types
                                .mut_ref_fields
                                .contains(&(def.name.clone(), f.name.clone()))
                            {
                                "[1]"
                            } else {
                                ""
                            };
                            dbg_of(&f.ty, &format!("this{}{slot}", access(v, k)), &params)
                        })
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
                            // (A bare `T` field: `T::default()` by the class passed in.)
                            .or_else(|| match &f.ty {
                                Ty::Param(p) => def
                                    .type_params
                                    .iter()
                                    .position(|x| x == p)
                                    .map(|i| format!("this?.$cargs?.[{i}]?.default()")),
                                _ => None,
                            })
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
                            &dyn_clones(&self.types.adts),
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
                    &dyn_clones(&self.types.adts),
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
    pub(super) fn emit_adt_path(
        &mut self,
        segs: &[String],
        path: Option<&ast::Path>,
    ) -> Option<(Frag, Ty)> {
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
                // A const reading the impl's type parameters: through a class view carrying their
                // classes (`W::<f64>::ZERO` → `κσ.$tv(W, [..]).ZERO`), by the value's type.
                if self.tparam_consts.contains_key(&(adt.clone(), m.clone()))
                    && let Some(Ty::Adt(a, targs)) =
                        path.and_then(|p| p.syntax().parent()).and_then(|e| self.ty_of(&e))
                    && a == adt
                {
                    let d = self.dict_of(&Ty::Adt(a, targs));
                    return Some((Frag::text(format!("{d}.{}", escape_ident(&m))), ty));
                }
                // A const reading the impl's const parameters: through a class view carrying
                // them (`U::<3>::PER` → `κσ.$cv(U, [3]).PER`; `Self::PER` in a method, `this.$P`).
                if self.param_consts.contains(&(adt.clone(), m.clone())) {
                    use ra_ap_syntax::ast::HasGenericArgs;
                    let params = self.types.adts.defs[&adt].type_params.clone();
                    let written: Vec<String> = path
                        .and_then(|p| {
                            p.segments()
                                .find(|s| s.name_ref().is_some_and(|n| n.text() == adt.as_str()))
                        })
                        .and_then(|s| s.generic_arg_list())
                        .into_iter()
                        .flat_map(|l| l.generic_args())
                        .map(|a| {
                            let t: String = a.syntax().text().to_string().split_whitespace().collect();
                            t.trim_matches(|c| c == '{' || c == '}').to_string()
                        })
                        .collect();
                    let args: Vec<Ty> = params
                        .iter()
                        .enumerate()
                        .map(|(i, p)| match written.get(i) {
                            Some(w) => Ty::ConstVal(w.clone()),
                            None => Ty::Param(p.clone()),
                        })
                        .collect();
                    let view = super::class_view(self.class(&adt), &args, &[]);
                    return Some((Frag::text(format!("{view}.{}", escape_ident(&m))), ty));
                }
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
    /// `ScriptArg::Str("x")`: the payload itself (the enum's value is the JS value).
    /// `ScriptArg::try_from(v)`: `Ok(v)` when the value fits a variant, else `Err(v)`.
    pub(super) fn emit_extern_enum_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[String],
    ) -> Option<(Frag, Ty)> {
        let [e, v] = segs else { return None };
        if self.types.adts.defs.contains_key(e) {
            return None;
        }
        let b = bindings::current();
        let et = b.enum_of(e)?;
        let start = self.off(c.syntax());
        if v == "try_from" {
            let a = c.arg_list()?.args().next()?;
            let (f, _) = self.emit_expr(&a, None);
            let variants = et.variants_js(|_| "null".to_string());
            let out = f.wrap(
                "κσ.$jsEnumTry(",
                &format!(", {variants}, {})", et.tag_js()),
            );
            return Some((
                out.at(start),
                Ty::Result(
                    Box::new(Ty::Extern(e.clone())),
                    Box::new(Ty::Extern("JsValue".into())),
                ),
            ));
        }
        // `FlagValue::from("home")`: the payload is the value.
        if v == "from" {
            let a = c.arg_list()?.args().next()?;
            let (f, ty) = self.emit_expr(&a, None);
            return Some((self.by_value(&a, f, &ty).at(start), Ty::Extern(e.clone())));
        }
        // (An unknown variant: the type checker has reported it.)
        let Some((_, variant)) = et.variant(v) else {
            return Some((Frag::text("undefined"), Ty::Unknown));
        };
        let tys = vec![variant.payload.clone()];
        let mut args = self.emit_args(c.arg_list(), &tys);
        if args.is_empty() {
            return Some((Frag::text("undefined"), Ty::Unknown));
        }
        Some((args.remove(0).at(start), Ty::Extern(e.clone())))
    }

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
                        if let Some(ds) = self.dict_args_at(self.range_of(c.syntax())) {
                            for d in ds {
                                args.push(Frag::text(d));
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
        // (`$std`: the derived comparison, past an `impl PartialEq<Other>`'s `eq`.)
        let std_cmp = picked.as_deref() == Some("$std");
        let method = picked.as_deref().filter(|_| !std_cmp).unwrap_or(method);
        if !std_cmp
            && let Some(sig) = self.types.adts.method(adt, method).cloned()
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
            if let Some(ds) = self.dict_args_at(self.range_of(m.syntax())) {
                for d in ds {
                    args.push(Frag::text(d));
                }
            }
            args.extend(self.dbg_arg_frags(m.syntax()));
            // A `&mut self` method on an immutable enum: `this` is the receiver's cell
            // (`Dir.prototype.turn.call(cell)`).
            let self_ty = Ty::Adt(adt.to_string(), Vec::new());
            let f = if cell_self(&sig, &self_ty, &self.types.adts)
                && let Some(recv) = m.receiver()
            {
                let cell = self.mut_recv_cell(&recv, rf.clone());
                let mut f = Frag::mapped(
                    name_off,
                    format!("{}.prototype.{}.call(", self.class(adt), prop(method)),
                );
                f.append(Frag::join([vec![cell], args].concat(), ", ")).push(")");
                f
            } else {
                let mut f = rf.clone();
                f.push(".")
                    .append(Frag::mapped(name_off, prop(method)))
                    .push("(");
                f.append(Frag::join(args, ", ")).push(")");
                f
            };
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
        // A bindings enum's variant (`ScriptArg::Str(s)`): the value is the payload itself, told
        // apart by its JS type or its tag field.
        if let [e, v] = segs.as_slice()
            && !self.types.adts.defs.contains_key(e)
            && let Some(et) = bindings::current().enum_of(e)
            && let Some((_, variant)) = et.variant(v)
            && let Some(repr) = &et.repr
        {
            out.conds.push(Frag::text(variant.js_test(repr, subject)));
            if let ast::Pat::TupleStructPat(tp) = p
                && let Some(f) = tp.fields().next()
                && !matches!(f, ast::Pat::RestPat(_))
            {
                self.lower_pat(&f, subject, out);
            }
            return true;
        }
        let Some(Resolved::Ctor(adt, v)) = self.resolve(&segs) else {
            // `Range { start, end }`: the runtime range's bounds.
            if let ast::Pat::RecordPat(rp) = p
                && crate::typeck::range_struct_pat(&segs).is_some()
            {
                for f in rp.record_pat_field_list().into_iter().flat_map(|l| l.fields()) {
                    if let (Some(fname), Some(sub)) = (f.field_name(), f.pat()) {
                        let at = format!("κσ.$rangeOf({subject}).{}", fname.text());
                        self.lower_pat(&sub, &at, out);
                    }
                }
                return true;
            }
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

    /// Whether an immutable enum has a `&mut self` method: generic calls of those go through
    /// `κσ.$dcallMut` and the file's registry.
    pub(super) fn atom_mut_methods(&self) -> bool {
        let adts = &self.types.adts;
        adts.defs.keys().any(|k| {
            adts.is_atom(k)
                && adts.methods.get(k).is_some_and(|ms| {
                    ms.values()
                        .any(|s| s.receiver == Some(crate::adt::Receiver::MutRef))
                })
        })
    }

    /// A number, string, bool or char, or an immutable enum (`Adts::is_atom`): `typeck`'s `prim`.
    pub(super) fn prim(&self, t: &Ty) -> bool {
        t.is_js_primitive() || self.types.adts.atom_ty(t)
    }

    /// Whether values of `ty` are copied when read from a place (tuples, `Copy` structs).
    pub(super) fn copied_on_read(&self, ty: &Ty) -> bool {
        match ty {
            // A generic value may be a tuple or `Copy` struct; `κσ.$clone` copies those and leaves
            // numbers and strings alone.
            Ty::Tuple(_) | Ty::Param(_) => true,
            // (An immutable enum is shared: `Adts::is_atom`.)
            Ty::Adt(n, _) if self.types.adts.is_atom(n) => false,
            Ty::Adt(n, _) => self
                .types
                .adts
                .defs
                .get(n)
                .is_some_and(|d| d.derives("Copy")),
            // `Some(p)` is `p` itself and `Ok(x)` an object whose payload `ref mut` / `.as_mut()`
            // writes in place, so a copy of either mustn't share them with the original.
            Ty::Option(e) => self.copied_on_read(e),
            Ty::Result(a, b) => self.is_copy(a) && self.is_copy(b),
            _ => false,
        }
    }

    /// A type whose values are `Copy` (as far as the types say): reading one from a place leaves
    /// the original usable.
    fn is_copy(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Unit => true,
            Ty::Tuple(ts) => ts.iter().all(|t| self.is_copy(t)),
            Ty::Option(e) => self.is_copy(e),
            Ty::Result(a, b) => self.is_copy(a) && self.is_copy(b),
            Ty::Adt(n, _) => self
                .types
                .adts
                .defs
                .get(n)
                .is_some_and(|d| d.derives("Copy")),
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
