//! Typed bindings for JS APIs: the Netscript `NS` interface and the types it hands out.
//!
//! Declared in Rust syntax (generated from Bitburner's NetscriptDefinitions.d.ts by
//! `tools/dts2rs.mjs`) and loaded once with [`set`]:
//!
//! ```text
//! pub struct Server { #[js = "maxRam"] pub max_ram: f64, ... }
//! impl NS { #[js = "hack"] pub async fn hack(&self, #[optional] host: String) -> f64; }
//! ```
//!
//! Without bindings, `NS` stays untyped: any method or field is accepted, and snake_case names
//! map to camelCase.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ra_ap_syntax::{
    AstNode, SourceFile,
    ast::{self, HasAttrs, HasGenericArgs, HasModuleItem, HasName},
};

use crate::macros::EDITION;
use crate::ty::Ty;

// Parameter names and docs are kept for the language service (hover, signature help).
#[allow(dead_code)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    /// The type as the language service shows it: as declared for a reference (`&str`, `&[String]`;
    /// `ty` erases the `&`), else `ty`'s.
    pub shown: String,
    /// May be omitted (a trailing optional TS parameter).
    pub optional: bool,
}

// Parameter names and docs are kept for the language service (hover, signature help).
#[allow(dead_code)]
pub struct Method {
    pub js: String,
    pub params: Vec<Param>,
    /// Element type of a trailing `...rest` parameter.
    pub variadic: Option<Ty>,
    pub ret: Ty,
    pub is_async: bool,
    pub doc: String,
    /// No `self`: called on the type (`Date::now()`), `js` naming the whole JS callee
    /// (`globalThis.Date.now`, `new globalThis.Date`).
    pub is_static: bool,
    /// `#[getter]`: the JS expression itself, not a call (`js::document()` is `globalThis.document`).
    pub getter: bool,
    /// `#[array]`: the result is array-like (a `NodeList`): made a JS array (`null` → empty).
    pub array: bool,
    /// `#[runtime]`: the runtime's `κσ.$jsv.$<js>(value, ..)`, not a JS method of the value
    /// (`flags.get_str("target")`, read off the game's plain object).
    pub runtime: bool,
}

// Parameter names and docs are kept for the language service (hover, signature help).
#[allow(dead_code)]
pub struct Field {
    pub js: String,
    pub ty: Ty,
    pub doc: String,
    /// `#[array]`: an array-like value (an `HTMLCollection`), read as a JS array.
    pub array: bool,
}

/// How a bindings enum's variants are told apart at run time. The value is the JS value itself
/// (a `ScriptArg::Num(5.0)` is `5`, a `PlayerRequirement::Money(m)` is the game's object), so
/// nothing is converted when it crosses into or out of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Repr {
    /// `#[untagged]`: a primitive union (`string | number | boolean`), by the value's JS type.
    Untagged,
    /// `#[tag = "type"]`: an object union, by a field's literal value (`v.type === "money"`).
    Tagged(String),
}

/// The tag value a tagged enum's variant carries (`#[js = "money"]`, `#[js = true]`).
#[derive(Clone, Debug, PartialEq)]
pub enum JsTag {
    Str(String),
    Bool(bool),
}

/// A variant of a bindings enum: `Str(String)`, `#[js = "money"] Money(MoneyRequirement)`.
#[derive(Clone, Debug)]
pub struct ExternVariant {
    pub name: String,
    /// Every variant has one payload: the value itself, seen as this type.
    pub payload: Ty,
    /// The payload as declared, for messages (`String`, `RunOptions`).
    pub shown: String,
    pub tag: Option<JsTag>,
    pub doc: String,
}

