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
    text.contains("type ") && !aliases(&SourceFile::parse(text, EDITION).tree()).is_empty()
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
            l.generic_args()
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
        let file = SourceFile::parse(text, EDITION).tree();
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
