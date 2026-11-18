//! Associated type bounds (`I: IntoIterator<Item: Display>`), as a pre-pass over the source: the
//! bound moves to a `where` clause on the projection (`where I::Item: Display`), which the
//! checker already understands. In argument position (`it: impl Iterator<Item: Into<i64>>`) the
//! associated type becomes a type parameter of the function with those bounds
//! (`impl Iterator<Item = __Item0>`, `fn f<__Item0: Into<i64>>`). An `OffsetMap` keeps positions
//! pointing at the original text.
//!
//! Left alone (the checker reports them): bounds on a type that isn't a plain name
//! (`Vec<T>: IntoIterator<Item: ..>`), in return position, in `dyn`, and nested ones.

use std::collections::BTreeMap;

use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasGenericArgs, HasGenericParams, HasName, HasTypeBounds},
};

use crate::mbe::OffsetMap;

/// Whether `text` may have an associated type bound: `<Name:` or `, Name:` with a capitalized
/// name (a parameter list's `, name:` is lowercase).
pub fn has_assoc_bounds(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'<' || b[i] == b',' {
            let mut j = i + 1;
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j].is_ascii_uppercase() {
                let mut k = j;
                while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == b'_') {
                    k += 1;
                }
                while k < b.len() && b[k] == b' ' {
                    k += 1;
                }
                if k + 1 < b.len() && b[k] == b':' && b[k + 1] != b':' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

/// What one item (a `fn`, `impl` or `trait`) gains.
#[derive(Default)]
struct ItemEdits {
    /// `I::Item: Display`
    preds: Vec<String>,
    /// `__Item0: Into<i64>`
    params: Vec<String>,
}

/// Rewrite the associated type bounds in `text` (see the module docs).
pub fn expand_assoc_bounds(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    // By the item's start.
    let mut items: BTreeMap<usize, (ast::Item, ItemEdits)> = BTreeMap::new();
    let mut fresh = 0usize;
    let lists: Vec<ast::GenericArgList> = file
        .syntax()
        .descendants()
        .filter_map(ast::GenericArgList::cast)
        .filter(|l| {
            l.generic_args().any(|g| {
                matches!(g, ast::GenericArg::AssocTypeArg(a) if a.type_bound_list().is_some() && a.ty().is_none())
            })
        })
        .collect();
    for list in lists {
        // Nested inside another rewritten list: left alone.
        if list
            .syntax()
            .ancestors()
            .skip(1)
            .filter_map(ast::GenericArgList::cast)
            .any(|outer| {
                outer.generic_args().any(|g| {
                    matches!(g, ast::GenericArg::AssocTypeArg(a) if a.type_bound_list().is_some() && a.ty().is_none())
                })
            })
        {
            continue;
        }
        let Some(site) = bound_site(&list) else {
            continue;
        };
        let Some(item) = list.syntax().ancestors().find_map(|a| {
            ast::Fn::cast(a.clone())
                .map(ast::Item::Fn)
                .or_else(|| ast::Impl::cast(a.clone()).map(ast::Item::Impl))
                .or_else(|| ast::Trait::cast(a).map(ast::Item::Trait))
        }) else {
            continue;
        };
        let mut kept: Vec<String> = Vec::new();
        let mut preds = Vec::new();
        let mut params = Vec::new();
        for g in list.generic_args() {
            match &g {
                ast::GenericArg::AssocTypeArg(a)
                    if a.type_bound_list().is_some() && a.ty().is_none() =>
                {
                    let (Some(n), Some(bounds)) = (a.name_ref(), a.type_bound_list()) else {
                        kept.push(g.syntax().text().to_string());
                        continue;
                    };
                    let bounds = bounds.syntax().text().to_string();
                    match &site {
                        Site::Subject(s) => preds.push(format!("{s}::{}: {bounds}", n.text())),
                        Site::ImplArg => {
                            let p = format!("__{}{fresh}", n.text());
                            fresh += 1;
                            kept.push(format!("{} = {p}", n.text()));
                            params.push(format!("{p}: {bounds}"));
                        }
                    }
                }
                _ => kept.push(g.syntax().text().to_string()),
            }
        }
        if matches!(site, Site::ImplArg) && !matches!(item, ast::Item::Fn(_)) {
            continue;
        }
        let r = list.syntax().text_range();
        let turbofish = list.coloncolon_token().is_some();
        let new = if kept.is_empty() {
            String::new()
        } else {
            format!("{}<{}>", if turbofish { "::" } else { "" }, kept.join(", "))
        };
        edits.push((r.start().into(), r.end().into(), new));
        let start: usize = item.syntax().text_range().start().into();
        let entry = items
            .entry(start)
            .or_insert_with(|| (item.clone(), ItemEdits::default()));
        entry.1.preds.extend(preds);
        entry.1.params.extend(params);
    }
    if edits.is_empty() {
        return false;
    }
    for (item, e) in items.values() {
        if !e.preds.is_empty() {
            edits.extend(where_edit(text, item, &e.preds.join(", ")));
        }
        if !e.params.is_empty()
            && let ast::Item::Fn(f) = item
        {
            edits.extend(params_edit(text, f, &e.params.join(", ")));
        }
    }
    crate::alias::apply(text, map, &mut edits);
    true
}

/// Where a bound list with associated type bounds sits.
enum Site {
    /// On a type parameter or a `where` predicate of a plain name (`I`, `Self`).
    Subject(String),
    /// `impl Trait<..>` as a function's parameter type.
    ImplArg,
}

fn bound_site(list: &ast::GenericArgList) -> Option<Site> {
    // list → segment → path → path type → bound → bound list → its owner
    let bound = list.syntax().ancestors().find_map(ast::TypeBound::cast)?;
    // (The list must be the bound's own trait's arguments, not a type argument's.)
    let ast::Type::PathType(pt) = bound.ty()? else {
        return None;
    };
    let seg = pt.path()?.segment()?;
    if seg.generic_arg_list().as_ref() != Some(list) {
        return None;
    }
    let owner = bound.syntax().parent()?.parent()?;
    if let Some(tp) = ast::TypeParam::cast(owner.clone()) {
        return Some(Site::Subject(tp.name()?.text().to_string()));
    }
    if let Some(wp) = ast::WherePred::cast(owner.clone()) {
        // (`I`, `Self`: a plain name.)
        let name = wp.ty()?.syntax().text().to_string();
        let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        return plain.then_some(Site::Subject(name));
    }
    if let Some(it) = ast::ImplTraitType::cast(owner) {
        // A parameter's type, not the return type.
        let in_param = it
            .syntax()
            .ancestors()
            .take_while(|a| !ast::Fn::can_cast(a.kind()))
            .any(|a| a.kind() == SyntaxKind::PARAM);
        return in_param.then_some(Site::ImplArg);
    }
    None
}

/// An insertion adding `preds` to `item`'s `where` clause (made when it has none).
fn where_edit(text: &str, item: &ast::Item, preds: &str) -> Option<(usize, usize, String)> {
    let (wc, before) = match item {
        ast::Item::Fn(f) => (
            f.where_clause(),
            f.body()
                .map(|b| b.syntax().text_range().start())
                .or_else(|| f.semicolon_token().map(|t| t.text_range().start())),
        ),
        ast::Item::Impl(i) => (
            i.where_clause(),
            i.assoc_item_list().map(|l| l.syntax().text_range().start()),
        ),
        ast::Item::Trait(t) => (
            t.where_clause(),
            t.assoc_item_list().map(|l| l.syntax().text_range().start()),
        ),
        _ => return None,
    };
    if let Some(wc) = wc {
        let end: usize = wc.syntax().text_range().end().into();
        let sep = if text[..end].trim_end().ends_with(',') || wc.predicates().next().is_none() {
            " "
        } else {
            ", "
        };
        return Some((end, end, format!("{sep}{preds}")));
    }
    let at: usize = before?.into();
    Some((at, at, format!("where {preds} ")))
}

/// An insertion adding the type parameters `params` to `f`.
fn params_edit(text: &str, f: &ast::Fn, params: &str) -> Option<(usize, usize, String)> {
    match f.generic_param_list() {
        Some(gl) => {
            let at: usize = gl.r_angle_token()?.text_range().start().into();
            let sep = if text[..at].trim_end().ends_with(['<', ',']) {
                " "
            } else {
                ", "
            };
            Some((at, at, format!("{sep}{params}")))
        }
        None => {
            let at: usize = f.name()?.syntax().text_range().end().into();
            Some((at, at, format!("<{params}>")))
        }
    }
}