impl ExternVariant {
    /// What JS calls values of an untagged variant's payload: the `typeof` result, or `array`
    /// for an array.
    pub fn js_kind(&self) -> &'static str {
        match &self.payload {
            Ty::Str | Ty::Char => "string",
            Ty::Int(k) if k.is_big() => "bigint",
            Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => "number",
            Ty::Bool => "boolean",
            Ty::Vec(_) | Ty::Tuple(_) | Ty::Coll(..) => "array",
            Ty::Closure { .. } | Ty::Fn => "function",
            _ => "object",
        }
    }

    /// The JS test that `subject` is this variant, given the enum's representation.
    pub fn js_test(&self, repr: &Repr, subject: &str) -> String {
        match (repr, &self.tag) {
            (Repr::Tagged(key), Some(JsTag::Str(v))) => {
                format!("{subject}?.{key} === {}", serde_json::to_string(v).unwrap_or_default())
            }
            (Repr::Tagged(key), Some(JsTag::Bool(v))) => format!("{subject}?.{key} === {v}"),
            (Repr::Tagged(key), None) => format!("{subject}?.{key} === undefined"),
            (Repr::Untagged, _) => match self.js_kind() {
                "array" => format!("Array.isArray({subject})"),
                "object" => format!(
                    "(typeof {subject} === \"object\" && {subject} !== null && !Array.isArray({subject}))"
                ),
                kind => format!("typeof {subject} === \"{kind}\""),
            },
        }
    }
}

#[derive(Default)]
pub struct ExternType {
    pub fields: HashMap<String, Field>,
    pub methods: HashMap<String, Method>,
    /// `#[extends = "Person"]`: the interfaces it extends (TS `interface Player extends Person`),
    /// where a value of it is accepted too.
    pub extends: Vec<String>,
    /// Whether it's declared as a struct (an interface a script can name and build), not only
    /// through an `impl`.
    pub is_struct: bool,
    /// `#[debug_keys]`: `{:?}` also prints the object's other own keys (`Flags`: each flag by
    /// name, which no field declares).
    pub debug_keys: bool,
    /// Whether it's declared as an enum (a TS union), with `repr` and `variants`.
    pub is_enum: bool,
    pub repr: Option<Repr>,
    pub variants: Vec<ExternVariant>,
    pub doc: String,
}

impl ExternType {
    pub fn variant(&self, name: &str) -> Option<(usize, &ExternVariant)> {
        self.variants.iter().enumerate().find(|(_, v)| v.name == name)
    }

    /// The variants' tests and names as a JS array literal, for the runtime's
    /// `$dbgJsEnum` / `$jsEnumTry`: `[["string", "Str"], ..]` for an untagged enum,
    /// `[["money", "Money"], ..]` for a tagged one (whose tag key is passed separately).
    pub fn variants_js(&self, with_fmt: impl Fn(&Ty) -> String) -> String {
        let items: Vec<String> = self
            .variants
            .iter()
            .map(|v| {
                let test = match (&self.repr, &v.tag) {
                    (Some(Repr::Tagged(_)), Some(JsTag::Str(s))) => {
                        serde_json::to_string(s).unwrap_or_default()
                    }
                    (Some(Repr::Tagged(_)), Some(JsTag::Bool(b))) => b.to_string(),
                    (Some(Repr::Tagged(_)), None) => "undefined".to_string(),
                    _ => format!("\"{}\"", v.js_kind()),
                };
                format!("[{test}, \"{}\", {}]", v.name, with_fmt(&v.payload))
            })
            .collect();
        format!("[{}]", items.join(", "))
    }

    /// The tag key as a JS argument (`"type"`), or `undefined` for an untagged enum.
    pub fn tag_js(&self) -> String {
        match &self.repr {
            Some(Repr::Tagged(k)) => serde_json::to_string(k).unwrap_or_default(),
            _ => "undefined".to_string(),
        }
    }
}

#[derive(Default)]
pub struct Bindings {
    pub types: HashMap<String, ExternType>,
    /// Free functions (the `js` crate's `js::set_timeout(..)`, `js::document()`).
    pub functions: HashMap<String, Method>,
    /// JS names Bitburner's RAM calculator charges for wherever they appear as identifiers (NS
    /// functions and namespaces). User identifiers with these names get a `$` suffix.
    pub ram_names: HashSet<String>,
}

