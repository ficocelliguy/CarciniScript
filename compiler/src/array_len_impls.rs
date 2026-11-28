//! Impls of one trait for arrays of one element type and different lengths (`impl Fill for [u8;
//! 1]` next to `impl Fill for [u8; 2]`), as a pre-pass over the source text. Arrays are JS arrays
//! whatever their length, so the checker would see two impls for one class; here they become the
//! first impl, each method's body picking the impl's by the length at run time (`match self.len()
//! { 1 => { .. } _ => { .. } }`), and the others are left out. Only when every impl has the same
//! methods with the same signatures, each taking `self`, and nothing else differs. An
//! `OffsetMap` keeps positions pointing at the original text.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericParams, HasName},
};

use crate::mbe::OffsetMap;

/// True if some impl's self type looks like a fixed-length array (a cheap check before parsing).
pub(crate) fn has_array_impls(text: &str) -> bool {
    text.contains("impl") && text.contains(" for [") && text.contains(';')
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// `[E; n]` with a literal length: (E as written, n).
fn array_self(imp: &ast::Impl) -> Option<(String, u128)> {
    let ast::Type::ArrayType(a) = imp.self_ty()? else {
        return None;
    };
    let ast::Expr::Literal(l) = a.const_arg()?.expr()? else {
        return None;
    };
    let ast::LiteralKind::IntNumber(n) = l.kind() else {
        return None;
    };
    Some((squash(&a.ty()?.syntax().text().to_string()), n.value().ok()?))
}

/// An impl's methods by name: (the signature up to the body, the body), when every item is a
/// method with `self` and a body, or a type alias / const (returned as text, which must match).
struct Shape {
    fns: Vec<(String, String, ast::BlockExpr)>,
    others: Vec<String>,
}

fn shape(imp: &ast::Impl) -> Option<Shape> {
    let mut s = Shape { fns: Vec::new(), others: Vec::new() };
    for item in imp.assoc_item_list()?.assoc_items() {
        match item {
            ast::AssocItem::Fn(f) => {
                f.param_list()?.self_param()?;
                let body = f.body()?;
                let name = f.name()?.text().to_string();
                // (The signature from the name to the body, which every impl must spell alike.)
                let start = f.name()?.syntax().text_range().start();
                let end = body.syntax().text_range().start();
                let full = f.syntax().text().to_string();
                let base = f.syntax().text_range().start();
                let sig = squash(&full[usize::from(start - base)..usize::from(end - base)]);
                s.fns.push((name, sig, body));
            }
            ast::AssocItem::TypeAlias(t) => s.others.push(squash(&t.syntax().text().to_string())),
            ast::AssocItem::Const(k) => s.others.push(squash(&k.syntax().text().to_string())),
            ast::AssocItem::MacroCall(_) => return None,
        }
    }
    s.fns.sort_by(|a, b| a.0.cmp(&b.0));
    s.others.sort();
    (!s.fns.is_empty()).then_some(s)
}

pub(crate) fn expand(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    // (trait, element, impl generics) → the impls: (length, impl).
    let mut groups: HashMap<(String, String, String), Vec<(u128, ast::Impl)>> = HashMap::new();
    let mut order: Vec<(String, String, String)> = Vec::new();
    for imp in file.syntax().descendants().filter_map(ast::Impl::cast) {
        let (Some(tr), Some((elem, n))) = (imp.trait_(), array_self(&imp)) else {
            continue;
        };
        let generics = imp.generic_param_list().map(|g| squash(&g.syntax().text().to_string())).unwrap_or_default();
        let key = (squash(&tr.syntax().text().to_string()), elem, generics);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push((n, imp));
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for key in order {
        let impls = &groups[&key];
        if impls.len() < 2 {
            continue;
        }
        // (A length twice is rustc's error.)
        let mut lens: Vec<u128> = impls.iter().map(|(n, _)| *n).collect();
        lens.sort();
        lens.dedup();
        if lens.len() != impls.len() {
            continue;
        }
        let Some(shapes) = impls.iter().map(|(_, imp)| shape(imp)).collect::<Option<Vec<_>>>() else {
            continue;
        };
        let first = &shapes[0];
        let alike = shapes.iter().all(|s| {
            s.others == first.others
                && s.fns.len() == first.fns.len()
                && s.fns.iter().zip(&first.fns).all(|(a, b)| a.0 == b.0 && a.1 == b.1)
        });
        // (Each body the same: nothing to pick; `erased_twins` keeps one.)
        let differ = shapes.iter().any(|s| {
            s.fns.iter().zip(&first.fns).any(|(a, b)| squash(&a.2.syntax().text().to_string()) != squash(&b.2.syntax().text().to_string()))
        });
        if !alike || !differ {
            continue;
        }
        // The first impl's bodies pick by the length; the rest go.
        for (i, (name, _, body)) in first.fns.iter().enumerate() {
            let mut arms = String::from("{ match self.len() {");
            let last = impls.len() - 1;
            for (j, ((n, _), s)) in impls.iter().zip(&shapes).enumerate() {
                let b = &s.fns[i];
                debug_assert_eq!(&b.0, name);
                let pat = if j == last { "_".to_string() } else { n.to_string() };
                arms.push_str(&format!(" {pat} => {},", b.2.syntax().text()));
            }
            arms.push_str(" } }");
            let r = body.syntax().text_range();
            edits.push((r.start().into(), r.end().into(), arms));
        }
        for (_, imp) in &impls[1..] {
            let r = imp.syntax().text_range();
            edits.push((r.start().into(), r.end().into(), String::new()));
        }
    }
    if edits.is_empty() {
        return false;
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
