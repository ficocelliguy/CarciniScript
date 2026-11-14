//! `use Enum::*` and `use Enum::{A, B as Bee}` for enums of this file, as a pre-pass over the
//! source text: each imported variant name, in expressions, patterns and macro arguments, is
//! written out as `Enum::A`, and the `use` goes away. A `use` inside a function covers that
//! function; one at the top covers the file. An `OffsetMap` keeps positions pointing at the
//! original text.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, SyntaxToken, TextRange,
    ast::{self, HasGenericArgs, HasModuleItem, HasName},
};

use crate::macros::EDITION;
use crate::mbe::OffsetMap;

/// Each file-level enum's variant names.
fn enums(file: &SourceFile) -> HashMap<String, Vec<String>> {
    file.items()
        .filter_map(|i| match i {
            ast::Item::Enum(e) => Some(e),
            _ => None,
        })
        .filter_map(|e| {
            let variants = e
                .variant_list()?
                .variants()
                .filter_map(|v| v.name().map(|n| n.text().to_string()))
                .collect();
            Some((e.name()?.text().to_string(), variants))
        })
        .collect()
}

pub fn has_variant_imports(text: &str) -> bool {
    if !text.contains("use ") {
        return false;
    }
    let file = SourceFile::parse(text, EDITION).tree();
    let enums = enums(&file);
    !enums.is_empty()
        && file
            .syntax()
            .descendants()
            .filter_map(ast::Use::cast)
            .any(|u| imports(&u, &enums).is_some())
}

/// What `u` imports, when every leaf is a variant (or all variants) of an enum of this file:
/// local name → `Enum::Variant`.
fn imports(u: &ast::Use, enums: &HashMap<String, Vec<String>>) -> Option<HashMap<String, String>> {
    let mut out = HashMap::new();
    for (mut segs, rename, star) in crate::typeck::use_leaves(u) {
        if matches!(segs.first().map(String::as_str), Some("crate" | "self")) {
            segs.remove(0);
        }
        let variants = enums.get(segs.first()?)?;
        let e = &segs[0];
        match (segs.len(), star) {
            (1, true) => {
                for v in variants {
                    out.insert(v.clone(), format!("{e}::{v}"));
                }
            }
            (2, false) if variants.contains(&segs[1]) => {
                out.insert(
                    rename.unwrap_or_else(|| segs[1].clone()),
                    format!("{e}::{}", segs[1]),
                );
            }
            _ => return None,
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Rewrite the imported variants in `text`, recording the edits in `map`.
pub fn expand(text: &mut String, map: &mut OffsetMap) {
    let file = SourceFile::parse(text, EDITION).tree();
    let enums = enums(&file);
    // (scope, names) for each `use`, and the `use`s to drop.
    let mut scopes: Vec<(TextRange, HashMap<String, String>)> = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        let Some(names) = imports(&u, &enums) else {
            continue;
        };
        let scope = u
            .syntax()
            .ancestors()
            .find(|a| ast::StmtList::can_cast(a.kind()) || ast::ItemList::can_cast(a.kind()))
            .unwrap_or_else(|| file.syntax().clone())
            .text_range();
        scopes.push((scope, names));
        let r = u.syntax().text_range();
        edits.push((r.start().into(), r.end().into(), String::new()));
    }
    if scopes.is_empty() {
        return;
    }
    // Innermost scope first.
    scopes.sort_by_key(|(r, _)| r.len());
    let lookup = |at: TextRange, name: &str| -> Option<String> {
        scopes
            .iter()
            .filter(|(r, _)| r.contains_range(at))
            .find_map(|(_, m)| m.get(name).cloned())
    };
    let skip = |n: &SyntaxNode| {
        n.ancestors().any(|a| {
            ast::Use::can_cast(a.kind())
                || ast::Attr::can_cast(a.kind())
                || ast::PathType::can_cast(a.kind())
        })
    };
    for n in file.syntax().descendants() {
        if let Some(p) = ast::Path::cast(n.clone()) {
            // A lone name: `A`, `B(1)`, `C { x }`, also in patterns.
            if p.qualifier().is_some()
                || n.parent().is_some_and(|x| ast::Path::can_cast(x.kind()))
                || n.parent()
                    .is_some_and(|x| ast::MacroCall::can_cast(x.kind()))
                || skip(&n)
            {
                continue;
            }
            let Some(seg) = p.segment() else { continue };
            let Some(name) = seg.name_ref() else { continue };
            if seg.generic_arg_list().is_some() {
                continue;
            }
            if let Some(full) = lookup(n.text_range(), name.text()) {
                let r = name.syntax().text_range();
                edits.push((r.start().into(), r.end().into(), full));
            }
        } else if let Some(ip) = ast::IdentPat::cast(n.clone()) {
            // `A =>` parses as a binding; with `A` imported it's the variant.
            if ip.ref_token().is_some()
                || ip.mut_token().is_some()
                || ip.pat().is_some()
                || skip(&n)
            {
                continue;
            }
            let Some(name) = ip.name() else { continue };
            if let Some(full) = lookup(n.text_range(), name.text()) {
                let r = name.syntax().text_range();
                edits.push((r.start().into(), r.end().into(), full));
            }
        } else if n.kind() == SyntaxKind::TOKEN_TREE
            && !n
                .ancestors()
                .any(|a| ast::Attr::can_cast(a.kind()) || ast::MacroRules::can_cast(a.kind()))
        {
            // Macro arguments: `println!("{:?}", A)`, `vec![A, B(2)]`.
            let toks: Vec<SyntaxToken> = n
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
                .collect();
            for (i, t) in toks.iter().enumerate() {
                if t.kind() != SyntaxKind::IDENT {
                    continue;
                }
                let prev = i
                    .checked_sub(1)
                    .map(|j| toks[j].text().to_string())
                    .unwrap_or_default();
                let before = i
                    .checked_sub(2)
                    .map(|j| toks[j].text().to_string())
                    .unwrap_or_default();
                let next = toks
                    .get(i + 1)
                    .map(|t| t.text().to_string())
                    .unwrap_or_default();
                let after = toks
                    .get(i + 2)
                    .map(|t| t.text().to_string())
                    .unwrap_or_default();
                if matches!(prev.as_str(), "." | "::")
                    || (prev == ":" && before == ":")
                    || matches!(next.as_str(), "::" | "!")
                    || (next == ":" && after != ":")
                    || (next == "=" && after != "=")
                {
                    continue;
                }
                if let Some(full) = lookup(t.text_range(), t.text()) {
                    let r = t.text_range();
                    edits.push((r.start().into(), r.end().into(), full));
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
}
