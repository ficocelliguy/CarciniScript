//! Modules: `mod name;` loads `name.rs` next to the current file (through the host's resolver),
//! and its public functions, structs, enums, traits and constants become usable here, by `use`
//! or as `name::item`. Each file compiles to its own JS module; this file imports it with
//! `import * as name from "./name.rs"`. A child module of `typeck`.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasGenericParams, HasModuleItem, HasName, HasVisibility},
};

use super::{Checker, FnSig};
use crate::adt::Adts;
use crate::ty::Ty;

/// Reads another source file by path (relative to the project root, like `lib/util.rs`).
pub type Resolver<'r> = &'r dyn Fn(&str) -> Option<String>;

/// A loaded module, for codegen.
#[derive(Clone, Debug)]
pub struct ModuleRef {
    /// The Rust (and JS namespace) name: `util`.
    pub name: String,
    /// The import specifier, relative to the importing file: `./util.rs`.
    pub specifier: String,
    /// `pub mod`: re-exported, so `parent::util::item` reaches it.
    pub public: bool,
}

/// A function or constant that lives in another module: `name` here is `module.js` in JS.
#[derive(Clone)]
pub struct Imported {
    pub name: String,
    pub module: String,
    pub js: String,
    /// The function's signature; None for a constant.
    pub sig: Option<FnSig>,
}

/// What a module file makes public.
#[derive(Default)]
struct Exports {
    fns: HashMap<String, FnSig>,
    adts: Adts,
    consts: HashMap<String, Ty>,
    /// Its functions' and associated functions' const parameters passed as values
    /// (function or `Type::name`, parameter, whether an argument holds it).
    const_needs: Vec<(String, String, bool)>,
    /// The values of its `const`s that are constant integers or `bool`s.
    const_vals: HashMap<String, crate::consteval::CVal>,
    /// A module this one loads (directly or not) loads it back.
    circular: Option<String>,
    /// Its `pub mod`s, by name.
    nested: Vec<(String, Exports)>,
    /// Its `pub type X = ..;` aliases (without generic parameters).
    aliases: HashMap<String, Ty>,
}

/// What a module's struct or enum `name` brings besides its definition and methods: `Deref`
/// targets, iterator items, associated types and const parameters of its impls.
fn export_type_extras(c: &Checker, out: &mut Exports, name: &str) {
    // (Types leave this checker without its inference variables: `zonk` makes them `Unknown`.)
    let a = &c.adts;
    if let (Some(ks), Some(file)) = (a.assoc_consts.get(name), &c.root_file) {
        let known = crate::consteval::Known {
            consts: Some(&c.const_vals),
            assoc: Some(&a.assoc_const_vals),
        };
        for k in ks.keys() {
            if let Some(v) = crate::consteval::eval_assoc(file, &known, name, k) {
                let vals = out
                    .adts
                    .assoc_const_vals
                    .entry(name.to_string())
                    .or_default();
                vals.insert(k.clone(), v);
            }
        }
    }
    if let Some(t) = a.deref_targets.get(name) {
        out.adts.deref_targets.insert(name.to_string(), c.zonk(t));
    }
    if a.deref_mut.contains(name) {
        out.adts.deref_mut.insert(name.to_string());
    }
    if let Some(t) = a.iter_items.get(name) {
        out.adts.iter_items.insert(name.to_string(), c.zonk(t));
    }
    if a.into_iters.contains(name) {
        out.adts.into_iters.insert(name.to_string());
    }
    if let Some(t) = a.future_outputs.get(name) {
        out.adts.future_outputs.insert(name.to_string(), c.zonk(t));
    }
    if let Some(cp) = a.const_params.get(name) {
        out.adts.const_params.insert(name.to_string(), cp.clone());
    }
    for (k, v) in &a.impl_assoc {
        if k.0 == name {
            let v = v.iter().map(|(n, t)| (n.clone(), c.zonk(t))).collect();
            out.adts.impl_assoc.insert(k.clone(), v);
        }
    }
}

/// `dir/of/current.rs` + `name` → `dir/of/name.rs`.
pub fn module_path(current: &str, name: &str) -> String {
    match current.rfind('/') {
        Some(i) => format!("{}/{name}.rs", &current[..i]),
        None => format!("{name}.rs"),
    }
}

