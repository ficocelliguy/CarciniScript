//! Inline modules (`mod util { .. }`) and `self::` / `super::` / `crate::` paths, as a pre-pass
//! over the source text: every item of an inline module moves to the top level under a mangled
//! name (`util__double`, `a__b__f` when nested), the `mod` wrappers go away, and each path is
//! resolved in its own module's scope and rewritten to the mangled name: `util::double`,
//! `self::f`, `super::f`, `crate::a::b::f`, names a `use` (also `as` and `*`) brings in, and
//! unqualified names inside the module. The result is one flat file the rest of the compiler
//! already handles; an `OffsetMap` keeps positions pointing at the original text.
//!
//! `#[cfg(test)]` modules are left alone (they're dropped later). Items nested inside function
//! bodies keep their names.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, SyntaxToken,
    ast::{self, HasModuleItem, HasName, HasVisibility},
};

use crate::macros::EDITION;
use crate::mbe::OffsetMap;

/// What a `use` brings into a module: an item of some module, or a whole module.
#[derive(Clone, Debug)]
enum Target {
    Item(Vec<String>, String),
    Module(Vec<String>),
}

#[derive(Default, Debug)]
struct Module {
    items: HashSet<String>,
    children: HashSet<String>,
    uses: HashMap<String, Target>,
    globs: Vec<Vec<String>>,
    /// Items and child modules without `pub`.
    private: HashSet<String>,
    /// Names other `use`s bring in (`use std::fmt;`), and whether one is a `*`.
    external: HashSet<String>,
    external_glob: bool,
    /// Whether some of its names can't be known here (a macro call or `mod x;` among its items,
    /// or a `#[cfg]`'d twin): then missing or private names aren't reported.
    open: bool,
}

/// How a path comes out.
enum Res {
    /// Leave it as written.
    Keep,
    /// Replace its first `n + 1` segments with this name.
    Rewrite(usize, String),
    Error(String),
}

fn has_pub(n: &SyntaxNode) -> bool {
    n.children().any(|c| c.kind() == SyntaxKind::VISIBILITY)
}

fn has_cfg(n: &SyntaxNode) -> bool {
    n.children()
        .filter_map(ast::Attr::cast)
        .any(|a| a.syntax().text().to_string().contains("cfg"))
}

/// The mangled top-level name of item `name` of module `path`.
fn mangle(path: &[String], name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{}__{name}", path.join("__"))
    }
}

/// An inline module that isn't test-only.
fn inline_module(m: &ast::Module) -> bool {
    m.item_list().is_some() && !crate::adt::test_only(m.syntax())
}

/// The inline modules enclosing `n`, outermost first.
fn module_path(n: &SyntaxNode) -> Vec<String> {
    let mut out: Vec<String> = n
        .ancestors()
        .filter_map(ast::Module::cast)
        .filter(inline_module)
        .filter_map(|m| m.name().map(|x| x.text().to_string()))
        .collect();
    out.reverse();
    out
}

fn item_name(item: &ast::Item) -> Option<ast::Name> {
    match item {
        ast::Item::Fn(x) => x.name(),
        ast::Item::Struct(x) => x.name(),
        ast::Item::Enum(x) => x.name(),
        ast::Item::Trait(x) => x.name(),
        ast::Item::Const(x) => x.name(),
        ast::Item::Static(x) => x.name(),
        ast::Item::TypeAlias(x) => x.name(),
        _ => None,
    }
}

struct Flattener {
    modules: HashMap<Vec<String>, Module>,
}

impl Flattener {
    fn collect(&mut self, items: impl Iterator<Item = ast::Item>, path: Vec<String>, open: bool) {
        let mut module = Module {
            open,
            ..Module::default()
        };
        let mut inner = Vec::new();
        for item in items {
            if let ast::Item::Module(m) = &item
                && inline_module(m)
                && let Some(n) = m.name()
            {
                if !module.children.insert(n.text().to_string()) || has_cfg(m.syntax()) {
                    module.open = true;
                }
                if !has_pub(m.syntax()) {
                    module.private.insert(n.text().to_string());
                }
                let mut p = path.clone();
                p.push(n.text().to_string());
                inner.push((m.clone(), p, has_cfg(m.syntax())));
                continue;
            }
            if let Some(n) = item_name(&item) {
                module.items.insert(n.text().to_string());
                if !has_pub(item.syntax()) {
                    module.private.insert(n.text().to_string());
                }
            } else if !matches!(item, ast::Item::Impl(_) | ast::Item::Use(_)) {
                module.open = true;
            }
        }
        // Two modules of one name (under different `#[cfg]`s): keep what both have.
        if let Some(old) = self.modules.remove(&path) {
            module.items.extend(old.items);
            module.children.extend(old.children);
            module.open = true;
        }
        self.modules.insert(path, module);
        for (m, p, open) in inner {
            let items: Vec<ast::Item> = m
                .item_list()
                .map(|l| l.items().collect())
                .unwrap_or_default();
            self.collect(items.into_iter(), p, open);
        }
    }

