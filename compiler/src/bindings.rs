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

#[derive(Default)]
pub struct ExternType {
    pub fields: HashMap<String, Field>,
    pub methods: HashMap<String, Method>,
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
    errors
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
        // `impl Fn(f64) -> bool`: a callback.
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
                "Option" => Ty::Option(Box::new(arg().unwrap_or(Ty::Unknown))),
                "JsValue" => Ty::Unknown,
                n if structs.contains(n) => Ty::Extern(n.to_string()),
                n => Ty::from_name(n),
            }
        }
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
    let structs: HashSet<String> = file
        .items()
        .filter_map(|i| match i {
            ast::Item::Struct(s) => s.name().map(|n| n.text().to_string()),
            _ => None,
        })
        .collect();
    let mut bindings = Bindings::default();
    for item in file.items() {
        match item {
            ast::Item::Struct(s) => {
                let Some(name) = s.name() else { continue };
                let ty = bindings.types.entry(name.text().to_string()).or_default();
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
        ram.extend(t.methods.values().map(|m| m.js.clone()));
    }
    if let Some(ns) = bindings.types.get("NS") {
        ram.extend(ns.fields.values().map(|f| f.js.clone()));
    }
    bindings.ram_names = ram;
    (bindings, errors)
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
        let pty = p.ty().map(|t| lower(&t, structs)).unwrap_or(Ty::Unknown);
        if attr_value(&p, "variadic").is_some() {
            variadic = Some(pty);
            continue;
        }
        let name = p
            .pat()
            .map(|x| x.syntax().text().to_string())
            .unwrap_or_default();
        params.push(Param {
            name,
            ty: pty,
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