/// `p` (a `#[path]` or `include_str!` argument) from the directory of `current`, as a path for
/// the resolver (from the project root, like `current`), `.` and `..` folded: `../lib/util.rs`
/// in `hack/main.rs` is `lib/util.rs`. A `/lib/util.rs` is from the root wherever `current` is.
/// None when `..` climbs out of the root.
pub fn relative_path(current: &str, p: &str) -> Option<String> {
    let p = p.replace('\\', "/");
    let mut parts: Vec<&str> = if p.starts_with('/') {
        Vec::new()
    } else {
        match current.rfind('/') {
            Some(i) => current[..i].split('/').collect(),
            None => Vec::new(),
        }
    };
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// The import specifier of `#[path = "p"]`: `/lib/util.rs` as written (the host resolves it
/// from the root, as Bitburner does its imports), a relative path from this file's directory as
/// `./p`.
fn path_specifier(p: &str) -> String {
    let p = p.replace('\\', "/");
    if p.starts_with('/') || p.starts_with("./") || p.starts_with("../") {
        p
    } else {
        format!("./{p}")
    }
}

/// The `#[path = "file.rs"]` of a `mod` item.
pub fn path_attr(m: &ast::Module) -> Option<String> {
    m.attrs().find_map(|a| {
        let ast::Meta::KeyValueMeta(meta) = a.meta()? else {
            return None;
        };
        if meta.path()?.syntax().text() != "path" {
            return None;
        }
        match meta.expr()? {
            ast::Expr::Literal(l) => match l.kind() {
                ast::LiteralKind::String(s) => s.value().ok().map(|v| v.into_owned()),
                _ => None,
            },
            _ => None,
        }
    })
}

/// The `#[path]` of `mod n;` among the items of `text`.
pub fn path_attr_in(text: &str, n: &str) -> Option<String> {
    if !text.contains("#[path") {
        return None;
    }
    let file = crate::macros::parse_tree(text);
    file.items().find_map(|i| match i {
        ast::Item::Module(m) if m.item_list().is_none() && m.name()?.text() == n => path_attr(&m),
        _ => None,
    })
}

/// Where `mod n;` in `filename` may live, each with its import specifier: its `#[path]` alone
/// when it has one (`explicit`), else `n.rs` next to the file, `n/mod.rs`, and (a module file's
/// children live in a folder named after it) `geo/n.rs` next to `geo.rs`.
pub fn module_candidates(filename: &str, n: &str, explicit: Option<&str>) -> Vec<(String, String)> {
    if let Some(p) = explicit {
        return relative_path(filename, p)
            .map(|r| (r, path_specifier(p)))
            .into_iter()
            .collect();
    }
    let mut out = vec![
        (module_path(filename, n), format!("./{n}.rs")),
        (
            module_path(filename, &format!("{n}/mod")),
            format!("./{n}/mod.rs"),
        ),
    ];
    let stem = filename
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".rs"))
        .filter(|s| !matches!(*s, "mod" | "main" | "lib"));
    if let Some(stem) = stem {
        out.push((
            module_path(filename, &format!("{stem}/{n}")),
            format!("./{stem}/{n}.rs"),
        ));
    }
    out
}

/// The file of `mod n;` in `filename` (with `explicit` its `#[path]`, if any): its path, text
/// and import specifier, the first of `module_candidates` the resolver has.
pub fn find_module(
    filename: &str,
    n: &str,
    explicit: Option<&str>,
    resolver: Resolver,
) -> Option<(String, String, String)> {
    module_candidates(filename, n, explicit)
        .into_iter()
        .find_map(|(p, spec)| resolver(&p).map(|t| (p, t, spec)))
}

/// `find_module` for `mod n;` in the file `parent`, whose text is `parent_text` (where its
/// `#[path]` would be).
pub fn find_module_in(
    parent: &str,
    parent_text: &str,
    n: &str,
    resolver: Resolver,
) -> Option<(String, String, String)> {
    find_module(parent, n, path_attr_in(parent_text, n).as_deref(), resolver)
}

/// A type's, trait's or constant's name (`Point`, `MAX`, `__BenchFlags`), not a function's or
/// module's.
pub fn capitalized(s: &str) -> bool {
    s.trim_start_matches('_')
        .starts_with(|c: char| c.is_ascii_uppercase())
}

/// What `pub use x::Error as AreaError;` items in `text` rename: `AreaError` → `Error`, for
/// capitalized names of the file's own modules (not std's or crates').
pub fn pub_use_renames(text: &str) -> HashMap<String, String> {
    let file = crate::macros::parse_tree(text);
    let mut out = HashMap::new();
    for item in file.items() {
        let ast::Item::Use(u) = item else { continue };
        if u.visibility().is_none() {
            continue;
        }
        for (path, rename, star) in use_leaves(&u) {
            let (Some(alias), Some(orig)) = (rename, path.last()) else {
                continue;
            };
            let capital = capitalized;
            if !star
                && path.len() >= 2
                && !matches!(path[0].as_str(), "std" | "core" | "alloc")
                && !crate::typeck::crates::BUNDLED.contains(&path[0].as_str())
                && capital(&alias)
                && capital(orig)
            {
                out.insert(alias, orig.clone());
            }
        }
    }
    out
}