    /// Where a path starts, after `crate::` / `self::` / `super::`, and the segments left.
    fn start<'a>(
        &self,
        from: &[String],
        segs: &'a [String],
    ) -> Option<(Vec<String>, &'a [String], bool)> {
        match segs.first().map(String::as_str) {
            Some("crate") => Some((Vec::new(), &segs[1..], true)),
            Some("self") => Some((from.to_vec(), &segs[1..], true)),
            Some("super") => {
                let mut p = from.to_vec();
                let mut i = 0;
                while segs.get(i).map(String::as_str) == Some("super") {
                    p.pop()?;
                    i += 1;
                }
                Some((p, &segs[i..], true))
            }
            _ => Some((from.to_vec(), segs, false)),
        }
    }

    /// Walk module names from `start`: the module reached and the index of the first segment that
    /// isn't a module.
    fn walk(&self, start: Vec<String>, rest: &[String]) -> (Vec<String>, usize) {
        let mut cur = start;
        let mut i = 0;
        while i < rest.len() {
            let Some(m) = self.modules.get(&cur) else {
                break;
            };
            if m.children.contains(&rest[i]) {
                cur.push(rest[i].clone());
                i += 1;
                continue;
            }
            // `use a::b;` then `b::f`
            if let Some(Target::Module(p)) = m.uses.get(&rest[i]) {
                cur = p.clone();
                i += 1;
                continue;
            }
            break;
        }
        (cur, i)
    }

    /// Resolve the path `segs` written in module `from`; `local` says whether a single name is a
    /// local variable there.
    fn resolve(
        &self,
        from: &[String],
        segs: &[String],
        missing: bool,
        local: impl Fn(&str) -> bool,
    ) -> Res {
        let Some((start, rest, explicit)) = self.start(from, segs) else {
            return Res::Keep;
        };
        if rest.is_empty() {
            return Res::Keep;
        }
        // This file may be a module of a bigger crate, where `crate::` means something else:
        // such paths are rewritten when they fit, but never reported.
        let mut checked = segs.first().is_none_or(|s| s != "crate");
        // Each module crossed must be visible from `from` (public, or an ancestor's child).
        let mut cur = start.clone();
        let (module, i) = self.walk(start, rest);
        for (k, seg) in rest[..i].iter().enumerate() {
            if !missing && k + 1 == rest.len() {
                break;
            }
            let Some(m) = self.modules.get(&cur) else {
                break;
            };
            if m.children.contains(seg) {
                if checked && m.private.contains(seg) && !m.open && !from.starts_with(&cur) {
                    return Res::Error(format!("module `{seg}` is private"));
                }
                cur.push(seg.clone());
            } else if let Some(Target::Module(p)) = m.uses.get(seg) {
                // (A `use crate::..` alias may mean a module of a bigger crate.)
                checked &= !m.external.contains(seg);
                cur = p.clone();
            }
        }
        if i >= rest.len() {
            return Res::Keep;
        }
        let name = &rest[i];
        if !explicit && i == 0 && rest.len() == 1 && local(name) {
            return Res::Keep;
        }
        let skipped = segs.len() - rest.len() + i;
        let Some(mangled) = self.lookup(&module, name, 0) else {
            // A path into one of our inline modules must name something there. (The file itself
            // may not be the crate root, so `crate::` and `super::` names there aren't checked.)
            if missing
                && checked
                && (explicit || i > 0)
                && !module.is_empty()
                && let Some(m) = self.modules.get(&module)
                && !m.open
                && !m.children.contains(name)
                && !m.uses.contains_key(name)
                && !m.external.contains(name)
                && !m.external_glob
            {
                let place = format!("module `{}`", module.join("::"));
                return Res::Error(format!("cannot find `{name}` in {place}"));
            }
            return Res::Keep;
        };
        if let Some(m) = self.modules.get(&module)
            && m.items.contains(name)
            && m.private.contains(name)
            && checked
            && !m.open
            && !from.starts_with(&module)
        {
            return Res::Error(format!(
                "`{name}` is private to module `{}`",
                module.join("::")
            ));
        }
        if mangled == *name && skipped == 0 {
            return Res::Keep;
        }
        Res::Rewrite(skipped, mangled)
    }

    /// The mangled name `name` stands for in module `path` (its own items, then `use`s).
    fn lookup(&self, path: &[String], name: &str, depth: usize) -> Option<String> {
        if depth > 16 {
            return None;
        }
        let m = self.modules.get(path)?;
        if m.items.contains(name) {
            return Some(mangle(path, name));
        }
        match m.uses.get(name) {
            Some(Target::Item(p, n)) => {
                return self.lookup(p, n, depth + 1).or_else(|| Some(mangle(p, n)));
            }
            Some(Target::Module(_)) => return None,
            None => {}
        }
        for g in &m.globs {
            if let Some(x) = self.lookup(g, name, depth + 1) {
                return Some(x);
            }
        }
        None
    }

    /// Record what `use` items in `path` bring in; returns the uses that change, with their new
    /// text: none for ours (they go away), and the rest rewritten where they reach into one of
    /// our modules (`use shapes::Kind::*` becomes `use shapes__Kind::*`).
    fn uses(
        &mut self,
        items: &[ast::Item],
        path: &[String],
        errors: &mut Vec<(usize, usize, String)>,
    ) -> Vec<(ast::Use, String)> {
        let mut consumed = Vec::new();
        for item in items {
            let ast::Item::Use(u) = item else { continue };
            let mut ours = true;
            let mut rewritten = false;
            // The `use` leaves kept, as written or rewritten.
            let mut kept: Vec<String> = Vec::new();
            for (mut segs, rename, star) in crate::typeck::use_leaves(u) {
                // `use a::b::{self as c}` names the module `a::b`.
                if segs.len() > 1 && segs.last().is_some_and(|s| s == "self") {
                    segs.pop();
                }
                let leaf = |mut parts: Vec<String>| {
                    if star {
                        parts.push("*".into());
                    }
                    let mut t = parts.join("::");
                    if let Some(r) = &rename {
                        t = format!("{t} as {r}");
                    }
                    t
                };
                kept.push(leaf(segs.clone()));
                let external = |f: &mut Self| {
                    let m = f.modules.entry(path.to_vec()).or_default();
                    if star {
                        m.external_glob = true;
                    } else if let Some(n) = rename.clone().or_else(|| segs.last().cloned()) {
                        m.external.insert(n);
                    }
                };
                let Some((start, rest, explicit)) = self.start(path, &segs) else {
                    ours = false;
                    external(self);
                    continue;
                };
                let (module, i) = self.walk(start, rest);
                let into_module = i > 0 || explicit;
                if !into_module {
                    ours = false;
                    external(self);
                    continue;
                }
                if segs.first().is_some_and(|s| s == "crate") {
                    external(self);
                }
                if !star && let Res::Error(e) = self.resolve(path, &segs, false, |_| false) {
                    let r = u.syntax().text_range();
                    errors.push((r.start().into(), r.end().into(), e));
                }
                let m = self.modules.entry(path.to_vec()).or_default();
                if star && i == rest.len() {
                    m.globs.push(module);
                } else if !star && i + 1 == rest.len() {
                    let name = rest[i].clone();
                    let alias = rename.unwrap_or_else(|| name.clone());
                    m.uses.insert(alias, Target::Item(module, name));
                } else if i == rest.len() && !star {
                    let alias = rename.unwrap_or_else(|| segs.last().cloned().unwrap_or_default());
                    m.uses.insert(alias, Target::Module(module));
                } else {
                    // Through our modules to an item, then on (`shapes::Kind::Round`).
                    ours = false;
                    rewritten = true;
                    external(self);
                    let name = &rest[i];
                    let mut parts = vec![
                        self.lookup(&module, name, 0)
                            .unwrap_or_else(|| mangle(&module, name)),
                    ];
                    parts.extend(rest[i + 1..].iter().cloned());
                    *kept.last_mut().unwrap() = leaf(parts);
                    continue;
                }
                kept.pop();
            }
            if ours {
                consumed.push((u.clone(), String::new()));
            } else if rewritten {
                let vis = u.visibility().map(|v| format!("{v} ")).unwrap_or_default();
                let text = kept
                    .iter()
                    .map(|k| format!("{vis}use {k};"))
                    .collect::<Vec<_>>()
                    .join(" ");
                consumed.push((u.clone(), text));
            }
        }
        consumed
    }
}

