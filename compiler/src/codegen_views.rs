//! Read-only slice views. `f(&v[a..b])` copies the range (`κσ.$slice`), so a function that
//! recurses on `&v[1..]` is quadratic. When `f` is one of the file's own functions and its
//! parameter only reads the slice a little (its length, an element outside loops, a sub-slice
//! for another such call, an iteration), the call passes a view instead (`κσ.$sliceMut`'s Proxy,
//! whose element reads cost a trap each). A child module of `codegen`.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList, HasName},
};

/// For each of the file's functions with `&[T]` parameters, which of them a view is safe for.
pub(super) fn view_safe_params(file: &ast::SourceFile) -> HashMap<String, Vec<bool>> {
    let fns: Vec<ast::Fn> = crate::adt::all_items(file)
        .into_iter()
        .filter_map(|i| match i {
            ast::Item::Fn(f) => Some(f),
            _ => None,
        })
        .collect();
    // Every `&[T]` parameter to start with; then the ones a view would make slow or that
    // escape are dropped until nothing changes (a parameter passed on to another function
    // needs that one's parameter safe too).
    let mut safe: HashMap<String, Vec<bool>> = HashMap::new();
    for f in &fns {
        let Some(name) = f.name() else { continue };
        let flags: Vec<bool> = f
            .param_list()
            .map(|pl| pl.params().map(|p| is_slice_ref(p.ty())).collect())
            .unwrap_or_default();
        if flags.iter().any(|b| *b) {
            safe.insert(name.text().to_string(), flags);
        }
    }
    loop {
        let mut changed = false;
        for f in &fns {
            let Some(name) = f.name() else { continue };
            let Some(flags) = safe.get(name.text()).cloned() else {
                continue;
            };
            let names: Vec<Option<String>> = f
                .param_list()
                .map(|pl| {
                    pl.params()
                        .map(|p| match p.pat() {
                            Some(ast::Pat::IdentPat(ip)) if ip.pat().is_none() => {
                                ip.name().map(|n| n.text().to_string())
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (i, flag) in flags.iter().enumerate() {
                if !*flag {
                    continue;
                }
                let ok = match (names.get(i), f.body()) {
                    (Some(Some(p)), Some(body)) => param_view_safe(&body, p, &safe),
                    _ => false,
                };
                if !ok {
                    safe.get_mut(name.text()).unwrap()[i] = false;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    safe.retain(|_, v| v.iter().any(|b| *b));
    safe
}

/// `&[T]` (not `&mut [T]`, whose argument is a view already).
fn is_slice_ref(ty: Option<ast::Type>) -> bool {
    match ty {
        Some(ast::Type::RefType(r)) => {
            r.mut_token().is_none() && matches!(r.ty(), Some(ast::Type::SliceType(_)))
        }
        _ => false,
    }
}

/// Whether every use of the parameter `p` in `body` reads it in a way that's fine through a
/// view. (A name bound again in the body is checked as the parameter, which only rejects more.)
fn param_view_safe(body: &ast::BlockExpr, p: &str, safe: &HashMap<String, Vec<bool>>) -> bool {
    body.syntax()
        .descendants()
        .filter_map(ast::PathExpr::cast)
        .filter(|e| is_name(e, p))
        .all(|e| use_ok(e.syntax(), body.syntax(), safe))
}

fn is_name(e: &ast::PathExpr, p: &str) -> bool {
    e.path()
        .and_then(|path| path.as_single_name_ref())
        .is_some_and(|n| n.text() == p)
}

/// Methods on a slice that read O(1) of it, or run on the base array at once.
const READ_METHODS: &[&str] = &[
    "len",
    "is_empty",
    "first",
    "last",
    "get",
    "iter",
    "into_iter",
    "contains",
    "to_vec",
    "to_owned",
    "join",
    "concat",
    "starts_with",
    "ends_with",
    "binary_search",
    "binary_search_by",
    "binary_search_by_key",
    "partition_point",
    "is_sorted",
    "windows",
    "chunks",
    "chunks_exact",
    "split_at",
    "split_first",
    "split_last",
];
/// Methods whose argument the runtime iterates or copies at once.
const ARG_METHODS: &[&str] = &[
    "extend_from_slice",
    "extend",
    "starts_with",
    "ends_with",
    "copy_from_slice",
    "clone_from_slice",
];

fn use_ok(node: &SyntaxNode, body: &SyntaxNode, safe: &HashMap<String, Vec<bool>>) -> bool {
    // Up through `(p)` and `&p`.
    let mut node = node.clone();
    while let Some(parent) = node.parent() {
        if matches!(parent.kind(), SyntaxKind::PAREN_EXPR | SyntaxKind::REF_EXPR) {
            node = parent;
        } else {
            break;
        }
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if let Some(m) = ast::MethodCallExpr::cast(parent.clone()) {
        let name = m
            .name_ref()
            .map(|n| n.text().to_string())
            .unwrap_or_default();
        return if m.receiver().is_some_and(|r| r.syntax() == &node) {
            READ_METHODS.contains(&name.as_str())
        } else {
            ARG_METHODS.contains(&name.as_str())
        };
    }
    if let Some(ix) = ast::IndexExpr::cast(parent.clone())
        && ix.base().is_some_and(|b| b.syntax() == &node)
    {
        // A sub-slice is copied or viewed; an element read costs a trap: fine outside loops.
        return matches!(ix.index(), Some(ast::Expr::RangeExpr(_))) || !in_loop(&node, body);
    }
    if let Some(f) = ast::ForExpr::cast(parent.clone()) {
        return f.iterable().is_some_and(|it| it.syntax() == &node);
    }
    if let Some(c) = ast::CallExpr::cast(parent.clone()) {
        // Passed on whole: that function's parameter must take a view too.
        let callee = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|path| path.as_single_name_ref())
                .map(|n| n.text().to_string()),
            _ => None,
        };
        let Some(callee) = callee else { return false };
        let index = c
            .arg_list()
            .and_then(|l| l.args().position(|a| a.syntax() == &node));
        return match (safe.get(&callee), index) {
            (Some(flags), Some(i)) => flags.get(i).copied().unwrap_or(false),
            _ => false,
        };
    }
    // Matched against (slice patterns read a few elements).
    match parent.kind() {
        SyntaxKind::MATCH_EXPR | SyntaxKind::LET_EXPR => true,
        SyntaxKind::LET_STMT => ast::LetStmt::cast(parent)
            .is_some_and(|l| matches!(l.pat(), Some(ast::Pat::SlicePat(_)))),
        _ => false,
    }
}

/// Whether `node` is inside a loop or closure within `body`.
fn in_loop(node: &SyntaxNode, body: &SyntaxNode) -> bool {
    node.ancestors().take_while(|a| a != body).any(|a| {
        matches!(
            a.kind(),
            SyntaxKind::FOR_EXPR
                | SyntaxKind::WHILE_EXPR
                | SyntaxKind::LOOP_EXPR
                | SyntaxKind::CLOSURE_EXPR
        )
    })
}
