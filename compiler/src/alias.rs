//! `type` aliases, as a pre-pass over the source text: each use of an alias (at the top of the
//! file, or inside a block for that block: `type Grid<T> = Vec<Vec<T>>;`) is replaced by the type it stands for, with its generic
//! arguments filled in, and the alias items go away. In expressions and patterns an alias of a
//! path type is written with turbofish (`Pt::new()` → `Point::new()`, `Ints::new()` →
//! `Vec::<i32>::new()`). An `OffsetMap` keeps positions pointing at the original text.

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, TextRange,
    ast::{self, HasGenericArgs, HasGenericParams, HasModuleItem, HasName},
};

use crate::macros::EDITION;
use crate::mbe::OffsetMap;

struct Alias {
    name: String,
    params: Vec<String>,
    body: ast::Type,
    item: SyntaxNode,
    /// Where it can be used: the file, or the block it's declared in.
    scope: TextRange,
}

fn aliases(file: &SourceFile) -> Vec<Alias> {
    let mut out = Vec::new();
    let local = file
        .syntax()
        .descendants()
        .filter(|n| {
            n.parent()
                .is_some_and(|p| p.kind() == SyntaxKind::STMT_LIST)
        })
        .filter_map(ast::TypeAlias::cast);
    for t in file
        .items()
        .filter_map(|i| match i {
            ast::Item::TypeAlias(t) => Some(t),
            _ => None,
        })
        .chain(local)
    {
        let (Some(name), Some(body)) = (t.name(), t.ty()) else {
            continue;
        };
        let params = t
            .generic_param_list()
            .map(|l| {
                l.type_or_const_params()
                    .filter_map(|p| match p {
                        ast::TypeOrConstParam::Type(tp) => tp.name().map(|n| n.text().to_string()),
                        ast::TypeOrConstParam::Const(cp) => cp.name().map(|n| n.text().to_string()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let scope = match t.syntax().parent() {
            Some(p) if p.kind() == SyntaxKind::STMT_LIST => p.text_range(),
            _ => file.syntax().text_range(),
        };
        out.push(Alias {
            name: name.text().to_string(),
            params,
            body,
            item: t.syntax().clone(),
            scope,
        });
    }
    out
}

/// The alias `name` means at `n`: the one of the innermost scope around it.
fn find<'a>(table: &'a [Alias], name: &str, n: &SyntaxNode) -> Option<&'a Alias> {
    table
        .iter()
        .filter(|a| a.name == name && a.scope.contains_range(n.text_range()))
        .min_by_key(|a| a.scope.len())
}

pub fn has_aliases(text: &str) -> bool {
    text.contains("type ") && !aliases(&crate::macros::parse_tree(text)).is_empty()
}

/// The alias's type with `args` (source text) for its parameters.
fn instantiate(a: &Alias, args: &[String]) -> String {
    let mut out = String::new();
    for t in a
        .body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        let param = (t.kind() == SyntaxKind::IDENT
            && t.parent().is_some_and(|p| p.kind() == SyntaxKind::NAME_REF))
        .then(|| a.params.iter().position(|p| p == t.text()))
        .flatten();
        match param {
            Some(i) => out.push_str(args.get(i).map(String::as_str).unwrap_or("_")),
            None => out.push_str(t.text()),
        }
    }
    if matches!(
        a.body,
        ast::Type::DynTraitType(_) | ast::Type::ImplTraitType(_)
    ) {
        out = format!("({out})");
    }
    out
}

/// A path type's text as an expression path: generic arguments with turbofish.
fn turbofish(ty: &str) -> Option<String> {
    let file = SourceFile::parse(&format!("type X = {ty};"), EDITION).tree();
    let Some(ast::Item::TypeAlias(t)) = file.items().next() else {
        return None;
    };
    let Some(ast::Type::PathType(p)) = t.ty() else {
        return None;
    };
    let path = p.path()?;
    let mut segs: Vec<ast::PathSegment> = std::iter::successors(Some(path), |p| p.qualifier())
        .filter_map(|p| p.segment())
        .collect();
    segs.reverse();
    let mut parts = Vec::new();
    for s in segs {
        let mut part = s.name_ref()?.text().to_string();
        if let Some(g) = s.generic_arg_list() {
            let g = g.syntax().text().to_string();
            part.push_str("::");
            part.push_str(g.trim_start_matches("::"));
        }
        parts.push(part);
    }
    Some(parts.join("::"))
}

/// Whether a generic parameter named `name` around `n` shadows the alias.
fn shadowed(n: &SyntaxNode, name: &str) -> bool {
    n.ancestors().any(|a| {
        let params = ast::Fn::cast(a.clone())
            .and_then(|x| x.generic_param_list())
            .or_else(|| ast::Impl::cast(a.clone()).and_then(|x| x.generic_param_list()))
            .or_else(|| ast::Struct::cast(a.clone()).and_then(|x| x.generic_param_list()))
            .or_else(|| ast::Enum::cast(a.clone()).and_then(|x| x.generic_param_list()))
            .or_else(|| ast::Trait::cast(a.clone()).and_then(|x| x.generic_param_list()));
        params.is_some_and(|l| {
            l.type_or_const_params().any(|p| match p {
                ast::TypeOrConstParam::Type(tp) => tp.name().is_some_and(|x| x.text() == name),
                ast::TypeOrConstParam::Const(cp) => cp.name().is_some_and(|x| x.text() == name),
            })
        })
    })
}

fn generic_args(seg: &ast::PathSegment) -> Vec<String> {
    seg.generic_arg_list()
        .map(|l| {
            // (Lifetimes aren't among the parameters: they're erased.)
            l.generic_args()
                .filter(|g| !matches!(g, ast::GenericArg::LifetimeArg(_)))
                .map(|g| g.syntax().text().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Expand every alias in `text`, recording the edits in `map`.
pub fn expand(text: &mut String, map: &mut OffsetMap) {
    // Each round replaces the uses outside the aliases; one alias's body may name another, so
    // repeat until nothing changes, then drop the alias items.
    for round in 0..16 {
        let file = crate::macros::parse_tree(text);
        let table = aliases(&file);
        if table.is_empty() {
            return;
        }
        let in_alias = |n: &SyntaxNode| {
            table
                .iter()
                .any(|a| a.item.text_range().contains_range(n.text_range()))
        };
        let mut edits: Vec<(usize, usize, String)> = Vec::new();
        for n in file.syntax().descendants() {
            if let Some(pt) = ast::PathType::cast(n.clone()) {
                let Some(path) = pt.path() else { continue };
                if path.qualifier().is_some() {
                    continue;
                }
                let Some(seg) = path.segment() else { continue };
                let Some(name) = seg.name_ref() else { continue };
                let Some(a) = find(&table, name.text(), &n) else {
                    continue;
                };
                if in_alias(&n) || shadowed(&n, name.text()) {
                    continue;
                }
                let r = n.text_range();
                edits.push((
                    r.start().into(),
                    r.end().into(),
                    instantiate(a, &generic_args(&seg)),
                ));
            } else if let Some(p) = ast::Path::cast(n.clone()) {
                // An expression or pattern path starting with an alias: `Pt::new()`, `Pt { .. }`.
                if p.syntax()
                    .parent()
                    .is_some_and(|x| ast::Path::can_cast(x.kind()))
                    || n.ancestors().any(|x| {
                        ast::PathType::can_cast(x.kind())
                            || ast::Use::can_cast(x.kind())
                            || ast::Attr::can_cast(x.kind())
                            || ast::MacroCall::can_cast(x.kind())
                    })
                {
                    continue;
                }
                let Some(first) = std::iter::successors(Some(p.clone()), |p| p.qualifier()).last()
                else {
                    continue;
                };
                let Some(seg) = first.segment() else { continue };
                let Some(name) = seg.name_ref() else { continue };
                let Some(a) = find(&table, name.text(), &n) else {
                    continue;
                };
                if in_alias(&n) || shadowed(&n, name.text()) {
                    continue;
                }
                let Some(new) = turbofish(&instantiate(a, &generic_args(&seg))) else {
                    continue;
                };
                let r = seg.syntax().text_range();
                edits.push((r.start().into(), r.end().into(), new));
            }
        }
        if edits.is_empty() || round == 15 {
            for a in &table {
                let r = a.item.text_range();
                edits.push((r.start().into(), r.end().into(), String::new()));
            }
        }
        let done = edits.iter().all(|e| e.2.is_empty());
        edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
        let mut last_start = usize::MAX;
        for (a, b, new) in edits {
            if b > last_start {
                continue;
            }
            text.replace_range(a..b, &new);
            map.replace(a, b, new.len());
            last_start = a;
        }
        if done {
            return;
        }
    }
}

/// Whether `text` may rename a std item in a `use` (`use std::collections::HashMap as Map;`).
pub fn has_use_renames(text: &str) -> bool {
    text.contains("use ") && text.contains(" as ")
}

/// `use std::collections::HashMap as Map;` (also `core::` / `alloc::`, in a list, and modules:
/// `use std::collections as coll;`): each use of `Map` in the file is written as the path it
/// stands for, `std::collections::HashMap`, which the compiler knows. (Renames of the file's own
/// items and of crates are resolved elsewhere.) Whether anything changed.
pub fn expand_use_renames(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let mut renames: std::collections::HashMap<String, String> = Default::default();
    let mut uses = Vec::new();
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        uses.push(u.syntax().text_range());
        for (mut path, rename, star) in crate::typeck::use_leaves(&u) {
            let Some(r) = rename else { continue };
            if path.last().is_some_and(|s| s == "self") {
                path.pop();
            }
            if star
                || r == "_"
                || path.len() < 2
                || path.last() == Some(&r)
                || !matches!(path[0].as_str(), "std" | "core" | "alloc")
            {
                continue;
            }
            renames.insert(r, path.join("::"));
        }
    }
    // (Not when the file declares something of that name too.)
    for n in file.syntax().descendants().filter_map(ast::Name::cast) {
        let is_rename = n
            .syntax()
            .parent()
            .is_some_and(|p| p.kind() == SyntaxKind::RENAME);
        if !is_rename {
            renames.remove(n.text());
        }
    }
    if renames.is_empty() {
        return false;
    }
    let mut edits = rename_uses_of(&file, &renames, &uses);
    let changed = !edits.is_empty();
    apply(text, map, &mut edits);
    changed
}

/// Non-trivia tokens of `file`.
fn tokens(file: &SourceFile) -> Vec<ra_ap_syntax::SyntaxToken> {
    file.syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect()
}

/// Edits writing each use of a renamed name (a path's first segment, not a field, method, macro
/// or the name of something declared) as what it stands for. Not inside `uses`.
fn rename_uses_of(
    file: &SourceFile,
    renames: &std::collections::HashMap<String, String>,
    uses: &[TextRange],
) -> Vec<(usize, usize, String)> {
    let toks = tokens(file);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if t.kind() != SyntaxKind::IDENT {
            continue;
        }
        let Some(full) = renames.get(t.text()) else {
            continue;
        };
        let r = t.text_range();
        let prev = if i > 0 { toks[i - 1].text() } else { "" };
        let next = toks.get(i + 1).map(|t| t.text()).unwrap_or("");
        if uses.iter().any(|u| u.contains_range(r))
            || matches!(prev, "." | "::" | "$" | "'")
            || (prev == ":" && i >= 2 && toks[i - 2].text() == ":")
            || next == "!"
            || t.parent_ancestors()
                .any(|a| a.kind() == SyntaxKind::MACRO_RULES)
            || t.parent().is_some_and(|p| {
                p.kind() == SyntaxKind::NAME
                    || p.parent().is_some_and(|g| {
                        matches!(
                            g.kind(),
                            SyntaxKind::RECORD_EXPR_FIELD | SyntaxKind::RECORD_PAT_FIELD
                        )
                    })
            })
        {
            continue;
        }
        edits.push((usize::from(r.start()), usize::from(r.end()), full.clone()));
    }
    edits
}

fn apply(text: &mut String, map: &mut OffsetMap, edits: &mut [(usize, usize, String)]) {
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    let mut last_start = usize::MAX;
    for (a, b, new) in edits.iter() {
        if *b > last_start {
            continue;
        }
        text.replace_range(*a..*b, new);
        map.replace(*a, *b, new.len());
        last_start = *a;
    }
}

/// An edit removing `rename` (` as Name`) with the whitespace before it.
fn drop_rename(text: &str, rename: &ast::Rename) -> (usize, usize, String) {
    let r = rename.syntax().text_range();
    let end = usize::from(r.end());
    let start = usize::from(r.start());
    let start = start - (text[..start].len() - text[..start].trim_end().len());
    (start, end, String::new())
}

fn is_std_crate(name: &str) -> bool {
    matches!(name, "std" | "core" | "alloc")
}

/// Whether `text` may rename a crate (`extern crate itertools as it;`).
pub fn has_crate_renames(text: &str) -> bool {
    text.contains("extern crate") && text.contains(" as ")
}

/// `extern crate itertools as it;` (std or a bundled crate): the rename goes, and each path
/// starting with `it::` (also `crate::it::`, and in a `use`) starts with `itertools::` instead
/// (`std::` for `extern crate alloc as core_alloc;`). Between std's crates (`extern crate core as
/// std;`, `extern crate std as alloc;`) only the rename goes: they're one crate here. Other
/// crates are left for the "unresolved" error.
pub fn expand_crate_renames(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let mut renames: std::collections::HashMap<String, String> = Default::default();
    let mut edits = Vec::new();
    for e in file
        .syntax()
        .descendants()
        .filter_map(ast::ExternCrate::cast)
    {
        let (Some(n), Some(rename)) = (e.name_ref(), e.rename()) else {
            continue;
        };
        let Some(alias) = rename.name() else { continue };
        let (name, alias) = (n.text().to_string(), alias.text().to_string());
        let known = is_std_crate(&name) || crate::typeck::crates::BUNDLED.contains(&name.as_str());
        if !known || name == alias {
            continue;
        }
        edits.push(drop_rename(text, &rename));
        if is_std_crate(&name) && !is_std_crate(&alias) {
            // (std's paths are the best known.)
            renames.insert(alias, "std".to_string());
        } else if !is_std_crate(&name) {
            renames.insert(alias, name);
        }
    }
    if edits.is_empty() {
        return false;
    }
    let toks = tokens(&file);
    for (i, t) in toks.iter().enumerate() {
        if t.kind() != SyntaxKind::IDENT
            || toks.get(i + 1).is_none_or(|n| n.text() != "::")
            || t.parent_ancestors()
                .any(|a| a.kind() == SyntaxKind::MACRO_RULES)
        {
            continue;
        }
        let Some(name) = renames.get(t.text()) else {
            continue;
        };
        let r = t.text_range();
        let prev = if i > 0 { toks[i - 1].text() } else { "" };
        let before = if i > 1 { toks[i - 2].text() } else { "" };
        let start = match prev {
            "." | "$" | "'" => continue,
            // `crate::it::join` → `itertools::join`
            "::" if before == "crate" && (i < 3 || toks[i - 3].text() != "::") => {
                toks[i - 2].text_range().start()
            }
            "::" if (i > 1 && toks[i - 2].kind() == SyntaxKind::IDENT)
                || matches!(before, ">" | "self" | "super" | "Self" | "crate") =>
            {
                continue;
            }
            _ => r.start(),
        };
        edits.push((start.into(), r.end().into(), name.clone()));
    }
    apply(text, map, &mut edits);
    true
}

/// Whether `text` may rename an imported item (`use geo::Point as P;`), or import one that a
/// `mod` file renamed.
pub fn has_import_renames(text: &str) -> bool {
    text.contains("use ") && (text.contains(" as ") || text.contains("mod "))
}

/// `use geo::Point as P;` / `use serde_json::Value as Json;` at the top of the file, for a type,
/// trait or constant (both names capitalized) of a `mod` file or a crate: the rename goes and
/// each use of `P` is written `Point`. Not when that would be ambiguous: the file declares or
/// imports another `Point`, or renames two different `Point`s (types share one namespace here, so
/// those stay an error). Std's are `expand_use_renames`'s, inline modules' `modflat`'s.
///
/// With the file's name and a resolver, a `use geo::AreaError;` whose `mod` file renames it
/// (`pub use area::Error as AreaError;`) is `use geo::Error;` the same way: the module exports
/// the type by its own name.
pub fn expand_import_renames(
    text: &mut String,
    map: &mut OffsetMap,
    files: Option<(&str, crate::typeck::Resolver)>,
) -> bool {
    let file = crate::macros::parse_tree(text);
    // `geo::AreaError` → `Error`, from `mod` files' `pub use`s.
    let reexported = |path: &[String]| -> Option<String> {
        let (filename, resolver) = files?;
        let rel: Vec<&String> = path
            .iter()
            .skip_while(|s| matches!(s.as_str(), "crate" | "self"))
            .collect();
        let (name, mods) = rel.split_last()?;
        if mods.is_empty() {
            return None;
        }
        let mut cur = filename.to_string();
        let mut src = String::new();
        for m in mods {
            let (p, t, _) = crate::typeck::find_module(&cur, m, resolver)?;
            cur = p;
            src = t;
        }
        crate::typeck::pub_use_renames(&src).remove(name.as_str())
    };
    let capital = crate::typeck::capitalized;
    // alias → (original name, its path); the original name → its paths.
    let mut renames: std::collections::HashMap<String, (String, String)> = Default::default();
    let mut paths: std::collections::HashMap<String, std::collections::HashSet<String>> =
        Default::default();
    let mut drops: Vec<(ast::Rename, String)> = Vec::new();
    // `use geo::AreaError;` leaves whose last segment is written `Error`.
    let mut renamed_leaves: Vec<(ast::NameRef, String)> = Vec::new();
    let mut uses = Vec::new();
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        uses.push(u.syntax().text_range());
        let top = u
            .syntax()
            .parent()
            .is_some_and(|p| p.kind() == SyntaxKind::SOURCE_FILE);
        let mut trees: Vec<ast::UseTree> = u
            .syntax()
            .descendants()
            .filter_map(ast::UseTree::cast)
            .filter(|t| t.use_tree_list().is_none())
            .collect();
        for (path, rename, star) in crate::typeck::use_leaves(&u) {
            let tree = (!trees.is_empty()).then(|| trees.remove(0));
            if star || path.is_empty() {
                continue;
            }
            let orig = path.last().cloned().unwrap_or_default();
            let full = path
                .iter()
                .map(String::as_str)
                .skip_while(|s| matches!(*s, "crate" | "self"))
                .collect::<Vec<_>>()
                .join("::");
            // (A renamed import doesn't bring in its original name, but one that this pass
            // writes back does.)
            let Some(alias) = rename else {
                if top
                    && capital(&orig)
                    && let Some(real) = reexported(&path)
                    && let Some(seg) = tree
                        .as_ref()
                        .and_then(|t| t.path())
                        .and_then(|p| p.segment())
                        .and_then(|s| s.name_ref())
                {
                    let mut real_path = full
                        .rsplit_once("::")
                        .map_or(String::new(), |p| p.0.to_string());
                    real_path.push_str("::");
                    real_path.push_str(&real);
                    paths
                        .entry(real.clone())
                        .or_default()
                        .insert(real_path.clone());
                    renamed_leaves.push((seg, orig.clone()));
                    renames.insert(orig, (real, real_path));
                    continue;
                }
                paths.entry(orig).or_default().insert(full);
                continue;
            };
            if !top
                || alias == orig
                || path.len() < 2
                || is_std_crate(&path[0])
                || !capital(&alias)
                || !capital(&orig)
            {
                continue;
            }
            paths.entry(orig.clone()).or_default().insert(full.clone());
            if let Some(prev) = renames.get(&alias)
                && prev.1 != full
            {
                renames.insert(alias.clone(), (String::new(), String::new()));
                continue;
            }
            renames.insert(alias.clone(), (orig, full.clone()));
            if let Some(r) = tree.and_then(|t| t.rename()) {
                drops.push((r, alias));
            }
        }
    }
    // Names the file declares (not the `as` names).
    let mut declared = std::collections::HashSet::new();
    for n in file.syntax().descendants().filter_map(ast::Name::cast) {
        if n.syntax()
            .parent()
            .is_none_or(|p| p.kind() != SyntaxKind::RENAME)
        {
            declared.insert(n.text().to_string());
        }
    }
    renames.retain(|alias, (orig, _)| {
        !orig.is_empty()
            && !declared.contains(alias.as_str())
            && !declared.contains(orig.as_str())
            && paths.get(orig.as_str()).is_none_or(|p| p.len() == 1)
    });
    if renames.is_empty() {
        return false;
    }
    let names: std::collections::HashMap<String, String> = renames
        .iter()
        .map(|(a, (o, _))| (a.clone(), o.clone()))
        .collect();
    let mut edits = rename_uses_of(&file, &names, &uses);
    for (r, alias) in drops {
        if names.contains_key(&alias) {
            edits.push(drop_rename(text, &r));
        }
    }
    for (seg, alias) in renamed_leaves {
        if let Some(real) = names.get(&alias) {
            let r = seg.syntax().text_range();
            edits.push((r.start().into(), r.end().into(), real.clone()));
        }
    }
    apply(text, map, &mut edits);
    true
}