/// The result of flattening: the new text replaces the old (positions through `map`), and
/// `names` maps each mangled type name back to the one written, for `{:?}`.
pub struct Flattened {
    pub names: HashMap<String, String>,
    /// Paths that name a private or missing item of an inline module (positions in the input).
    pub errors: Vec<(usize, usize, String)>,
}

pub fn has_inline_modules(text: &str) -> bool {
    if !text.contains("mod ") && !text.contains("super::") && !text.contains("self::") {
        return false;
    }
    let file = SourceFile::parse(text, EDITION).tree();
    file.syntax()
        .descendants()
        .filter_map(ast::Module::cast)
        .any(|m| inline_module(&m))
        || file
            .syntax()
            .descendants()
            .filter_map(ast::Path::cast)
            .any(|p| p.syntax().text().to_string().starts_with("self::"))
}

/// Flatten `text` in place, recording the edits in `map`.
pub fn flatten(text: &mut String, map: &mut OffsetMap) -> Flattened {
    let parsed = SourceFile::parse(text, EDITION);
    // In a file that doesn't parse, module errors would only be noise.
    let broken = !parsed.errors().is_empty();
    let file = parsed.tree();
    let mut f = Flattener {
        modules: HashMap::new(),
    };
    let root_items: Vec<ast::Item> = file.items().collect();
    f.collect(root_items.iter().cloned(), Vec::new(), false);
    // `use`s, per module (after every module's items are known).
    let mut errors = Vec::new();
    let mut removed: Vec<(ast::Use, String)> = f.uses(&root_items, &[], &mut errors);
    for m in file
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
        .filter(inline_module)
    {
        // (A module's own node is among its ancestors, so this includes its name.)
        let path = module_path(m.syntax());
        let items: Vec<ast::Item> = m
            .item_list()
            .map(|l| l.items().collect())
            .unwrap_or_default();
        removed.extend(f.uses(&items, &path, &mut errors));
    }

    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let range = |n: &SyntaxNode| -> (usize, usize) {
        let r = n.text_range();
        (r.start().into(), r.end().into())
    };
    let mut names = HashMap::new();
    // Item names in inline modules.
    for m in file
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
        .filter(inline_module)
    {
        let path = module_path(m.syntax());
        for item in m.item_list().into_iter().flat_map(|l| l.items()) {
            if let Some(n) = item_name(&item) {
                let new = mangle(&path, n.text());
                names.insert(new.clone(), n.text().to_string());
                let (a, b) = range(n.syntax());
                edits.push((a, b, new));
            }
        }
        // Drop the wrapper: `mod util {` .. `}`.
        if let Some(list) = m.item_list()
            && let (Some(l), Some(r)) = (list.l_curly_token(), list.r_curly_token())
        {
            let (ma, mb) = range(m.syntax());
            edits.push((ma, l.text_range().end().into(), String::new()));
            edits.push((r.text_range().start().into(), mb, String::new()));
        }
    }
    for (u, new) in &removed {
        let (a, b) = range(u.syntax());
        edits.push((a, b, new.clone()));
    }
    // Paths.
    for p in file.syntax().descendants().filter_map(ast::Path::cast) {
        let n = p.syntax();
        if n.parent().is_some_and(|x| ast::Path::can_cast(x.kind()))
            || n.ancestors().any(|a| {
                ast::Use::can_cast(a.kind())
                    || ast::Attr::can_cast(a.kind())
                    || ast::MacroCall::can_cast(a.kind())
            })
            || crate::adt::test_only(n)
            || removed
                .iter()
                .any(|(u, _)| u.syntax().text_range().contains_range(n.text_range()))
        {
            continue;
        }
        let segs: Vec<ast::PathSegment> = p.segments().collect();
        let Some(names_seg): Option<Vec<String>> = segs
            .iter()
            .map(|s| {
                s.name_ref()
                    .map(|n| n.text().to_string())
                    .or_else(|| s.self_token().map(|_| "self".into()))
                    .or_else(|| s.super_token().map(|_| "super".into()))
                    .or_else(|| s.crate_token().map(|_| "crate".into()))
            })
            .collect()
        else {
            continue;
        };
        // A single name inside a function may be a local, which wins over items.
        let (skipped, mangled) =
            match f.resolve(&module_path(n), &names_seg, true, |name| is_local(n, name)) {
                Res::Keep => continue,
                Res::Error(e) => {
                    let (a, b) = range(n);
                    errors.push((a, b, e));
                    continue;
                }
                Res::Rewrite(k, m) => (k, m),
            };
        // Replace from the path's start through the name (keeping generics and what follows).
        let first = &segs[0];
        let Some(last) = segs.get(skipped).and_then(|s| s.name_ref()) else {
            continue;
        };
        let a: usize = first.syntax().text_range().start().into();
        let b: usize = last.syntax().text_range().end().into();
        edits.push((a, b, mangled));
    }
    // Paths written as tokens inside macro arguments (`format!("{}", geo::UNIT)`).
    for tt in file
        .syntax()
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::TOKEN_TREE)
    {
        if crate::adt::test_only(&tt)
            || tt
                .ancestors()
                .any(|a| ast::MacroRules::can_cast(a.kind()) || ast::Attr::can_cast(a.kind()))
        {
            continue;
        }
        let toks: Vec<SyntaxToken> = tt
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
            .collect();
        let is_name = |t: &SyntaxToken| {
            matches!(
                t.kind(),
                SyntaxKind::IDENT
                    | SyntaxKind::SELF_KW
                    | SyntaxKind::SUPER_KW
                    | SyntaxKind::CRATE_KW
            )
        };
        // `::` is one token or two `:`s; returns the index after it.
        let sep = |i: usize| -> Option<usize> {
            match toks.get(i)?.text() {
                "::" => Some(i + 1),
                ":" if toks.get(i + 1)?.text() == ":" => Some(i + 2),
                _ => None,
            }
        };
        let mut i = 0;
        while i < toks.len() {
            if !is_name(&toks[i])
                || (i > 0
                    && (toks[i - 1].text() == "."
                        || sep(i - 1).is_some()
                        || (i > 1 && sep(i - 2).is_some())))
            {
                i += 1;
                continue;
            }
            // A run `a::b::c`.
            let mut run = vec![i];
            let mut j = i + 1;
            while let Some(k) = sep(j) {
                if toks.get(k).is_some_and(is_name) {
                    run.push(k);
                    j = k + 1;
                } else {
                    break;
                }
            }
            let next = toks
                .get(j)
                .map(|t| t.text().to_string())
                .unwrap_or_default();
            let seg_names: Vec<String> = run.iter().map(|&k| toks[k].text().to_string()).collect();
            i = j;
            // `name = value` in a format macro is an argument name; `name!` a macro.
            if next == "!" || (next == "=" && toks.get(j + 1).is_none_or(|t| t.text() != "=")) {
                continue;
            }
            let a: usize = toks[run[0]].text_range().start().into();
            let (skipped, mangled) = match f.resolve(&module_path(&tt), &seg_names, true, |name| {
                is_local(&tt, name)
            }) {
                Res::Keep => continue,
                Res::Error(e) => {
                    errors.push((a, toks[run[run.len() - 1]].text_range().end().into(), e));
                    continue;
                }
                Res::Rewrite(k, m) => (k, m),
            };
            let b: usize = toks[run[skipped]].text_range().end().into();
            edits.push((a, b, mangled));
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    // Nested edits (a path inside a removed wrapper range never overlaps: wrappers only cover
    // `mod x {` and `}`), so apply from the end.
    let mut last_start = usize::MAX;
    for (a, b, new) in edits {
        if b > last_start {
            continue;
        }
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
        last_start = a;
    }
    if broken {
        errors.clear();
    }
    Flattened { names, errors }
}

/// Whether `name` is bound by a pattern or parameter in the function around `n`.
fn is_local(n: &SyntaxNode, name: &str) -> bool {
    let Some(f) = n
        .ancestors()
        .find(|a| ast::Fn::can_cast(a.kind()) || ast::ClosureExpr::can_cast(a.kind()))
    else {
        return false;
    };
    let f = f
        .ancestors()
        .filter(|a| ast::Fn::can_cast(a.kind()))
        .last()
        .unwrap_or(f);
    f.descendants()
        .filter_map(ast::IdentPat::cast)
        .any(|ip| ip.name().is_some_and(|x| x.text() == name))
}
