//! The last use of a `Copy` value's binding, which needn't copy it: `let p = Point { .. };
//! v.push(p);` pushes `p` itself, where every other read of a place copies (`by_value`), since
//! JS would share the object. Nothing reads the binding afterwards, so nothing can tell.
//!
//! Conservative, and by name: a `let` binding `x` holding a value of its own (see `scope_of`)
//! qualifies when no other binding in its scope is named `x`, and every mention of `x` there is
//! either the last one (a plain path, not in a loop or closure that the binding is outside of)
//! or can't keep hold of the value: a scalar read through it (`x.len`, `x.dist(o)`, `x == y`)
//! or a formatting macro's argument. A borrow (`&x`, `&x.inner`), a closure, a pattern or any
//! other macro mentioning it rules it out.
//! Earlier copies (`f(x)` before the last use) are still copies.

use std::collections::HashSet;

use ra_ap_syntax::{
    AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, TextRange,
    ast::{self, HasName},
};

use super::Codegen;
use crate::ty::Ty;

/// Macros that only read their arguments while they run.
const FORMAT_MACROS: &[&str] = &[
    "println",
    "print",
    "eprintln",
    "eprint",
    "format",
    "write",
    "writeln",
    "panic",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

impl Codegen<'_> {
    /// Whether `p` (a read of a `Copy` value, about to be copied) is its binding's last use.
    pub(super) fn last_copy_use(&self, p: &ast::PathExpr) -> bool {
        // (A macro argument is parsed on its own: not a node of the file.)
        if self.base != 0 {
            return false;
        }
        let Some(root) = &self.file_root else {
            return false;
        };
        let set = self.last_copies.get_or_init(|| {
            let ty = |n: &SyntaxNode| {
                let r = n.text_range();
                self.types.get(r.start().into(), r.end().into()).cloned()
            };
            last_uses(root, &ty)
        });
        set.contains(&self.range_of(p.syntax()))
    }
}

/// The ranges of the path expressions that are a qualifying binding's last use.
fn last_uses(root: &SyntaxNode, ty: &dyn Fn(&SyntaxNode) -> Option<Ty>) -> HashSet<(usize, usize)> {
    let mut out = HashSet::new();
    for ip in root.descendants().filter_map(ast::IdentPat::cast) {
        if ip.ref_token().is_some() || ip.pat().is_some() {
            continue;
        }
        let Some(name) = ip.name() else { continue };
        let Some(scope) = scope_of(&ip) else { continue };
        if let Some(r) = last_use(&ip, &name, scope, ty) {
            out.insert((r.start().into(), r.end().into()));
        }
    }
    out
}

/// Where a `let` binding is visible, from the end of its `let` to the end of the block, when its
/// value is certainly its own: a new value, or a place read the `let` copies. (Not parameters,
/// which a function passed as a value gets from the runtime, nor pattern bindings, which may
/// name a place; nor `.copied()` of an `Option`, which is the same object in JS.)
fn scope_of(ip: &ast::IdentPat) -> Option<(TextRange, SyntaxNode)> {
    let l = ip.syntax().parent().and_then(ast::LetStmt::cast)?;
    let block = l.syntax().parent()?;
    if !ast::StmtList::can_cast(block.kind()) || l.let_else().is_some() {
        return None;
    }
    let mut init = l.initializer()?;
    while let ast::Expr::ParenExpr(p) = &init {
        init = p.expr()?;
    }
    let fresh = match &init {
        ast::Expr::RecordExpr(_)
        | ast::Expr::TupleExpr(_)
        | ast::Expr::ArrayExpr(_)
        | ast::Expr::CallExpr(_)
        | ast::Expr::BinExpr(_)
        | ast::Expr::PathExpr(_)
        | ast::Expr::FieldExpr(_)
        | ast::Expr::IndexExpr(_)
        | ast::Expr::PrefixExpr(_) => true,
        ast::Expr::MethodCallExpr(m) => m.name_ref().is_some_and(|n| n.text() == "clone"),
        _ => false,
    };
    fresh.then(|| {
        (
            TextRange::new(l.syntax().text_range().end(), block.text_range().end()),
            l.syntax().clone(),
        )
    })
}