/// The leaves of a `use` tree: (path, `as` name, whether it's `path::*`).
pub fn use_leaves(u: &ast::Use) -> Vec<(Vec<String>, Option<String>, bool)> {
    fn walk(
        t: &ast::UseTree,
        prefix: &[String],
        out: &mut Vec<(Vec<String>, Option<String>, bool)>,
    ) {
        let mut path = prefix.to_vec();
        if let Some(p) = t.path() {
            path.extend(
                p.segments()
                    .filter_map(|s| s.name_ref())
                    .map(|n| n.text().to_string()),
            );
        }
        if let Some(list) = t.use_tree_list() {
            for c in list.use_trees() {
                walk(&c, &path, out);
            }
            return;
        }
        let rename = t
            .rename()
            .and_then(|r| r.name())
            .map(|n| n.text().to_string());
        out.push((path, rename, t.star_token().is_some()));
    }
    let mut out = Vec::new();
    if let Some(t) = u.use_tree() {
        walk(&t, &[], &mut out);
    }
    out
}

/// Export `name` (a function, type, trait or constant the module can see) as `as_name`.
fn reexport(c: &Checker, out: &mut Exports, name: &str, as_name: &str) {
    if let Some(sig) = c.fns.get(name) {
        out.fns.insert(as_name.to_string(), sig.clone());
    }
    if let Some(t) = c.consts.get(name) {
        out.consts.insert(as_name.to_string(), t.clone());
    }
    if let Some(v) = c.const_vals.get(name) {
        out.const_vals.insert(as_name.to_string(), *v);
    }
    // Types and traits keep their names: types share one namespace here, so an importing file
    // writes `pub use area::Error as AreaError`'s `AreaError` as `Error` (`pub_use_renames`).
    {
        if let Some(def) = c.adts.defs.get(name) {
            let mut def = def.clone();
            def.module = None;
            out.adts.defs.insert(name.to_string(), def);
        }
        if let Some(ms) = c.adts.methods.get(name) {
            out.adts.methods.insert(name.to_string(), ms.clone());
        }
        if let Some(ts) = c.adts.trait_impls.get(name) {
            out.adts.trait_impls.insert(name.to_string(), ts.clone());
        }
        if let Some(fs) = c.adts.from_impls.get(name) {
            out.adts.from_impls.insert(name.to_string(), fs.clone());
        }
        if let Some(os) = c.adts.op_impls.get(name) {
            out.adts.op_impls.insert(name.to_string(), os.clone());
        }
        if let Some(ks) = c.adts.assoc_consts.get(name) {
            out.adts.assoc_consts.insert(name.to_string(), ks.clone());
        }
        export_type_extras(c, out, name);
        if let Some(t) = c.type_aliases.get(name) {
            out.aliases.insert(name.to_string(), t.clone());
        }
        if let Some(t) = c.adts.traits.get(name) {
            let mut t = t.clone();
            t.module = None;
            out.adts.traits.insert(name.to_string(), t);
        }
    }
}

