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
    AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, TextSize,
    ast::{self, HasLoopBody, HasModuleItem, HasName, HasVisibility},
};

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
    /// Items and child modules that aren't `pub`, by the module path they're visible from (and
    /// below): their own module, its parent for `pub(super)`, the one `pub(in ..)` names.
    private: HashMap<String, Vec<String>>,
    /// Names other `use`s bring in (`use std::fmt;`), and whether one is a `*`.
    external: HashSet<String>,
    external_glob: bool,
    /// Its traits (by name), for `TraitScopes`.
    traits: HashSet<String>,
    /// What `use .. as _;` brings in (only traits matter: their methods).
    anon: Vec<(Vec<String>, String)>,
    /// Whether a macro call among its items may write more `use`s.
    item_macros: bool,
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

/// Where an item of module `path` can be used from (that module path and below), or None when
/// it's `pub` / `pub(crate)` (the file is the crate).
fn visible_from(n: &SyntaxNode, path: &[String]) -> Option<Vec<String>> {
    let Some(v) = n.children().find_map(ast::Visibility::cast) else {
        return Some(path.to_vec());
    };
    let text: String = v.syntax().text().to_string().split_whitespace().collect();
    match text.as_str() {
        "pub" | "pub(crate)" => None,
        "pub(self)" => Some(path.to_vec()),
        "pub(super)" => Some(path[..path.len().saturating_sub(1)].to_vec()),
        t => {
            // `pub(in crate::a::b)`, `pub(in super)`, `pub(in self::x)`.
            let inner = t.strip_prefix("pub(in")?.strip_suffix(')')?;
            // (`crate::` may be a bigger crate than this file: not checked, like `crate::` paths.)
            if inner.starts_with("crate") {
                return None;
            }
            let mut out = path.to_vec();
            for (i, seg) in inner.split("::").enumerate() {
                match seg {
                    "self" if i == 0 => {}
                    "super" => {
                        out.pop();
                    }
                    s => out.push(s.to_string()),
                }
            }
            Some(out)
        }
    }
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
                if let Some(scope) = visible_from(m.syntax(), &path) {
                    module.private.insert(n.text().to_string(), scope);
                }
                let mut p = path.clone();
                p.push(n.text().to_string());
                inner.push((m.clone(), p, has_cfg(m.syntax())));
                continue;
            }
            if let Some(n) = item_name(&item) {
                module.items.insert(n.text().to_string());
                if matches!(item, ast::Item::Trait(_)) {
                    module.traits.insert(n.text().to_string());
                }
                if let Some(scope) = visible_from(item.syntax(), &path) {
                    module.private.insert(n.text().to_string(), scope);
                }
            } else if !matches!(item, ast::Item::Impl(_) | ast::Item::Use(_)) {
                module.open = true;
                module.item_macros |= matches!(item, ast::Item::MacroCall(_));
            }
        }
        // Two modules of one name (under different `#[cfg]`s): keep what both have.
        if let Some(old) = self.modules.remove(&path) {
            module.items.extend(old.items);
            module.traits.extend(old.traits);
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
                if checked
                    && m.private
                        .get(seg)
                        .is_some_and(|scope| !from.starts_with(scope))
                    && !m.open
                {
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
            // `prelude::Point` with `mod prelude { pub use crate::geo::Point; }` (an item of a
            // `mod` file, or of std): the `use` is hoisted out with the module's items, so the
            // name alone is it.
            if (explicit || i > 0)
                && let Some(m) = self.modules.get(&module)
                && m.external.contains(name)
            {
                return Res::Rewrite(skipped, name.clone());
            }
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
            && m.private
                .get(name)
                .is_some_and(|scope| !from.starts_with(scope))
            && checked
            && !m.open
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
                    if alias == "_" {
                        m.anon.push((module, name));
                        kept.pop();
                        continue;
                    }
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
    let file = crate::macros::parse_tree(text);
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
    let parsed = crate::macros::parse_file(text);
    // In a file that doesn't parse, module errors would only be noise.
    let broken = !parsed.errors().is_empty();
    let file = parsed.tree();
    let Scan {
        f,
        mut errors,
        removed,
        block_uses,
    } = scan(&file);

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
            // (And its `//!` docs and `#![..]` attributes, which can't follow other items.)
            // (The parser hangs comments on the item after them.)
            let ours = |t: &SyntaxToken| {
                t.parent_ancestors()
                    .find(|a| a.kind() == SyntaxKind::ITEM_LIST)
                    .is_some_and(|l| &l == list.syntax())
            };
            for e in list.syntax().descendants_with_tokens() {
                let inner = match &e {
                    NodeOrToken::Token(t) => t.kind() == SyntaxKind::INNER_DOC_COMMENT && ours(t),
                    NodeOrToken::Node(n) if n.parent().as_ref() != Some(list.syntax()) => false,
                    NodeOrToken::Node(n) => {
                        ast::Attr::cast(n.clone()).is_some_and(|a| a.excl_token().is_some())
                    }
                };
                if inner {
                    let r = e.text_range();
                    edits.push((r.start().into(), r.end().into(), String::new()));
                }
            }
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
        // A name a block's own `use` brings in (the innermost block's).
        if let [name] = names_seg.as_slice()
            && !is_local(n, n.text_range().start(), name)
            && let Some((_, _, mangled)) = block_uses
                .iter()
                .filter(|(r, alias, _)| alias == name && r.contains_range(n.text_range()))
                .min_by_key(|(r, _, _)| r.len())
        {
            if mangled != name
                && let Some(nr) = segs[0].name_ref()
            {
                let (a, b) = range(nr.syntax());
                edits.push((a, b, mangled.clone()));
            }
            continue;
        }
        // A single name inside a function may be a local, which wins over items.
        let (skipped, mangled) = match f.resolve(&module_path(n), &names_seg, true, |name| {
            is_local(n, n.text_range().start(), name)
        }) {
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
            if next == "!"
                || (next == "="
                    && toks
                        .get(j + 1)
                        .is_none_or(|t| !matches!(t.text(), "=" | ">")))
            {
                continue;
            }
            let a: usize = toks[run[0]].text_range().start().into();
            let (skipped, mangled) = match f.resolve(&module_path(&tt), &seg_names, true, |name| {
                is_local(&tt, toks[run[0]].text_range().start(), name)
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

/// Whether `name` is bound by a pattern or parameter in the function around `n` that is in scope
/// at `at` (`let ln = ln(x)?;` calls the item `ln`).
fn is_local(n: &SyntaxNode, at: TextSize, name: &str) -> bool {
    // An item of a block around it (`fn f() { struct Formatter { .. } .. }`), which wins over
    // the module's names and imports.
    let block_item = n
        .ancestors()
        .filter(|a| a.kind() == SyntaxKind::STMT_LIST)
        .flat_map(|b| b.children().filter_map(ast::Item::cast))
        .any(|item| match item {
            ast::Item::Struct(s) => s.name().is_some_and(|x| x.text() == name),
            ast::Item::Enum(e) => e.name().is_some_and(|x| x.text() == name),
            ast::Item::Fn(f) => f.name().is_some_and(|x| x.text() == name),
            ast::Item::Trait(t) => t.name().is_some_and(|x| x.text() == name),
            ast::Item::TypeAlias(t) => t.name().is_some_and(|x| x.text() == name),
            ast::Item::Const(c) => c.name().is_some_and(|x| x.text() == name),
            ast::Item::Static(s) => s.name().is_some_and(|x| x.text() == name),
            _ => false,
        });
    if block_item {
        return true;
    }
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
        .filter(|ip| ip.name().is_some_and(|x| x.text() == name))
        .any(|ip| binding_in_scope(ip.syntax(), at))
}

/// Whether the binding `pat` is visible at `at`. Unknown binders count as visible.
fn binding_in_scope(pat: &SyntaxNode, at: TextSize) -> bool {
    let inside = |n: Option<SyntaxNode>| n.is_some_and(|n| n.text_range().contains(at));
    for a in pat.ancestors().skip(1) {
        if let Some(l) = ast::LetStmt::cast(a.clone()) {
            // After the statement, in the same block.
            return l.syntax().text_range().end() <= at && inside(l.syntax().parent());
        }
        if ast::Param::can_cast(a.kind()) || ast::ClosureExpr::can_cast(a.kind()) {
            let body = a.ancestors().find_map(|b| {
                ast::Fn::cast(b.clone())
                    .and_then(|f| f.body().map(|b| b.syntax().clone()))
                    .or_else(|| {
                        ast::ClosureExpr::cast(b).and_then(|c| c.body().map(|b| b.syntax().clone()))
                    })
            });
            return inside(body);
        }
        if let Some(arm) = ast::MatchArm::cast(a.clone()) {
            return inside(arm.guard().map(|g| g.syntax().clone()))
                || inside(arm.expr().map(|e| e.syntax().clone()));
        }
        if let Some(fe) = ast::ForExpr::cast(a.clone()) {
            return inside(fe.loop_body().map(|b| b.syntax().clone()));
        }
        if let Some(le) = ast::LetExpr::cast(a.clone()) {
            // `if let` / `while let` (also in `&&` chains): after the `let`, before any `else`.
            let Some(owner) = le
                .syntax()
                .ancestors()
                .find(|b| ast::IfExpr::can_cast(b.kind()) || ast::WhileExpr::can_cast(b.kind()))
            else {
                return true;
            };
            let in_else = ast::IfExpr::cast(owner.clone())
                .and_then(|i| i.else_branch())
                .is_some_and(|e| e.syntax().text_range().contains(at));
            return le.syntax().text_range().end() <= at && inside(Some(owner)) && !in_else;
        }
    }
    true
}

/// What `flatten` learns before it edits: the modules, their `use`s (with those it removes or
/// rewrites), errors, and the aliases block-level `use`s bring in.
struct Scan {
    f: Flattener,
    errors: Vec<(usize, usize, String)>,
    removed: Vec<(ast::Use, String)>,
    block_uses: Vec<(ra_ap_syntax::TextRange, String, String)>,
}

fn scan(file: &ast::SourceFile) -> Scan {
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
    // `use`s inside function bodies (`fn main() { use geo::Point as P; .. }`): the alias is
    // recorded with the module's (its scope is wider than the block, which is close enough).
    // In that block it also wins over the module's own items (`use deeply::function;` there
    // shadows the outer `fn function`).
    let mut block_uses: Vec<(ra_ap_syntax::TextRange, String, String)> = Vec::new();
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        let Some(block) = u
            .syntax()
            .parent()
            .filter(|p| p.kind() == SyntaxKind::STMT_LIST)
        else {
            continue;
        };
        let path = module_path(u.syntax());
        for (segs, rename, star) in crate::typeck::use_leaves(&u) {
            if star {
                continue;
            }
            let Some((start, rest, explicit)) = f.start(&path, &segs) else {
                continue;
            };
            let (module, i) = f.walk(start, rest);
            if (i > 0 || explicit) && i + 1 == rest.len() {
                let name = &rest[i];
                let mangled = f
                    .lookup(&module, name, 0)
                    .unwrap_or_else(|| mangle(&module, name));
                let alias = rename.unwrap_or_else(|| name.clone());
                block_uses.push((block.text_range(), alias, mangled));
            }
        }
        removed.extend(f.uses(&[ast::Item::Use(u)], &path, &mut errors));
    }
    Scan {
        f,
        errors,
        removed,
        block_uses,
    }
}

/// Which of a file's traits are in scope where, for the rule that a trait's methods can only be
/// called with the trait in scope (`traitscope`): a trait is in scope in its own module, and
/// where a `use` (also `as _` and `*`) brings it in. A block's `use` counts for its whole module,
/// as in `flatten`.
pub struct TraitScopes {
    f: Flattener,
    file: ast::SourceFile,
}

/// The scopes of `text` (the program as written), or `None` when it has no inline modules: then
/// every trait it declares is in scope everywhere.
pub fn trait_scopes(text: &str) -> Option<TraitScopes> {
    if !has_inline_modules(text) {
        return None;
    }
    let parsed = crate::macros::parse_file(text);
    if !parsed.errors().is_empty() {
        return None;
    }
    let file = parsed.tree();
    let Scan { f, .. } = scan(&file);
    Some(TraitScopes { f, file })
}

impl TraitScopes {
    /// The module path of the code at `at`.
    fn module_at(&self, at: usize) -> Option<Vec<String>> {
        let tok = self
            .file
            .syntax()
            .token_at_offset(TextSize::new(u32::try_from(at).ok()?))
            .right_biased()?;
        Some(module_path(&tok.parent()?))
    }

    /// The traits (mangled names) in scope at `at`; `None` when that can't be known (a macro
    /// call among the module's items may write a `use`).
    pub fn at(&self, at: usize) -> Option<HashSet<String>> {
        let path = self.module_at(at)?;
        if self.f.modules.get(&path)?.item_macros {
            return None;
        }
        let mut out = HashSet::new();
        self.visible(&path, &mut out, 0);
        Some(out)
    }

    /// Every name module `path` sees, mangled: its own items, what its `use`s name, and through
    /// its `*` imports, theirs (more than a glob brings in, which only lets more calls through).
    fn visible(&self, path: &[String], out: &mut HashSet<String>, depth: usize) {
        let Some(m) = self.f.modules.get(path) else { return };
        if depth > 16 {
            return;
        }
        out.extend(m.items.iter().map(|n| mangle(path, n)));
        for t in m.uses.values() {
            if let Target::Item(p, n) = t {
                out.insert(self.f.lookup(p, n, 0).unwrap_or_else(|| mangle(p, n)));
            }
        }
        for (p, n) in &m.anon {
            out.insert(self.f.lookup(p, n, 0).unwrap_or_else(|| mangle(p, n)));
        }
        for g in &m.globs {
            self.visible(g, out, depth + 1);
        }
    }

    /// The trait whose mangled name is `mangled`: its module path and name.
    pub fn trait_path(&self, mangled: &str) -> Option<(Vec<String>, String)> {
        self.f.modules.iter().find_map(|(p, m)| {
            m.traits
                .iter()
                .find(|t| mangle(p, t) == mangled)
                .map(|t| (p.clone(), t.clone()))
        })
    }

    /// Where rustc suggests a `use` for code at `at`: before its module's first `use`, or else
    /// its first item.
    pub fn use_site(&self, at: usize) -> Option<usize> {
        let tok = self
            .file
            .syntax()
            .token_at_offset(TextSize::new(u32::try_from(at).ok()?))
            .right_biased()?;
        let items: Vec<ast::Item> = match tok
            .parent()?
            .ancestors()
            .filter_map(ast::Module::cast)
            .find(inline_module)
        {
            Some(m) => m.item_list()?.items().collect(),
            None => self.file.items().collect(),
        };
        let first = items
            .iter()
            .find(|i| matches!(i, ast::Item::Use(_)))
            .or_else(|| items.first())?;
        // (After its doc comments and attributes.)
        let start = first
            .syntax()
            .children_with_tokens()
            .find(|c| {
                !matches!(
                    c.kind(),
                    SyntaxKind::COMMENT | SyntaxKind::WHITESPACE | SyntaxKind::ATTR
                )
            })
            .map(|c| c.text_range().start())
            .unwrap_or_else(|| first.syntax().text_range().start());
        Some(start.into())
    }
}