/// The last use of the binding `ip` in `scope`, when it qualifies (see the module docs).
fn last_use(
    ip: &ast::IdentPat,
    name: &ast::Name,
    (scope, decl): (TextRange, SyntaxNode),
    ty: &dyn Fn(&SyntaxNode) -> Option<Ty>,
) -> Option<TextRange> {
    let text = name.text().to_string();
    let text = text.as_str();
    let block = ip
        .syntax()
        .ancestors()
        .find(|a| ast::StmtList::can_cast(a.kind()) || ast::Fn::can_cast(a.kind()))?;
    let mentions: Vec<SyntaxToken> = block
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::IDENT && t.text() == text)
        .filter(|t| scope.contains_range(t.text_range()))
        .collect();
    let (last, earlier) = mentions.split_last()?;
    // The last one: a plain path expression `x`.
    let last_path = plain_path(last)?;
    if crosses(last_path.syntax(), &decl, |k| {
        matches!(k, SyntaxKind::LOOP_EXPR | SyntaxKind::WHILE_EXPR | SyntaxKind::FOR_EXPR)
    }) || crosses(last_path.syntax(), &decl, |k| k == SyntaxKind::CLOSURE_EXPR)
    {
        return None;
    }
    if earlier.iter().all(|t| harmless(t, &decl, ty)) {
        Some(last_path.syntax().text_range())
    } else {
        None
    }
}

/// The path expression `x` a token is all of, if it is.
fn plain_path(t: &SyntaxToken) -> Option<ast::PathExpr> {
    let name_ref = t.parent().filter(|p| p.kind() == SyntaxKind::NAME_REF)?;
    let path = name_ref.parent()?.parent().and_then(ast::Path::cast)?;
    if path.qualifier().is_some() {
        return None;
    }
    path.syntax().parent().and_then(ast::PathExpr::cast)
}

/// Whether one of `n`'s ancestors of a kind `is` is inside the binding's scope but doesn't hold
/// its declaration `decl` (a loop runs the use again; a closure may run it any number of times).
fn crosses(n: &SyntaxNode, decl: &SyntaxNode, is: impl Fn(SyntaxKind) -> bool) -> bool {
    let at = decl.text_range();
    n.ancestors()
        .take_while(|a| !a.text_range().contains_range(at))
        .any(|a| is(a.kind()))
}

/// Whether a mention of the binding before its last use can't keep hold of its value.
fn harmless(t: &SyntaxToken, decl: &SyntaxNode, ty: &dyn Fn(&SyntaxNode) -> Option<Ty>) -> bool {
    if crosses(&t.parent().unwrap(), decl, |k| k == SyntaxKind::CLOSURE_EXPR) {
        return false;
    }
    // In a macro's tokens: only a formatting macro's, which reads them while it runs.
    if let Some(tt) = t.parent().filter(|p| p.kind() == SyntaxKind::TOKEN_TREE) {
        return tt
            .ancestors()
            .find_map(ast::MacroCall::cast)
            .and_then(|m| m.path()?.segment()?.name_ref())
            .is_some_and(|n| FORMAT_MACROS.iter().any(|m| n.text() == *m));
    }
    let Some(p) = plain_path(t) else {
        return false;
    };
    // `x`, `x.a.b`, `x.f(..)` (as far as it goes): a scalar value read through the binding.
    let mut e = p.syntax().clone();
    loop {
        let Some(up) = e.parent() else { return false };
        let receiver = match up.kind() {
            SyntaxKind::FIELD_EXPR => ast::FieldExpr::cast(up.clone()).and_then(|f| f.expr()),
            SyntaxKind::METHOD_CALL_EXPR => {
                ast::MethodCallExpr::cast(up.clone()).and_then(|m| m.receiver())
            }
            SyntaxKind::PAREN_EXPR => ast::ParenExpr::cast(up.clone()).and_then(|x| x.expr()),
            _ => None,
        };
        if receiver.is_none_or(|r| r.syntax() != &e) {
            break;
        }
        e = up;
    }
    let parent = e.parent();
    // (Not `&x.len` nor `&mut x.a`: a slot over the place.)
    if parent
        .as_ref()
        .is_some_and(|p| p.kind() == SyntaxKind::REF_EXPR)
    {
        return false;
    }
    // `x == y`, `x < y`: a comparison reads both sides.
    if e == *p.syntax()
        && let Some(b) = parent.as_ref().and_then(|p| ast::BinExpr::cast(p.clone()))
        && matches!(b.op_kind(), Some(ast::BinaryOp::CmpOp(_)))
    {
        return true;
    }
    e != *p.syntax()
        && matches!(
            ty(&e),
            Some(
                Ty::Int(_)
                    | Ty::IntLit
                    | Ty::Float { .. }
                    | Ty::Bool
                    | Ty::Char
                    | Ty::Str
                    | Ty::Unit
            )
        )
}