/// The public signatures of the module in `text`.
fn exports_of(text: &str, path: &str, resolver: Resolver, stack: &mut Vec<String>) -> Exports {
    // Renamed imports and crates, as the file itself compiles them (`pub fn f() -> M` with `use
    // units::Meters as M;`).
    let mut renamed = text.to_string();
    let mut map = crate::mbe::OffsetMap::identity(text.len());
    if crate::alias::has_crate_renames(&renamed) {
        crate::alias::expand_crate_renames(&mut renamed, &mut map);
    }
    // Inline modules' items, as the file's own compile flattens them (`pub mod context { pub
    // struct NoContext; }` exports `context__NoContext`), and paths into other files' ones.
    if crate::alias::has_inline_module_imports(&renamed) {
        crate::alias::expand_inline_module_imports(&mut renamed, &mut map, Some((path, resolver)));
    }
    if crate::modflat::has_inline_modules(&renamed) {
        crate::modflat::flatten(&mut renamed, &mut map);
    }
    if crate::modclash::has_mod_files(&renamed) {
        crate::modclash::expand(&mut renamed, &mut map, Some((path, resolver)));
    }
    if crate::alias::has_import_renames(&renamed) {
        crate::alias::expand_import_renames(&mut renamed, &mut map, Some((path, resolver)));
    }
    if crate::alias::has_use_renames(&renamed) {
        crate::alias::expand_use_renames(&mut renamed, &mut map);
    }
    let text = renamed.as_str();
    let parse = crate::macros::parse_file(text);
    let file = parse.tree();
    let mut c = Checker::new(text);
    c.declare_items(&file, path, resolver, stack);
    let mut out = Exports {
        circular: c
            .diags
            .iter()
            .find(|d| d.message.starts_with("circular modules"))
            .map(|d| d.message.clone()),
        ..Exports::default()
    };
    for item in file.items().filter(|i| !crate::adt::test_only(i.syntax())) {
        let public = |v: Option<ast::Visibility>| v.is_some();
        match &item {
            ast::Item::Fn(f) if public(f.visibility()) => {
                if let Some(n) = f.name()
                    && let Some(sig) = c.fns.get(n.text().to_string().as_str())
                {
                    out.fns.insert(n.text().to_string(), sig.clone());
                }
            }
            ast::Item::Struct(_) | ast::Item::Enum(_) => {
                let (name, vis) = match &item {
                    ast::Item::Struct(s) => (s.name(), s.visibility()),
                    ast::Item::Enum(e) => (e.name(), e.visibility()),
                    _ => unreachable!(),
                };
                let Some(n) = name else { continue };
                if !public(vis) {
                    continue;
                }
                let n = n.text().to_string();
                if let Some(def) = c.adts.defs.get(&n) {
                    out.adts.defs.insert(n.clone(), def.clone());
                }
                if let Some(ms) = c.adts.methods.get(&n) {
                    out.adts.methods.insert(n.clone(), ms.clone());
                }
                if let Some(ts) = c.adts.trait_impls.get(&n) {
                    out.adts.trait_impls.insert(n.clone(), ts.clone());
                }
                if let Some(fs) = c.adts.from_impls.get(&n) {
                    out.adts.from_impls.insert(n.clone(), fs.clone());
                }
                if let Some(os) = c.adts.op_impls.get(&n) {
                    out.adts.op_impls.insert(n.clone(), os.clone());
                }
                if let Some(ks) = c.adts.assoc_consts.get(&n) {
                    out.adts.assoc_consts.insert(n.clone(), ks.clone());
                }
                export_type_extras(&c, &mut out, &n);
            }
            // `pub type Op = fn(i64) -> i64;`
            ast::Item::TypeAlias(t)
                if public(t.visibility()) && t.generic_param_list().is_none() =>
            {
                if let (Some(n), Some(ty)) = (t.name(), t.ty()) {
                    let lowered = c.lower_type(&ty);
                    out.aliases.insert(n.text().to_string(), c.zonk(&lowered));
                }
            }
            ast::Item::Trait(t) if public(t.visibility()) => {
                if let Some(n) = t.name()
                    && let Some(def) = c.adts.traits.get(n.text().to_string().as_str())
                {
                    out.adts.traits.insert(n.text().to_string(), def.clone());
                }
            }
            ast::Item::Const(k) if public(k.visibility()) => {
                if let Some(n) = k.name()
                    && let Some(t) = c.consts.get(n.text().to_string().as_str())
                {
                    out.consts.insert(n.text().to_string(), t.clone());
                    if let Some(v) = k.body().and_then(|b| c.eval_const(&b)) {
                        out.const_vals.insert(n.text().to_string(), v);
                    }
                }
            }
            ast::Item::Static(s) if public(s.visibility()) => {
                if let Some(n) = s.name()
                    && let Some(t) = c.consts.get(n.text().to_string().as_str())
                {
                    out.consts.insert(n.text().to_string(), t.clone());
                }
            }
            // `pub use circle::Circle;`: what this module imported is part of its exports too.
            ast::Item::Use(u) if public(u.visibility()) => {
                for (path, rename, star) in use_leaves(u) {
                    let rel: Vec<&str> = path
                        .iter()
                        .map(String::as_str)
                        .skip_while(|s| matches!(*s, "crate" | "self"))
                        .collect();
                    match (rel.as_slice(), star) {
                        ([module], true) => {
                            for n in c.module_exports.get(*module).cloned().unwrap_or_default() {
                                reexport(&c, &mut out, &n, &n);
                            }
                        }
                        ([_, .., item], false) => {
                            let as_name = rename.clone().unwrap_or_else(|| item.to_string());
                            reexport(&c, &mut out, item, &as_name);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    // `pub mod shapes;`: its exports, reachable as `this::shapes::item`.
    for m in c.modules.iter().filter(|m| m.public) {
        let mut sub = Exports::default();
        let mut names: Vec<String> = c
            .module_exports
            .get(&m.name)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        names.sort();
        for n in names {
            reexport(&c, &mut sub, &n, &n);
        }
        // (Its own `pub mod`s were merged as `shapes.inner`.)
        let prefix = format!("{}.", m.name);
        let mut deeper: Vec<&String> = c
            .module_exports
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .collect();
        deeper.sort();
        for k in deeper {
            let mut inner = Exports::default();
            for n in &c.module_exports[k] {
                reexport(&c, &mut inner, n, n);
            }
            sub.nested.push((k[prefix.len()..].to_string(), inner));
        }
        out.nested.push((m.name.clone(), sub));
    }
    // Const parameters its functions and associated functions take as values
    // (`Buf::<4>::new()` passes `4`): another file's calls pass them too.
    for item in file.items().filter(|i| !crate::adt::test_only(i.syntax())) {
        match &item {
            ast::Item::Fn(f) => {
                if let Some(n) = f.name()
                    && out.fns.contains_key(n.text().to_string().as_str())
                {
                    for (p, d) in c.const_param_needs(f, Vec::new()) {
                        out.const_needs.push((n.text().to_string(), p, d));
                    }
                }
            }
            ast::Item::Impl(imp) if imp.trait_().is_none() => {
                let Some(ast::Type::PathType(pt)) = imp.self_ty() else {
                    continue;
                };
                let Some(ty) = pt
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.name_ref())
                    .map(|n| n.text().to_string())
                else {
                    continue;
                };
                if !out.adts.defs.contains_key(&ty) {
                    continue;
                }
                let impl_consts: Vec<String> = imp
                    .generic_param_list()
                    .into_iter()
                    .flat_map(|l| l.generic_params())
                    .filter_map(|p| match p {
                        ast::GenericParam::ConstParam(cp) => Some(cp.name()?.text().to_string()),
                        _ => None,
                    })
                    .collect();
                for a in imp
                    .assoc_item_list()
                    .into_iter()
                    .flat_map(|l| l.assoc_items())
                {
                    let ast::AssocItem::Fn(f) = a else { continue };
                    let Some(name) = f.name() else { continue };
                    let Some(m) = c.adts.method(&ty, name.text().to_string().as_str()) else {
                        continue;
                    };
                    let extra: Vec<String> = if m.receiver.is_none() {
                        impl_consts
                            .iter()
                            .filter(|n| m.impl_args.contains(&Ty::Param((*n).clone())))
                            .cloned()
                            .collect()
                    } else {
                        Vec::new()
                    };
                    for (p, d) in c.const_param_needs(&f, extra) {
                        out.const_needs
                            .push((format!("{ty}::{}", name.text()), p, d));
                    }
                }
            }
            _ => {}
        }
    }
    // Types leave this module's checker: resolve its variables, and forget any still open.
    let freeze = |t: &Ty| -> Ty { erase_vars(&c.zonk(t)) };
    for sig in out.fns.values_mut() {
        sig.params = sig.params.iter().map(freeze).collect();
        sig.ret = freeze(&sig.ret);
    }
    for def in out.adts.defs.values_mut() {
        for v in &mut def.variants {
            for f in &mut v.fields {
                f.ty = freeze(&f.ty);
            }
        }
    }
    for fs in out.adts.from_impls.values_mut() {
        for f in fs.iter_mut() {
            f.src = freeze(&f.src);
        }
    }
    for os in out.adts.op_impls.values_mut() {
        for o in os.iter_mut() {
            o.rhs = freeze(&o.rhs);
        }
    }
    for ms in out.adts.methods.values_mut() {
        for m in ms.values_mut() {
            m.params = m.params.iter().map(freeze).collect();
            m.ret = freeze(&m.ret);
        }
    }
    for t in out.adts.traits.values_mut() {
        for m in &mut t.methods {
            m.sig.params = m.sig.params.iter().map(freeze).collect();
            m.sig.ret = freeze(&m.sig.ret);
        }
    }
    for t in out.consts.values_mut() {
        *t = freeze(t);
    }
    fn freeze_all(ex: &mut Exports, freeze: &dyn Fn(&Ty) -> Ty) {
        for (_, sub) in &mut ex.nested {
            for sig in sub.fns.values_mut() {
                sig.params = sig.params.iter().map(freeze).collect();
                sig.ret = freeze(&sig.ret);
            }
            for t in sub.consts.values_mut() {
                *t = freeze(t);
            }
            freeze_all(sub, freeze);
        }
    }
    freeze_all(&mut out, &freeze);
    out
}

/// `t` with any type variable replaced by `Unknown`.
fn erase_vars(t: &Ty) -> Ty {
    let e = |x: &Ty| Box::new(erase_vars(x));
    match t {
        Ty::Var(_) => Ty::Unknown,
        Ty::Vec(x) => Ty::Vec(e(x)),
        Ty::Option(x) => Ty::Option(e(x)),
        Ty::Set(x) => Ty::Set(e(x)),
        Ty::Coll(k, x) => Ty::Coll(*k, e(x)),
        Ty::Future(x) => Ty::Future(e(x)),
        Ty::Map(k, v) => Ty::Map(e(k), e(v)),
        Ty::Entry(k, v) => Ty::Entry(e(k), e(v)),
        Ty::Result(k, v) => Ty::Result(e(k), e(v)),
        Ty::Tuple(xs) => Ty::Tuple(xs.iter().map(erase_vars).collect()),
        Ty::Adt(n, args) => Ty::Adt(n.clone(), args.iter().map(erase_vars).collect()),
        Ty::RefCell(x) => Ty::RefCell(e(x)),
        Ty::Iter { elem, mutable } => Ty::Iter {
            elem: e(elem),
            mutable: *mutable,
        },
        Ty::Closure { params, ret } => Ty::Closure {
            params: params.iter().map(erase_vars).collect(),
            ret: e(ret),
        },
        other => other.clone(),
    }
}

impl Checker<'_> {
    /// Load every `mod name;` in `file`, then check its `use` declarations.
    pub(super) fn load_modules(
        &mut self,
        file: &ast::SourceFile,
        filename: &str,
        resolver: Resolver,
        stack: &mut Vec<String>,
    ) {
        // Type names taken by this file and earlier modules: a module's type named like one goes
        // by its mangled name (see `modclash`).
        let mut taken = crate::modclash::local_type_names(file);
        for item in crate::adt::all_items(file) {
            let ast::Item::Module(m) = &item else {
                continue;
            };
            let Some(name) = m.name() else { continue };
            let n = name.text().to_string();
            if m.item_list().is_some() {
                self.error(
                    m.syntax(),
                    format!("inline modules (`mod {n} {{ .. }}`) are not supported yet; put the module in `{n}.rs`"),
                );
                continue;
            }
            let explicit = path_attr(m);
            let candidates = module_candidates(filename, &n, explicit.as_deref());
            let Some((path, text, specifier)) = candidates
                .iter()
                .find_map(|(p, s)| resolver(p).map(|t| (p.clone(), t, s.clone())))
            else {
                // (A `#[path]` is looked for on its own, like rustc; `..` past the root is
                // nowhere.)
                let looked: Vec<String> =
                    candidates.iter().map(|(p, _)| format!("`{p}`")).collect();
                let msg = match (explicit, looked.split_last()) {
                    (Some(p), None) => {
                        format!(
                            "file not found for module `{n}` (`#[path = \"{p}\"]` leaves the project root)"
                        )
                    }
                    (_, Some((last, []))) => {
                        format!("file not found for module `{n}` (looked for {last})")
                    }
                    (_, Some((last, rest))) => format!(
                        "file not found for module `{n}` (looked for {} and {last})",
                        rest.join(", ")
                    ),
                    (None, None) => unreachable!(),
                };
                self.error(m.syntax(), msg);
                continue;
            };
            if stack.contains(&path) {
                self.error(
                    m.syntax(),
                    format!("circular modules: `{path}` is already being loaded"),
                );
                continue;
            }
            stack.push(path.clone());
            let mut exports = exports_of(&text, &path, resolver, stack);
            let clash =
                crate::modclash::clashing(&crate::modclash::pub_adt_names(&text), &mut taken);
            if !clash.is_empty() {
                let renamed = crate::modclash::rename_in_module(&text, &n, &clash);
                exports = exports_of(&renamed, &path, resolver, stack);
                for c in &clash {
                    self.adts
                        .js_names
                        .insert(crate::modclash::mangle(&n, c), c.clone());
                }
            }
            stack.pop();
            if let Some(msg) = &exports.circular {
                self.error(m.syntax(), format!("{msg} (through `{path}`)"));
            }
            self.merge_module(&n, exports);
            self.adts.modules.insert(n.clone());
            self.modules.push(ModuleRef {
                specifier,
                name: n,
                public: m.visibility().is_some(),
            });
        }
        for item in crate::adt::all_items(file) {
            if let ast::Item::Use(u) = &item
                && let Some(tree) = u.use_tree()
            {
                self.check_use_tree(&tree, &[]);
            }
            // `extern crate rand;` (2015-edition style): std and the bundled crates are always
            // there; others are as unavailable as their `use`.
            if let ast::Item::ExternCrate(e) = &item
                && let Some(n) = e.name_ref()
            {
                let name = n.text().to_string();
                let known = matches!(name.as_str(), "std" | "core" | "alloc" | "self")
                    || super::crates::BUNDLED.contains(&name.as_str());
                if !known {
                    let msg = format!(
                        "unresolved import `{name}` (load a module with `mod {name};`; crates other than std aren't available)"
                    );
                    self.error(e.syntax(), msg);
                } else if e.rename().is_some_and(|r| r.name().is_some()) && name != "self" {
                    self.error(e.syntax(), "renaming crates isn't supported yet");
                }
            }
        }
    }

    fn merge_module(&mut self, module: &str, ex: Exports) {
        // `pub mod`s of the module: their items are `module.child.item` in JS.
        for (child, sub) in ex.nested {
            self.adts
                .modules
                .insert(child.rsplit('.').next().unwrap_or(&child).to_string());
            self.merge_module(&format!("{module}.{child}"), sub);
        }
        let mut names = HashSet::new();
        for (n, mut sig) in ex.fns {
            sig.module = Some(module.to_string());
            names.insert(n.clone());
            self.register_qualified(module, &n, Some(&sig), None);
            self.imported.push(Imported {
                name: n.clone(),
                module: module.to_string(),
                js: n.clone(),
                sig: Some(sig.clone()),
            });
            self.fns.insert(n, sig);
        }
        for (n, mut def) in ex.adts.defs {
            def.module = Some(module.to_string());
            names.insert(n.clone());
            self.adts.defs.insert(n, def);
        }
        self.adts.methods.extend(ex.adts.methods);
        self.adts.trait_impls.extend(ex.adts.trait_impls);
        self.adts.from_impls.extend(ex.adts.from_impls);
        self.adts.op_impls.extend(ex.adts.op_impls);
        self.adts.assoc_consts.extend(ex.adts.assoc_consts);
        self.adts.assoc_const_vals.extend(ex.adts.assoc_const_vals);
        for (k, n, d) in ex.const_needs {
            if !k.contains("::") {
                self.note_const_need(&format!("{module}.{k}"), &n, d);
            }
            self.note_const_need(&k, &n, d);
        }
        for (n, v) in ex.const_vals {
            self.const_vals.insert(format!("{module}.{n}"), v);
            self.const_vals.insert(n, v);
        }
        self.adts.deref_targets.extend(ex.adts.deref_targets);
        self.adts.deref_mut.extend(ex.adts.deref_mut);
        self.adts.iter_items.extend(ex.adts.iter_items);
        self.adts.into_iters.extend(ex.adts.into_iters);
        self.adts.future_outputs.extend(ex.adts.future_outputs);
        self.adts.const_params.extend(ex.adts.const_params);
        self.adts.impl_assoc.extend(ex.adts.impl_assoc);
        for (n, t) in ex.aliases {
            names.insert(n.clone());
            self.type_aliases.insert(n, t);
        }
        for (n, mut def) in ex.adts.traits {
            def.module = Some(module.to_string());
            names.insert(n.clone());
            self.adts.traits.insert(n, def);
        }
        for (n, t) in ex.consts {
            names.insert(n.clone());
            self.register_qualified(module, &n, None, Some(&t));
            self.imported.push(Imported {
                name: n.clone(),
                module: module.to_string(),
                js: n.clone(),
                sig: None,
            });
            self.consts.insert(n, t);
        }
        self.module_exports.insert(module.to_string(), names);
    }

    /// `use util::{a, b as c}`, `use util::*`, `use crate::util::a`: the names must exist; a
    /// renamed function or constant is usable under its new name.
    fn check_use_tree(&mut self, tree: &ast::UseTree, prefix: &[String]) {
        let mut path: Vec<String> = prefix.to_vec();
        if let Some(p) = tree.path() {
            path.extend(
                p.segments()
                    .filter_map(|s| s.name_ref())
                    .map(|n| n.text().to_string()),
            );
        }
        if let Some(list) = tree.use_tree_list() {
            for t in list.use_trees() {
                self.check_use_tree(&t, &path);
            }
            return;
        }
        let rel: Vec<&str> = path
            .iter()
            .map(String::as_str)
            .skip_while(|s| matches!(*s, "crate" | "self"))
            .collect();
        // (What any `use` brings in, for the std-import check, see `require_std_import`.)
        if tree.star_token().is_some() {
            self.glob_imports = true;
        } else if let Some(alias) = tree
            .rename()
            .and_then(|r| r.name())
            .map(|n| n.text().to_string())
            .or_else(|| path.last().cloned())
        {
            self.imported_names.insert(alias);
        }
        match rel.as_slice() {
            // `use std::cmp::max;` / `use std::mem::{swap, take as grab};`: calls of the name are
            // calls of `cmp::max` etc.
            ["std" | "core" | "alloc", module, f]
                if f.starts_with(|ch: char| ch.is_ascii_lowercase()) =>
            {
                let alias = tree
                    .rename()
                    .and_then(|r| r.name())
                    .map(|n| n.text().to_string())
                    .unwrap_or_else(|| f.to_string());
                self.std_fns
                    .insert(alias, vec![module.to_string(), f.to_string()]);
            }
            // `use js::set_timeout;` / `use js::*;`: the `js` crate's functions by name.
            ["js", f] if self.bindings.functions.contains_key(*f) => {
                let alias = tree
                    .rename()
                    .and_then(|r| r.name())
                    .map(|n| n.text().to_string())
                    .unwrap_or_else(|| f.to_string());
                self.js_imports.insert(alias, f.to_string());
            }
            ["js"] if tree.star_token().is_some() => {
                let b = self.bindings.clone();
                for f in b.functions.keys() {
                    self.js_imports.insert(f.clone(), f.clone());
                }
            }
            // The bundled `rand`: its functions by name (`use rand::thread_rng;`, and the ones
            // `use rand::prelude::*;` brings in).
            ["rand", .., f] if super::crates::RAND_FNS.contains(f) => {
                let alias = tree
                    .rename()
                    .and_then(|r| r.name())
                    .map(|n| n.text().to_string())
                    .unwrap_or_else(|| f.to_string());
                self.std_fns
                    .insert(alias, vec!["rand".to_string(), f.to_string()]);
            }
            ["rand", ..] if tree.star_token().is_some() => {
                for f in super::crates::RAND_FNS {
                    self.std_fns
                        .insert(f.to_string(), vec!["rand".to_string(), f.to_string()]);
                }
            }
            // `use std::cell::UnsafeCell;`: a std type by name, modeled or not (an unknown one is
            // then "not supported" rather than "cannot find").
            ["std" | "core" | "alloc", .., t]
                if t.starts_with(|ch: char| ch.is_ascii_uppercase()) =>
            {
                let alias = tree
                    .rename()
                    .and_then(|r| r.name())
                    .map(|n| n.text().to_string())
                    .unwrap_or_else(|| t.to_string());
                self.std_type_imports.insert(alias);
            }
            // The `js` crate (`use js::{Date, JSON};`), serde's derives (accepted; `JSON` goes
            // by types, not traits), and the bundled crates' traits and types.
            [first, ..] if *first == "std" || super::crates::BUNDLED.contains(first) => {}
            ["core" | "alloc", ..] | [] => {}
            [first, rest @ ..] if self.module_exports.contains_key(*first) => {
                // `use geo::shapes::area`: down through `pub mod`s first.
                let mut module = first.to_string();
                let mut rest = rest;
                while let [child, more @ ..] = rest
                    && !more.is_empty()
                    && self
                        .module_exports
                        .contains_key(&format!("{module}.{child}"))
                {
                    module = format!("{module}.{child}");
                    rest = more;
                }
                if let [child] = rest
                    && tree.star_token().is_none()
                    && self
                        .module_exports
                        .contains_key(&format!("{module}.{child}"))
                {
                    // `use geo::shapes;`: the module, usable as `shapes::item`.
                    return;
                }
                let module = module.as_str();
                let exports = self.module_exports[module].clone();
                match rest {
                    [] if tree.star_token().is_some() => {}
                    [] => {}
                    [item] if exports.contains(*item) => {
                        if let Some(alias) = tree.rename().and_then(|r| r.name()) {
                            let alias = alias.text().to_string();
                            if let Some(sig) = self.fns.get(*item).cloned() {
                                self.fns.insert(alias.clone(), sig.clone());
                                self.imported.push(Imported {
                                    name: alias,
                                    module: module.to_string(),
                                    js: item.to_string(),
                                    sig: Some(sig),
                                });
                            } else if let Some(t) = self.consts.get(*item).cloned() {
                                self.consts.insert(alias.clone(), t);
                                self.imported.push(Imported {
                                    name: alias,
                                    module: module.to_string(),
                                    js: item.to_string(),
                                    sig: None,
                                });
                            } else {
                                // (`alias::expand_import_renames` writes the others back.)
                                let msg = format!(
                                    "renaming the imported type `{item}` isn't supported when another `{item}` is in scope (types share one namespace here), when `{alias}` names two items (`#[cfg]` alternatives), or inside a block"
                                );
                                self.error(tree.syntax(), msg);
                            }
                        }
                    }
                    [item] => {
                        let msg = format!(
                            "unresolved import `{}::{item}` (it doesn't exist or isn't `pub`)",
                            module.replace('.', "::")
                        );
                        self.error(tree.syntax(), msg);
                    }
                    _ => self.error(
                        tree.syntax(),
                        "nested module paths in `use` are not supported yet",
                    ),
                }
            }
            [first, ..] => {
                let msg = format!(
                    "unresolved import `{first}` (load a module with `mod {first};`; crates other than std aren't available)"
                );
                self.error(tree.syntax(), msg);
            }
        }
    }

    /// A module's function or constant is also known as `module.name` (not a Rust name, so it
    /// can't clash), which a written path `module::name` resolves to first: a local item of the
    /// same name shadows the bare name, not the path.
    fn register_qualified(&mut self, module: &str, n: &str, sig: Option<&FnSig>, t: Option<&Ty>) {
        let key = format!("{module}.{n}");
        if let Some(sig) = sig {
            self.fns.insert(key.clone(), sig.clone());
        }
        if let Some(t) = t {
            self.consts.insert(key.clone(), t.clone());
        }
        self.imported.push(Imported {
            name: key,
            module: module.to_string(),
            js: n.to_string(),
            sig: sig.cloned(),
        });
    }

    /// `crate::util::x` / `util::x` → `x` when `util` is a loaded module (`util.x` when that's
    /// one of the module's functions or constants).
    pub(super) fn strip_modules(&self, segs: &[String]) -> Vec<String> {
        let mut i = 0;
        while i + 1 < segs.len()
            && (matches!(segs[i].as_str(), "crate" | "self")
                || self.adts.modules.contains(&segs[i]))
        {
            i += 1;
        }
        if let Some(key) = qualified_key(segs, i)
            && (self.fns.contains_key(&key) || self.consts.contains_key(&key))
        {
            return vec![key];
        }
        segs[i..].to_vec()
    }
}

/// `util::x` / `crate::geo::shapes::x` (modules up to `i`) → `util.x` / `geo.shapes.x`.
pub fn qualified_key(segs: &[String], i: usize) -> Option<String> {
    if i + 1 != segs.len() {
        return None;
    }
    let mods: Vec<&str> = segs[..i]
        .iter()
        .map(String::as_str)
        .skip_while(|s| matches!(*s, "crate" | "self"))
        .collect();
    if mods.is_empty() {
        return None;
    }
    Some(format!("{}.{}", mods.join("."), segs[i]))
}
