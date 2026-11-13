//! Modules: `mod name;` loads `name.rs` next to the current file (through the host's resolver),
//! and its public functions, structs, enums, traits and constants become usable here, by `use`
//! or as `name::item`. Each file compiles to its own JS module; this file imports it with
//! `import * as name from "./name.rs"`. A child module of `typeck`.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, SourceFile,
    ast::{self, HasModuleItem, HasName, HasVisibility},
};

use super::{Checker, FnSig};
use crate::adt::Adts;
use crate::macros::EDITION;
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
    /// A module this one loads (directly or not) loads it back.
    circular: Option<String>,
}

/// `dir/of/current.rs` + `name` → `dir/of/name.rs`.
pub fn module_path(current: &str, name: &str) -> String {
    match current.rfind('/') {
        Some(i) => format!("{}/{name}.rs", &current[..i]),
        None => format!("{name}.rs"),
    }
}

/// The public signatures of the module in `text`.
fn exports_of(text: &str, path: &str, resolver: Resolver, stack: &mut Vec<String>) -> Exports {
    let parse = SourceFile::parse(text, EDITION);
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
    for item in file.items() {
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
                }
            }
            ast::Item::Static(s) if public(s.visibility()) => {
                if let Some(n) = s.name()
                    && let Some(t) = c.consts.get(n.text().to_string().as_str())
                {
                    out.consts.insert(n.text().to_string(), t.clone());
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
        Ty::Future(x) => Ty::Future(e(x)),
        Ty::Map(k, v) => Ty::Map(e(k), e(v)),
        Ty::Entry(k, v) => Ty::Entry(e(k), e(v)),
        Ty::Result(k, v) => Ty::Result(e(k), e(v)),
        Ty::Tuple(xs) => Ty::Tuple(xs.iter().map(erase_vars).collect()),
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
            let path = module_path(filename, &n);
            if stack.contains(&path) {
                self.error(
                    m.syntax(),
                    format!("circular modules: `{path}` is already being loaded"),
                );
                continue;
            }
            let Some(text) = resolver(&path) else {
                self.error(
                    m.syntax(),
                    format!("file not found for module `{n}` (looked for `{path}`)"),
                );
                continue;
            };
            stack.push(path.clone());
            let exports = exports_of(&text, &path, resolver, stack);
            stack.pop();
            if let Some(msg) = &exports.circular {
                self.error(m.syntax(), format!("{msg} (through `{path}`)"));
            }
            self.merge_module(&n, exports);
            self.adts.modules.insert(n.clone());
            self.modules.push(ModuleRef {
                specifier: format!("./{n}.rs"),
                name: n,
            });
        }
        for item in crate::adt::all_items(file) {
            if let ast::Item::Use(u) = &item
                && let Some(tree) = u.use_tree()
            {
                self.check_use_tree(&tree, &[]);
            }
        }
    }

    fn merge_module(&mut self, module: &str, ex: Exports) {
        let mut names = HashSet::new();
        for (n, mut sig) in ex.fns {
            sig.module = Some(module.to_string());
            names.insert(n.clone());
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
        for (n, mut def) in ex.adts.traits {
            def.module = Some(module.to_string());
            names.insert(n.clone());
            self.adts.traits.insert(n, def);
        }
        for (n, t) in ex.consts {
            names.insert(n.clone());
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
        match rel.as_slice() {
            ["std" | "core" | "alloc", ..] | [] => {}
            [module, rest @ ..] if self.module_exports.contains_key(*module) => {
                let exports = self.module_exports[*module].clone();
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
                                self.error(
                                    tree.syntax(),
                                    "renaming imported types isn't supported yet",
                                );
                            }
                        }
                    }
                    [item] => {
                        let msg = format!(
                            "unresolved import `{module}::{item}` (it doesn't exist or isn't `pub`)"
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

    /// `crate::util::x` / `util::x` → `x` when `util` is a loaded module.
    pub(super) fn strip_modules(&self, segs: &[String]) -> Vec<String> {
        let mut i = 0;
        while i + 1 < segs.len()
            && (matches!(segs[i].as_str(), "crate" | "self")
                || self.adts.modules.contains(&segs[i]))
        {
            i += 1;
        }
        segs[i..].to_vec()
    }
}