impl Bindings {
    /// Whether interface `sub` is `sup` or extends it (`Player` extends `Person`).
    pub fn extends(&self, sub: &str, sup: &str) -> bool {
        let mut stack = vec![sub.to_string()];
        let mut seen = HashSet::new();
        while let Some(t) = stack.pop() {
            if t == sup {
                return true;
            }
            if seen.insert(t.clone())
                && let Some(et) = self.types.get(&t)
            {
                stack.extend(et.extends.iter().cloned());
            }
        }
        false
    }

    /// Whether `name` is an interface of the NS bindings a script can name and build
    /// (`RunOptions`, `Server`): a struct the bindings declare.
    pub fn is_interface(&self, name: &str) -> bool {
        self.types.get(name).is_some_and(|t| t.is_struct)
    }

    /// Whether `name` is an enum of the bindings (a TS union: `ScriptArg`, `PlayerRequirement`).
    pub fn is_enum(&self, name: &str) -> bool {
        self.types.get(name).is_some_and(|t| t.is_enum)
    }

    /// The bindings enum `name`.
    pub fn enum_of(&self, name: &str) -> Option<&ExternType> {
        self.types.get(name).filter(|t| t.is_enum)
    }
}

/// The JS globals scripts reach through the `js` crate (`js::Date`): always there, next to
/// the NS bindings.
const JS: &str = include_str!("../../bindings/js.rs");

/// `bindings` plus the JS globals' types (theirs don't count toward RAM names).
fn with_js(mut bindings: Bindings) -> Bindings {
    let (js, _) = parse(JS);
    for (name, ty) in js.types {
        bindings.types.entry(name).or_insert(ty);
    }
    for (name, f) in js.functions {
        bindings.functions.entry(name).or_insert(f);
    }
    bindings
}

thread_local! {
    /// The type names the `js` crate declares (`Date`, `Element`, `console`, ..).
    static JS_TYPE_NAMES: HashSet<String> = {
        let file = SourceFile::parse(JS, EDITION).tree();
        file.items()
            .filter_map(|i| match i {
                ast::Item::Struct(s) => s.name().map(|n| n.text().to_string()),
                _ => None,
            })
            .collect()
    };
}

/// Whether `name` is a type of the `js` crate.
pub fn is_js_type(name: &str) -> bool {
    JS_TYPE_NAMES.with(|t| t.contains(name))
}

thread_local! {
    static CURRENT: RefCell<Rc<Bindings>> = RefCell::new(Rc::new(with_js(Bindings::default())));
}

/// The bindings compiles currently use.
pub fn current() -> Rc<Bindings> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Parse and install bindings. Returns problems found (the parsable parts are still installed).
pub fn set(text: &str) -> Vec<String> {
    let (bindings, errors) = parse(text);
    CURRENT.with(|c| *c.borrow_mut() = Rc::new(with_js(bindings)));
    GENERATION.with(|g| g.set(g.get() + 1));
    errors
}

thread_local! {
    /// How many times bindings were installed (results kept across calls name the one they used).
    static GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub(crate) fn generation() -> u64 {
    GENERATION.with(|g| g.get())
}

/// Rust identifiers may be raw (`r#type`); lookups use the bare name.
pub fn bare(name: &str) -> String {
    name.strip_prefix("r#").unwrap_or(name).to_string()
}

/// The string value of a `#[name = "value"]` attribute, or whether a bare `#[name]` is present.
fn attr_value(item: &impl HasAttrs, name: &str) -> Option<String> {
    item.attrs().find_map(|a| {
        let text = a.syntax().text().to_string();
        let inner = text
            .trim()
            .strip_prefix("#[")?
            .strip_suffix(']')?
            .trim()
            .to_string();
        let (key, value) = match inner.split_once('=') {
            Some((k, v)) => (k.trim().to_string(), v.trim().to_string()),
            None => (inner, String::new()),
        };
        (key == name).then(|| value.trim_matches('"').to_string())
    })
}

/// The value of a `#[name = value]` attribute as written (quotes kept): `"money"` or `true`.
fn attr_raw(item: &impl HasAttrs, name: &str) -> Option<String> {
    item.attrs().find_map(|a| {
        let text = a.syntax().text().to_string();
        let inner = text.trim().strip_prefix("#[")?.strip_suffix(']')?.trim().to_string();
        let (key, value) = inner.split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_string())
    })
}

