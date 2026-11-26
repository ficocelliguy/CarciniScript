//! Items nested in functions, as a pre-pass over the source text: the checker and codegen keep
//! one namespace of items per file, so a nested item whose name another item also has (two
//! helpers that each define `fn imp()`, or `enum State` / `struct Acc`) is renamed `imp__1`,
//! `State__2`, .. along with the paths in its block. An `OffsetMap` keeps positions pointing at
//! the original text, and the returned names map a renamed type back to the written one, for
//! `{:?}`.

use std::collections::HashMap;

use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxToken, TextRange, ast, ast::HasName};

use crate::mbe::OffsetMap;

const KEYWORDS: [&str; 7] = [
    "fn ", "struct ", "enum ", "trait ", "const ", "static ", "type ",
];

/// True if some item name appears twice (a cheap check before parsing).
pub(crate) fn has_duplicate_items(text: &str) -> bool {
    let mut seen = std::collections::HashSet::new();
    for kw in KEYWORDS {
        let mut rest = text;
        while let Some(i) = rest.find(kw) {
            let before = rest[..i].chars().next_back();
            rest = &rest[i + kw.len()..];
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let name: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() && !seen.insert(name) {
                return true;
            }
        }
    }
    false
}

/// An item's name, and whether it names a type (which can start a path: `State::A`).
fn item_name(item: &ast::Item) -> Option<(ast::Name, bool)> {
    match item {
        ast::Item::Fn(x) => x.name().map(|n| (n, false)),
        ast::Item::Const(x) => x.name().map(|n| (n, false)),
        ast::Item::Static(x) => x.name().map(|n| (n, false)),
        ast::Item::Struct(x) => x.name().map(|n| (n, true)),
        ast::Item::Enum(x) => x.name().map(|n| (n, true)),
        ast::Item::Trait(x) => x.name().map(|n| (n, true)),
        ast::Item::TypeAlias(x) => x.name().map(|n| (n, true)),
        _ => None,
    }
}

pub(crate) fn expand(
    text: &mut String,
    map: &mut OffsetMap,
    names: &mut HashMap<String, String>,
) -> bool {
    let file = crate::macros::parse_tree(text);
    // Every item that shares the file's namespace: top-level ones and those in blocks.
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut nested: Vec<(String, TextRange, TextRange, bool)> = Vec::new();
    for item in file.syntax().descendants().filter_map(ast::Item::cast) {
        let Some((name, is_type)) = item_name(&item) else {
            continue;
        };
        let Some(parent) = item.syntax().parent() else {
            continue;
        };
        if crate::adt::test_only(item.syntax()) {
            continue;
        }
        match parent.kind() {
            SyntaxKind::SOURCE_FILE => {}
            SyntaxKind::STMT_LIST => nested.push((
                name.text().to_string(),
                name.syntax().text_range(),
                parent.text_range(),
                is_type,
            )),
            _ => continue,
        }
        *counts.entry(name.text().to_string()).or_default() += 1;
    }
    // (name, scope, new name, is a type), for the nested items whose name is taken elsewhere.
    let mut scopes: Vec<(String, TextRange, String, bool)> = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut next: HashMap<String, usize> = HashMap::new();
    for (name, at, scope, is_type) in nested {
        if counts[&name] < 2 {
            continue;
        }
        let new = loop {
            let k = next.entry(name.clone()).or_default();
            *k += 1;
            let new = format!("{name}__{k}");
            if !text.contains(new.as_str()) {
                break new;
            }
        };
        edits.push((at.start().into(), at.end().into(), new.clone()));
        if is_type {
            // (Through an earlier pass's name: `Error__file` is the file's `Error`.)
            let written = names.get(&name).cloned().unwrap_or_else(|| name.clone());
            names.insert(new.clone(), written);
        }
        scopes.push((name, scope, new, is_type));
    }
    if scopes.is_empty() {
        return false;
    }
    // Innermost scope first: a block's own `fn imp` hides an outer one.
    scopes.sort_by_key(|(_, r, _, _)| r.len());
    // `qualifier`: the name starts a longer path (`State::A`), which only a type can.
    let lookup = |at: TextRange, name: &str, qualifier: bool| -> Option<String> {
        scopes
            .iter()
            .find(|(n, r, _, _)| n == name && r.contains_range(at))
            .filter(|(_, _, _, is_type)| *is_type || !qualifier)
            .map(|(_, _, new, _)| new.clone())
    };
    for n in file.syntax().descendants() {
        if let Some(p) = ast::Path::cast(n.clone()) {
            // A path's first name: `imp()`, `xs.map(imp)`, `Acc(1)`, `State::B(n)`.
            if p.qualifier().is_some()
                || n.parent()
                    .is_some_and(|x| ast::MacroCall::can_cast(x.kind()))
                || n.ancestors().any(|a| ast::Use::can_cast(a.kind()))
            {
                continue;
            }
            let Some(name) = p.segment().and_then(|s| s.name_ref()) else {
                continue;
            };
            let qualifier = n.parent().is_some_and(|x| ast::Path::can_cast(x.kind()));
            if let Some(new) = lookup(n.text_range(), name.text(), qualifier) {
                let r = name.syntax().text_range();
                edits.push((r.start().into(), r.end().into(), new));
            }
        } else if n.kind() == SyntaxKind::TOKEN_TREE
            && !n
                .ancestors()
                .any(|a| ast::Attr::can_cast(a.kind()) || ast::MacroRules::can_cast(a.kind()))
        {
            // Macro arguments: `println!("{}", imp())`, `assert!(matches!(s, State::A))`.
            let toks: Vec<SyntaxToken> = n
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
                .collect();
            let text_at = |j: Option<usize>| j.and_then(|j| toks.get(j)).map(|t| t.text());
            for (i, t) in toks.iter().enumerate() {
                if t.kind() != SyntaxKind::IDENT {
                    continue;
                }
                let prev = text_at(i.checked_sub(1));
                let prev2 = text_at(i.checked_sub(2));
                let next = text_at(Some(i + 1));
                let path_after =
                    next == Some("::") || (next == Some(":") && text_at(Some(i + 2)) == Some(":"));
                let path_before = prev == Some("::") || (prev == Some(":") && prev2 == Some(":"));
                if path_before
                    || matches!(prev, Some("." | "fn" | "let" | "mut"))
                    || next == Some("!")
                    || (next == Some(":") && !path_after)
                {
                    continue;
                }
                // (`x: State` is a type; a lone `:` before a function name is a field.)
                let typed = prev == Some(":");
                if let Some(new) = lookup(t.text_range(), t.text(), path_after || typed) {
                    let r = t.text_range();
                    edits.push((r.start().into(), r.end().into(), new));
                }
            }
        }
    }
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
    true
}
