//! Functions nested in other functions, as a pre-pass over the source text: the checker and
//! codegen keep one namespace of functions per file, so a nested `fn` whose name another function
//! also has (two helpers that each define `fn imp()`) is renamed `imp__1`, `imp__2`, .. along with
//! the calls in its block. An `OffsetMap` keeps positions pointing at the original text.

use std::collections::HashMap;

use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxToken, TextRange, ast, ast::HasName};

use crate::mbe::OffsetMap;

/// True if some `fn` name appears twice (a cheap check before parsing).
pub(crate) fn has_duplicate_fns(text: &str) -> bool {
    let mut seen = std::collections::HashSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("fn ") {
        let before = rest[..i].chars().next_back();
        rest = &rest[i + 3..];
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
    false
}

pub(crate) fn expand(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    // Every function that shares the file's namespace: top-level ones and those in blocks.
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut nested: Vec<(String, TextRange, TextRange)> = Vec::new();
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(name) = f.name() else { continue };
        let Some(parent) = f.syntax().parent() else {
            continue;
        };
        match parent.kind() {
            SyntaxKind::SOURCE_FILE => {}
            SyntaxKind::STMT_LIST => nested.push((
                name.text().to_string(),
                name.syntax().text_range(),
                parent.text_range(),
            )),
            _ => continue,
        }
        *counts.entry(name.text().to_string()).or_default() += 1;
    }
    // (name, scope, new name), for the nested functions whose name is taken elsewhere.
    let mut scopes: Vec<(String, TextRange, String)> = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut next: HashMap<String, usize> = HashMap::new();
    for (name, at, scope) in nested {
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
        scopes.push((name, scope, new));
    }
    if scopes.is_empty() {
        return false;
    }
    // Innermost scope first: a block's own `fn imp` hides an outer one.
    scopes.sort_by_key(|(_, r, _)| r.len());
    let lookup = |at: TextRange, name: &str| -> Option<String> {
        scopes
            .iter()
            .find(|(n, r, _)| n == name && r.contains_range(at))
            .map(|(_, _, new)| new.clone())
    };
    for n in file.syntax().descendants() {
        if let Some(p) = ast::Path::cast(n.clone()) {
            // A lone name: `imp()`, `xs.map(imp)`.
            if p.qualifier().is_some()
                || n.parent().is_some_and(|x| ast::Path::can_cast(x.kind()))
                || n.parent()
                    .is_some_and(|x| ast::MacroCall::can_cast(x.kind()))
                || n.ancestors().any(|a| ast::Use::can_cast(a.kind()))
            {
                continue;
            }
            let Some(name) = p.segment().and_then(|s| s.name_ref()) else {
                continue;
            };
            if let Some(new) = lookup(n.text_range(), name.text()) {
                let r = name.syntax().text_range();
                edits.push((r.start().into(), r.end().into(), new));
            }
        } else if n.kind() == SyntaxKind::TOKEN_TREE
            && !n
                .ancestors()
                .any(|a| ast::Attr::can_cast(a.kind()) || ast::MacroRules::can_cast(a.kind()))
        {
            // Macro arguments: `println!("{}", imp())`.
            let toks: Vec<SyntaxToken> = n
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
                .collect();
            for (i, t) in toks.iter().enumerate() {
                if t.kind() != SyntaxKind::IDENT {
                    continue;
                }
                let prev = i.checked_sub(1).map(|j| toks[j].text().to_string());
                let next = toks.get(i + 1).map(|t| t.text().to_string());
                if matches!(prev.as_deref(), Some("." | ":" | "fn" | "let" | "mut"))
                    || matches!(next.as_deref(), Some(":" | "!"))
                {
                    continue;
                }
                if let Some(new) = lookup(t.text_range(), t.text()) {
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
