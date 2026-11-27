//! An inherent `const` with the name of a trait-impl `const` on the same type (`impl S { const
//! ID: u32 = 3; }` beside `impl Tr for S { const ID: u32 = 7; }`), as a pre-pass over the source
//! text: the checker and codegen keep one set of consts per type (one JS class's statics), so
//! the inherent one is renamed `ID__self`, along with `S::ID` and, inside an impl for `S`,
//! `Self::ID` (in rustc both name the inherent const, even inside the trait impl). The trait's
//! keeps its name: `<S as Tr>::ID`, a generic `T::ID` and the trait's default methods read it.
//! An `OffsetMap` keeps positions pointing at the original text.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxToken,
    ast::{self, HasName},
};

use crate::mbe::OffsetMap;

/// True if the text has a trait impl and some `const` name appears twice (a cheap check before
/// parsing).
pub(crate) fn has_shadowed_consts(text: &str) -> bool {
    if !text.contains(" for ") {
        return false;
    }
    let mut seen = HashSet::new();
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

/// An impl's self type name (`S` of `impl Tr for S` / `impl<T> S<T>`).
fn self_name(imp: &ast::Impl) -> Option<String> {
    let ast::Type::PathType(p) = imp.self_ty()? else {
        return None;
    };
    Some(p.path()?.segment()?.name_ref()?.text().to_string())
}

fn consts(imp: &ast::Impl) -> impl Iterator<Item = ast::Name> {
    imp.assoc_item_list()
        .into_iter()
        .flat_map(|l| l.assoc_items())
        .filter_map(|i| match i {
            ast::AssocItem::Const(k) => k.name(),
            _ => None,
        })
}

pub(crate) fn expand(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let impls: Vec<ast::Impl> = file.syntax().descendants().filter_map(ast::Impl::cast).collect();
    // (A trait's consts by trait name: its defaults are the type's too, given or not.)
    let declared: HashMap<String, Vec<String>> = file
        .syntax()
        .descendants()
        .filter_map(ast::Trait::cast)
        .filter_map(|t| {
            let ks = t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
                .filter_map(|i| match i {
                    ast::AssocItem::Const(k) => Some(k.name()?.text().to_string()),
                    _ => None,
                })
                .collect();
            Some((t.name()?.text().to_string(), ks))
        })
        .collect();
    let mut trait_consts: HashSet<(String, String)> = HashSet::new();
    for imp in &impls {
        let (Some(tr), Some(ty)) = (imp.trait_(), self_name(imp)) else { continue };
        for n in consts(imp) {
            trait_consts.insert((ty.clone(), n.text().to_string()));
        }
        let tr = match tr {
            ast::Type::PathType(p) => p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref()),
            _ => None,
        };
        for k in tr.and_then(|t| declared.get(&t.text().to_string())).into_iter().flatten() {
            trait_consts.insert((ty.clone(), k.clone()));
        }
    }
    // (type, const) → its new name.
    let mut renamed: HashMap<(String, String), String> = HashMap::new();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for imp in impls.iter().filter(|i| i.trait_().is_none()) {
        let Some(ty) = self_name(imp) else { continue };
        for n in consts(imp) {
            let key = (ty.clone(), n.text().to_string());
            if !trait_consts.contains(&key) {
                continue;
            }
            let new = renamed.entry(key).or_insert_with(|| {
                let mut new = format!("{}__self", n.text());
                while text.contains(new.as_str()) {
                    new.push('_');
                }
                new
            });
            let r = n.syntax().text_range();
            edits.push((r.start().into(), r.end().into(), new.clone()));
        }
    }
    if renamed.is_empty() {
        return false;
    }
    // `S::ID` and `Self::ID`, in paths and in macro arguments alike: over the tokens.
    let toks: Vec<SyntaxToken> = file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let at = |j: usize| toks.get(j).map(|t| t.text()).unwrap_or_default();
    // The index after a `::` at `i` (one token, or two `:` in a macro's tokens).
    let colons = |i: usize| match (at(i), at(i + 1)) {
        ("::", _) => Some(i + 1),
        (":", ":") if toks[i].text_range().end() == toks[i + 1].text_range().start() => {
            Some(i + 2)
        }
        _ => None,
    };
    for (i, t) in toks.iter().enumerate() {
        if !matches!(t.kind(), SyntaxKind::IDENT | SyntaxKind::SELF_TYPE_KW) {
            continue;
        }
        let Some(j) = colons(i + 1) else { continue };
        let Some(c) = toks.get(j).filter(|c| c.kind() == SyntaxKind::IDENT) else {
            continue;
        };
        // (Not a longer path: `S::ID::x`, `S::ID(..)` would be something else of the name.)
        if colons(j + 1).is_some() || at(j + 1) == "(" {
            continue;
        }
        let ty = if t.text() == "Self" {
            let Some(ty) = t
                .parent_ancestors()
                .find_map(ast::Impl::cast)
                .and_then(|i| self_name(&i))
            else {
                continue;
            };
            ty
        } else {
            t.text().to_string()
        };
        if let Some(new) = renamed.get(&(ty, c.text().to_string())) {
            let r = c.text_range();
            edits.push((r.start().into(), r.end().into(), new.clone()));
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
