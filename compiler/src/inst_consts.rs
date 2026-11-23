//! Associated consts of inherent impls per type argument (`impl Cell<u8> { const MIN: u8 = 0; }`
//! next to `impl Cell<i32> { const MIN: i32 = i32::MIN; }`), as a pre-pass over the source text:
//! the checker and codegen keep one set of consts per type, so each later impl's const is renamed
//! `MIN__1`, `MIN__2`, .. along with `Self::MIN` inside that impl and `Cell::<i32>::MIN` (type
//! arguments written as in the impl's header). An `OffsetMap` keeps positions pointing at the
//! original text.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxToken, TextRange,
    ast::{self, HasGenericArgs, HasName},
};

use crate::mbe::OffsetMap;

/// True if some `const` name appears twice and the text has an `impl` with type arguments (a
/// cheap check before parsing).
pub(crate) fn has_instantiation_consts(text: &str) -> bool {
    if !text.contains("impl") || !text.contains('<') {
        return false;
    }
    let mut seen = std::collections::HashSet::new();
    let mut rest = text;
    while let Some(i) = rest.find("const ") {
        let before = rest[..i].chars().next_back();
        rest = &rest[i + 6..];
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

/// An inherent impl's self type as (type name, its type arguments without whitespace).
fn self_inst(imp: &ast::Impl) -> Option<(String, String)> {
    if imp.trait_().is_some() {
        return None;
    }
    let ast::Type::PathType(p) = imp.self_ty()? else {
        return None;
    };
    let seg = p.path()?.segment()?;
    let args = seg.generic_arg_list()?;
    Some((seg.name_ref()?.text().to_string(), squash(&args.syntax().text().to_string())))
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .trim_start_matches("::")
        .to_string()
}

pub(crate) fn expand(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    // (type, const) → the impls declaring it: (type arguments, impl range, const name range).
    let mut decls: HashMap<(String, String), Vec<(String, TextRange, TextRange)>> = HashMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for imp in file.syntax().descendants().filter_map(ast::Impl::cast) {
        let Some((ty, args)) = self_inst(&imp) else {
            continue;
        };
        for item in imp.assoc_item_list().into_iter().flat_map(|l| l.assoc_items()) {
            let ast::AssocItem::Const(k) = item else { continue };
            let Some(n) = k.name() else { continue };
            let key = (ty.clone(), n.text().to_string());
            if !decls.contains_key(&key) {
                order.push(key.clone());
            }
            decls.entry(key).or_default().push((
                args.clone(),
                imp.syntax().text_range(),
                n.syntax().text_range(),
            ));
        }
    }
    // (type, type arguments, const) → new name; (impl range, const) → new name.
    let mut by_args: HashMap<(String, String, String), String> = HashMap::new();
    let mut by_impl: Vec<(TextRange, String, String)> = Vec::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for key in order {
        let impls = &decls[&key];
        let (ty, name) = key;
        let mut k = 0;
        for (i, (args, imp, at)) in impls.iter().enumerate() {
            // (The first impl with these arguments keeps the name; a repeat is rustc's error.)
            if impls[..i].iter().any(|(a, ..)| a == args) || i == 0 {
                continue;
            }
            let new = loop {
                k += 1;
                let new = format!("{name}__{k}");
                if !text.contains(new.as_str()) {
                    break new;
                }
            };
            edits.push((at.start().into(), at.end().into(), new.clone()));
            by_args.insert((ty.clone(), args.clone(), name.clone()), new.clone());
            by_impl.push((*imp, name.clone(), new));
        }
    }
    if by_impl.is_empty() {
        return false;
    }
    let in_impl = |at: TextRange, name: &str| {
        by_impl
            .iter()
            .filter(|(r, n, _)| n == name && r.contains_range(at))
            .min_by_key(|(r, ..)| r.len())
            .map(|(_, _, new)| new.clone())
    };
    for n in file.syntax().descendants() {
        if let Some(p) = ast::Path::cast(n.clone()) {
            // `Self::MIN`, `Cell::<i32>::MIN`.
            let (Some(q), Some(name)) = (p.qualifier(), p.segment().and_then(|s| s.name_ref()))
            else {
                continue;
            };
            let Some(qs) = q.segment() else { continue };
            let new = if qs.self_type_token().is_some() && q.qualifier().is_none() {
                in_impl(n.text_range(), name.text().as_ref())
            } else {
                let ty = qs.name_ref().map(|t| t.text().to_string());
                let args = qs
                    .generic_arg_list()
                    .map(|a| squash(&a.syntax().text().to_string()));
                ty.zip(args).and_then(|(ty, args)| {
                    by_args.get(&(ty, args, name.text().to_string())).cloned()
                })
            };
            if let Some(new) = new {
                let r = name.syntax().text_range();
                edits.push((r.start().into(), r.end().into(), new));
            }
        } else if n.kind() == SyntaxKind::TOKEN_TREE {
            // Macro arguments: `println!("{}", Self::MIN)`.
            let toks: Vec<SyntaxToken> = n
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
                .collect();
            let text_at = |j: usize| toks.get(j).map(|t| t.text().to_string());
            // The index after a `::` at `i` (one token or two), if there's one.
            let colons = |i: usize| match (text_at(i).as_deref(), text_at(i + 1).as_deref()) {
                (Some("::"), _) => Some(i + 1),
                (Some(":"), Some(":")) => Some(i + 2),
                _ => None,
            };
            for (i, t) in toks.iter().enumerate() {
                let Some(mut j) = colons(i + 1) else { continue };
                let new = if t.text() == "Self" {
                    toks.get(j)
                        .filter(|c| c.kind() == SyntaxKind::IDENT)
                        .and_then(|c| in_impl(c.text_range(), c.text()))
                } else if text_at(j).as_deref() == Some("<") {
                    // `Cell::<i32>::MIN`: the arguments up to the matching `>`.
                    let mut depth = 0;
                    let mut args = String::new();
                    while let Some(a) = toks.get(j) {
                        args.push_str(a.text());
                        j += 1;
                        match a.text() {
                            "<" => depth += 1,
                            ">" => depth -= 1,
                            _ => {}
                        }
                        if depth == 0 {
                            break;
                        }
                    }
                    let Some(k) = colons(j) else { continue };
                    j = k;
                    toks.get(j).and_then(|c| {
                        by_args
                            .get(&(t.text().to_string(), squash(&args), c.text().to_string()))
                            .cloned()
                    })
                } else {
                    None
                };
                if let (Some(new), Some(c)) = (new, toks.get(j)) {
                    let r = c.text_range();
                    edits.push((r.start().into(), r.end().into(), new));
                }
            }
        }
    }
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    edits.dedup();
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