fn doc(item: &impl HasAttrs) -> String {
    item.doc_comments()
        .map(|c| {
            c.syntax()
                .text()
                .to_string()
                .trim_start_matches('/')
                .trim()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn lower(t: &ast::Type, structs: &HashSet<String>) -> Ty {
    match t {
        ast::Type::RefType(r) => r.ty().map(|t| lower(&t, structs)).unwrap_or(Ty::Unknown),
        ast::Type::PtrType(r) => r.ty().map(|t| lower(&t, structs)).unwrap_or(Ty::Unknown),
        ast::Type::ParenType(p) => p.ty().map(|t| lower(&t, structs)).unwrap_or(Ty::Unknown),
        ast::Type::SliceType(s) => Ty::Vec(Box::new(
            s.ty().map(|t| lower(&t, structs)).unwrap_or(Ty::Unknown),
        )),
        ast::Type::TupleType(tt) if tt.fields().next().is_none() => Ty::Unit,
        // `(f64, String)`; `[f64; 2]` is a `Vec` (arrays are).
        ast::Type::TupleType(tt) => Ty::Tuple(tt.fields().map(|t| lower(&t, structs)).collect()),
        ast::Type::ArrayType(a) => Ty::Vec(Box::new(
            a.ty().map(|t| lower(&t, structs)).unwrap_or(Ty::Unknown),
        )),
        // `impl Fn(f64) -> bool`: a callback. `impl Future<Output = T>`: a JS promise (a
        // task's `next_completion`), `.await`ed like an `async fn`'s result.
        ast::Type::ImplTraitType(i) => {
            let Some(seg) = i
                .type_bound_list()
                .and_then(|l| l.bounds().next())
                .and_then(|b| match b.ty()? {
                    ast::Type::PathType(p) => p.path()?.segment(),
                    _ => None,
                })
            else {
                return Ty::Unknown;
            };
            if seg.name_ref().is_some_and(|n| n.text() == "Future") {
                let output = seg
                    .generic_arg_list()
                    .and_then(|l| {
                        l.generic_args().find_map(|a| match a {
                            ast::GenericArg::AssocTypeArg(at) => at.ty(),
                            _ => None,
                        })
                    })
                    .map(|t| lower(&t, structs))
                    .unwrap_or(Ty::Unit);
                return Ty::Future(Box::new(output));
            }
            let params = seg
                .parenthesized_arg_list()
                .map(|l| {
                    l.type_args()
                        .filter_map(|a| a.ty())
                        .map(|t| lower(&t, structs))
                        .collect()
                })
                .unwrap_or_default();
            let ret = seg
                .ret_type()
                .and_then(|r| r.ty())
                .map(|t| lower(&t, structs))
                .unwrap_or(Ty::Unit);
            Ty::Closure {
                params,
                ret: Box::new(ret),
            }
        }
        ast::Type::PathType(p) => {
            let Some(seg) = p.path().and_then(|p| p.segment()) else {
                return Ty::Unknown;
            };
            let name = seg
                .name_ref()
                .map(|n| n.text().to_string())
                .unwrap_or_default();
            let arg = || {
                let a = seg.generic_arg_list()?.generic_args().next()?;
                match a {
                    ast::GenericArg::TypeArg(ta) => ta.ty().map(|t| lower(&t, structs)),
                    _ => None,
                }
            };
            match name.as_str() {
                "Vec" => Ty::Vec(Box::new(arg().unwrap_or(Ty::Unknown))),
                // (A JS `Map` or object, made a `HashMap` where it's read: `$mapOfJs`.)
                "HashMap" => {
                    let args: Vec<Ty> = seg
                        .generic_arg_list()
                        .map(|l| {
                            l.generic_args()
                                .filter_map(|a| match a {
                                    ast::GenericArg::TypeArg(ta) => {
                                        ta.ty().map(|t| lower(&t, structs))
                                    }
                                    _ => None,
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    match args.as_slice() {
                        [k, v] => Ty::Map(Box::new(k.clone()), Box::new(v.clone())),
                        _ => Ty::Unknown,
                    }
                }
                "Option" => Ty::Option(Box::new(arg().unwrap_or(Ty::Unknown))),
                // `Result<SuccessResult, String>` (what `NsResult::into_result` gives).
                "Result" => {
                    let args: Vec<Ty> = seg
                        .generic_arg_list()
                        .map(|l| {
                            l.generic_args()
                                .filter_map(|a| match a {
                                    ast::GenericArg::TypeArg(ta) => {
                                        ta.ty().map(|t| lower(&t, structs))
                                    }
                                    _ => None,
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    match args.as_slice() {
                        [t, e] => Ty::Result(Box::new(t.clone()), Box::new(e.clone())),
                        _ => Ty::Unknown,
                    }
                }
                // (A parameter of this type takes any value: see `loosen`.)
                "JsValue" => Ty::Extern("JsValue".into()),
                "JsAny" => Ty::Unknown,
                // (The `js` crate's, always loaded: `react::Node`. A parameter of it takes what
                // a `jsx!` child may be.)
                "ReactNode" => Ty::Extern("ReactNode".into()),
                n if structs.contains(n) => Ty::Extern(n.to_string()),
                // (`Date`: the `js` crate's, declared in `js.rs`.)
                n if is_js_type(n) => Ty::Extern(n.to_string()),
                n => Ty::from_name(n),
            }
        }
        // `-> !` (`ns.exit()`).
        ast::Type::NeverType(_) => Ty::Never,
        _ => Ty::Unknown,
    }
}

pub fn parse(text: &str) -> (Bindings, Vec<String>) {
    let parse = SourceFile::parse(text, EDITION);
    let mut errors: Vec<String> = parse
        .errors()
        .iter()
        .take(20)
        .map(|e| e.to_string())
        .collect();
    let file = parse.tree();
    // (The declared type names, structs and enums: `lower` makes them `Ty::Extern`.)
    let structs: HashSet<String> = file
        .items()
        .filter_map(|i| match i {
            ast::Item::Struct(s) => s.name().map(|n| n.text().to_string()),
            ast::Item::Enum(e) => e.name().map(|n| n.text().to_string()),
            _ => None,
        })
        .collect();
    let mut bindings = Bindings::default();
    for item in file.items() {
        match item {
            // `#[untagged] pub enum ScriptArg { Str(String), Num(f64), Bool(bool) }`,
            // `#[tag = "type"] pub enum PlayerRequirement { #[js = "money"] Money(MoneyRequirement), .. }`
            ast::Item::Enum(e) => {
                let Some(name) = e.name() else { continue };
                let repr = match attr_value(&e, "tag") {
                    Some(key) => Repr::Tagged(key),
                    None if attr_value(&e, "untagged").is_some() => Repr::Untagged,
                    None => {
                        errors.push(format!(
                            "enum `{}` needs `#[untagged]` or `#[tag = \"..\"]`",
                            name.text()
                        ));
                        continue;
                    }
                };
                let mut variants = Vec::new();
                for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
                    let Some(vname) = v.name() else { continue };
                    let payload = match v.field_list() {
                        Some(ast::FieldList::TupleFieldList(l)) if l.fields().count() == 1 => {
                            l.fields().next().and_then(|f| f.ty())
                        }
                        _ => None,
                    };
                    let Some(payload) = payload else {
                        errors.push(format!(
                            "variant `{}::{}` must have exactly one tuple field",
                            name.text(),
                            vname.text()
                        ));
                        continue;
                    };
                    let tag = attr_raw(&v, "js").map(|raw| match raw.as_str() {
                        "true" => JsTag::Bool(true),
                        "false" => JsTag::Bool(false),
                        s => JsTag::Str(s.trim_matches('"').to_string()),
                    });
                    if matches!(repr, Repr::Tagged(_)) && tag.is_none() {
                        errors.push(format!(
                            "variant `{}::{}` of a tagged enum needs `#[js = \"..\"]`",
                            name.text(),
                            vname.text()
                        ));
                    }
                    variants.push(ExternVariant {
                        name: vname.text().to_string(),
                        payload: lower(&payload, &structs),
                        shown: payload.syntax().text().to_string(),
                        tag,
                        doc: doc(&v),
                    });
                }
                if repr == Repr::Untagged {
                    let mut kinds: Vec<&str> = variants.iter().map(|v| v.js_kind()).collect();
                    kinds.sort_unstable();
                    kinds.dedup();
                    if kinds.len() != variants.len() {
                        errors.push(format!(
                            "untagged enum `{}`: two variants have the same JS type",
                            name.text()
                        ));
                    }
                }
                let ty = bindings.types.entry(name.text().to_string()).or_default();
                ty.is_enum = true;
                ty.repr = Some(repr);
                ty.variants = variants;
                ty.doc = doc(&e);
            }
            ast::Item::Struct(s) => {
                let Some(name) = s.name() else { continue };
                let ty = bindings.types.entry(name.text().to_string()).or_default();
                ty.is_struct = true;
                ty.debug_keys = attr_value(&s, "debug_keys").is_some();
                if let Some(bases) = attr_value(&s, "extends") {
                    ty.extends = bases
                        .split(',')
                        .map(|b| b.trim().to_string())
                        .filter(|b| !b.is_empty())
                        .collect();
                }
                let Some(ast::FieldList::RecordFieldList(list)) = s.field_list() else {
                    continue;
                };
                for f in list.fields() {
                    let (Some(fname), Some(fty)) = (f.name(), f.ty()) else {
                        continue;
                    };
                    let js = attr_value(&f, "js").unwrap_or_else(|| bare(fname.text()));
                    let field = Field {
                        js,
                        ty: lower(&fty, &structs),
                        doc: doc(&f),
                        array: attr_value(&f, "array").is_some(),
                    };
                    ty.fields.insert(bare(fname.text()), field);
                }
            }
            ast::Item::Impl(imp) => {
                let target = match imp.self_ty() {
                    Some(ast::Type::PathType(p)) => p
                        .path()
                        .and_then(|p| p.segment())
                        .and_then(|s| s.name_ref()),
                    _ => None,
                };
                let Some(target) = target else { continue };
                let ty = bindings.types.entry(target.text().to_string()).or_default();
                let Some(items) = imp.assoc_item_list() else {
                    continue;
                };
                for assoc in items.assoc_items() {
                    let ast::AssocItem::Fn(f) = assoc else {
                        continue;
                    };
                    let Some(fname) = f.name() else { continue };
                    ty.methods
                        .insert(bare(fname.text()), method_of(&f, &structs));
                }
            }
            ast::Item::Fn(f) => {
                if let Some(fname) = f.name() {
                    bindings
                        .functions
                        .insert(bare(fname.text()), method_of(&f, &structs));
                }
            }
            _ => {}
        }
    }
    if !bindings.types.contains_key("NS") {
        errors.push("bindings don't declare `NS`".into());
    }
    let mut ram = HashSet::new();
    for t in bindings.types.values() {
        ram.extend(t.methods.values().filter(|m| !m.runtime).map(|m| m.js.clone()));
    }
    if let Some(ns) = bindings.types.get("NS") {
        ram.extend(ns.fields.values().map(|f| f.js.clone()));
    }
    bindings.ram_names = ram;
    (bindings, errors)
}

/// A parameter's type: `JsValue` (anywhere in it) takes any value, as JS does.
fn loosen(t: Ty) -> Ty {
    match t {
        Ty::Extern(n) if n == "JsValue" => Ty::Unknown,
        Ty::Vec(e) => Ty::Vec(Box::new(loosen(*e))),
        Ty::Option(e) => Ty::Option(Box::new(loosen(*e))),
        other => other,
    }
}

/// A declared function or method's signature.
fn method_of(f: &ast::Fn, structs: &HashSet<String>) -> Method {
    let fname = f.name().map(|n| n.text().to_string()).unwrap_or_default();
    let mut params = Vec::new();
    let mut variadic = None;
    for p in f
        .param_list()
        .map(|l| l.params().collect::<Vec<_>>())
        .unwrap_or_default()
    {
        let pty = loosen(p.ty().map(|t| lower(&t, structs)).unwrap_or(Ty::Unknown));
        if attr_value(&p, "variadic").is_some() {
            variadic = Some(pty);
            continue;
        }
        let name = p
            .pat()
            .map(|x| x.syntax().text().to_string())
            .unwrap_or_default();
        let shown = match p.ty() {
            Some(t @ ast::Type::RefType(_)) => t.syntax().text().to_string(),
            _ => pty.describe(),
        };
        params.push(Param {
            name,
            ty: pty,
            shown,
            optional: attr_value(&p, "optional").is_some(),
        });
    }
    Method {
        js: attr_value(f, "js").unwrap_or_else(|| bare(&fname)),
        params,
        variadic,
        ret: f
            .ret_type()
            .and_then(|r| r.ty())
            .map(|t| lower(&t, structs))
            .unwrap_or(Ty::Unit),
        is_async: f.async_token().is_some(),
        doc: doc(f),
        is_static: f.param_list().and_then(|l| l.self_param()).is_none(),
        getter: attr_value(f, "getter").is_some(),
        array: attr_value(f, "array").is_some(),
        runtime: attr_value(f, "runtime").is_some(),
    }
}

/// Whether Bitburner charges RAM for an identifier named `js`.
pub fn is_ram_name(js: &str) -> bool {
    CURRENT.with(|c| c.borrow().ram_names.contains(js))
}

/// The closest name to `name` among `candidates`, for "did you mean" hints.
pub fn suggest<'a>(name: &str, candidates: impl Iterator<Item = &'a String>) -> Option<String> {
    let best = candidates
        .map(|c| (edit_distance(name, c), c))
        .min_by_key(|(d, c)| (*d, c.len()))?;
    let limit = (name.len() / 3).max(2);
    (best.0 <= limit).then(|| best.1.clone())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structs_and_methods() {
        let (b, errors) = parse(
            r#"
pub struct NS {
    #[js = "hacknet"]
    pub hacknet: Hacknet,
}
impl NS {
    /// Steal money.
    #[js = "hack"]
    pub async fn hack(&self, #[optional] host: String) -> f64;
    #[js = "tprint"]
    pub fn tprint(&self, #[variadic] args: JsValue);
}
pub struct Hacknet {}
"#,
        );
        assert!(errors.is_empty(), "{errors:?}");
        let ns = &b.types["NS"];
        assert_eq!(ns.fields["hacknet"].ty, Ty::Extern("Hacknet".into()));
        let hack = &ns.methods["hack"];
        assert!(hack.is_async && hack.params[0].optional && hack.ret == Ty::Float { f32: false });
        assert_eq!(hack.doc, "Steal money.");
        assert_eq!(ns.methods["tprint"].variadic, Some(Ty::Unknown));
    }

    #[test]
    fn generated_netscript_bindings_parse_cleanly() {
        let (b, errors) = parse(include_str!("../../bindings/ns.rs"));
        assert!(errors.is_empty(), "{errors:?}");
        let ns = &b.types["NS"];
        let hack = &ns.methods["hack"];
        assert_eq!((hack.js.as_str(), hack.is_async), ("hack", true));
        assert_eq!(ns.methods["get_server_max_money"].js, "getServerMaxMoney");
        assert_eq!(ns.methods["scan"].ret, Ty::Vec(Box::new(Ty::Str)));
        assert_eq!(ns.fields["hacknet"].ty, Ty::Extern("Hacknet".into()));
        assert!(b.types["Server"].fields.contains_key("max_ram"));
    }

    #[test]
    fn suggests_close_names() {
        let names = ["get_server_max_money".to_string(), "hack".to_string()];
        assert_eq!(
            suggest("get_server_max_mony", names.iter()).as_deref(),
            Some("get_server_max_money")
        );
        assert_eq!(suggest("zzzzzz", names.iter()), None);
    }
}
