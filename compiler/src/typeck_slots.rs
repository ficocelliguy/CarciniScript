//! `iter_mut()` over numbers, strings and bools outside a `for` loop: the iterator yields slots
//! (`κσ.$iterMutSlots`, each item's `[1]` reads and writes the element), and the bindings that
//! receive them (`if let Some(x) = it.next()`, `while let Some((i, x)) = it.next()`,
//! `for r in refs` over a collected `Vec<&mut T>`) read and write `x[1]`, so writes reach the
//! Vec. The checker follows where those slots go through adapters, `next()` and `collect()`.
//! A child module of `typeck`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasName},
};

use super::{Checker, strip_parens};
use crate::ty::Ty;

/// Where an item holds slots: the item itself, or some fields of a tuple item.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SlotShape {
    Slot,
    Tuple(Vec<Option<SlotShape>>),
}

/// Whether the `iter_mut()` call `m` is consumed where the code generator loops by index
/// (`for x in v.iter_mut()`, `.for_each(..)`, `.enumerate()` into either, `for .. in
/// v.iter_mut().zip(..)`), so its items needn't be slots.
pub(super) fn iter_mut_indexed(m: &ast::MethodCallExpr) -> bool {
    let up = |n: &ra_ap_syntax::SyntaxNode| {
        let mut n = n.clone();
        while let Some(p) = n
            .parent()
            .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
        {
            n = p;
        }
        n
    };
    let for_iterable = |n: &ra_ap_syntax::SyntaxNode| {
        let n = up(n);
        n.parent()
            .and_then(ast::ForExpr::cast)
            .and_then(|f| f.iterable())
            .is_some_and(|it| it.syntax() == &n)
    };
    let receiver_of = |n: &ra_ap_syntax::SyntaxNode| -> Option<ast::MethodCallExpr> {
        let n = up(n);
        let p = ast::MethodCallExpr::cast(n.parent()?)?;
        p.receiver().is_some_and(|r| r.syntax() == &n).then_some(p)
    };
    if for_iterable(m.syntax()) {
        return true;
    }
    let Some(p) = receiver_of(m.syntax()) else {
        return false;
    };
    match p.name_ref().map(|n| n.text().to_string()).as_deref() {
        Some("for_each") => true,
        Some("enumerate") => {
            for_iterable(p.syntax())
                || receiver_of(p.syntax())
                    .is_some_and(|q| q.name_ref().is_some_and(|n| n.text() == "for_each"))
        }
        Some("zip") => for_iterable(p.syntax()),
        _ => false,
    }
}

impl Checker<'_> {
    /// `v.iter_mut()` of numbers, strings or bools (element type `elem`): a slot iterator unless
    /// it's looped over by index.
    pub(super) fn note_iter_mut(&mut self, m: &ast::MethodCallExpr, elem: &Ty) {
        let z = self.zonk(elem);
        if z.is_js_primitive() && !iter_mut_indexed(m) {
            let r = self.range(m.syntax());
            self.slot_calls.insert(r);
        }
    }

    /// The slot shape of the items of iterator expression `e`, if any hold slots.
    pub(super) fn iter_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        match strip_parens(e) {
            ast::Expr::MethodCallExpr(m) => {
                let name = m.name_ref()?.text().to_string();
                let recv = m.receiver()?;
                match name.as_str() {
                    "iter_mut" if self.slot_calls.contains(&self.range(m.syntax())) => {
                        Some(SlotShape::Slot)
                    }
                    // A collected `Vec<&mut T>`.
                    "iter" | "iter_mut" | "into_iter" => self.vec_slot_shape(&recv),
                    "skip" | "take" | "rev" | "step_by" | "peekable" | "by_ref" | "fuse"
                    | "cycle" => self.iter_slot_shape(&recv),
                    "enumerate" => Some(SlotShape::Tuple(vec![
                        None,
                        Some(self.iter_slot_shape(&recv)?),
                    ])),
                    "zip" => {
                        let a = self.iter_slot_shape(&recv);
                        let b = m.arg_list().and_then(|l| l.args().next()).and_then(|x| {
                            self.iter_slot_shape(&x).or_else(|| self.vec_slot_shape(&x))
                        });
                        (a.is_some() || b.is_some()).then(|| SlotShape::Tuple(vec![a, b]))
                    }
                    _ => None,
                }
            }
            ast::Expr::PathExpr(p) => {
                let n = p.path()?.as_single_name_ref()?.text().to_string();
                match self.slot_locals.get(&n)? {
                    (false, s) => Some(s.clone()),
                    _ => None,
                }
            }
            ast::Expr::RefExpr(r) => self.vec_slot_shape(&r.expr()?),
            _ => None,
        }
    }

    /// What a `for` loop over `e` binds: a slot iterator's items, or a collected `Vec<&mut T>`'s.
    pub(super) fn for_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        self.iter_slot_shape(e).or_else(|| self.vec_slot_shape(e))
    }

    /// The item shape of a local Vec collected from a slot iterator (`refs`, `&mut refs`).
    fn vec_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        match strip_parens(e) {
            ast::Expr::PathExpr(p) => {
                let n = p.path()?.as_single_name_ref()?.text().to_string();
                match self.slot_locals.get(&n)? {
                    (true, s) => Some(s.clone()),
                    _ => None,
                }
            }
            ast::Expr::RefExpr(r) => self.vec_slot_shape(&r.expr()?),
            _ => None,
        }
    }

    /// The shape of what an `Option` of an item holds: `it.next()`, `.nth(n)`, `.last()`, ..
    pub(super) fn option_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        let ast::Expr::MethodCallExpr(m) = strip_parens(e) else {
            return None;
        };
        match m.name_ref()?.text().to_string().as_str() {
            "next" | "next_back" | "nth" | "nth_back" | "last" => {
                self.iter_slot_shape(&m.receiver()?)
            }
            _ => None,
        }
    }

    /// An item itself: `it.next().unwrap()`.
    fn value_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        let ast::Expr::MethodCallExpr(m) = strip_parens(e) else {
            return None;
        };
        match m.name_ref()?.text().to_string().as_str() {
            "unwrap" | "expect" => self.option_slot_shape(&m.receiver()?),
            _ => None,
        }
    }

    /// `let name = init;`: a slot binding (`it.next().unwrap()`), or a local that holds a slot
    /// iterator or a Vec of slots, recorded for later bindings. Before the local is declared.
    pub(super) fn note_slot_let(&mut self, name: &ast::Name, init: Option<&ast::Expr>) {
        let n = name.text().to_string();
        self.slot_locals.remove(&n);
        let Some(init) = init else { return };
        if self.value_slot_shape(init) == Some(SlotShape::Slot) {
            let r = self.range(name.syntax());
            self.slot_bindings.insert(r);
            return;
        }
        if let Some(s) = self.iter_slot_shape(init) {
            self.slot_locals.insert(n, (false, s));
            return;
        }
        if let ast::Expr::MethodCallExpr(m) = strip_parens(init)
            && m.name_ref().is_some_and(|x| x.text() == "collect")
            && let Some(s) = m.receiver().and_then(|r| self.iter_slot_shape(&r))
        {
            self.slot_locals.insert(n, (true, s));
        }
    }

    /// Bind the names of `pat` (already declared) that receive slots: they read and write `[1]`
    /// and are `&mut` references.
    pub(super) fn bind_slot_shape(&mut self, pat: &ast::Pat, shape: &SlotShape) {
        match (pat, shape) {
            (ast::Pat::IdentPat(ip), SlotShape::Slot) if ip.pat().is_none() => {
                let Some(n) = ip.name() else { return };
                let r = self.range(n.syntax());
                self.slot_bindings.insert(r);
                let name = n.text().to_string();
                if let Some(local) = self.scopes.iter_mut().rev().find_map(|s| s.get_mut(&name)) {
                    local.reference = Some(true);
                }
            }
            (ast::Pat::TuplePat(t), SlotShape::Tuple(fields)) => {
                for (p, s) in t.fields().zip(fields) {
                    if let Some(s) = s {
                        self.bind_slot_shape(&p, s);
                    }
                }
            }
            (ast::Pat::ParenPat(p), _) => {
                if let Some(inner) = p.pat() {
                    self.bind_slot_shape(&inner, shape);
                }
            }
            _ => {}
        }
    }

    /// `Some(p)` matched against an `Option` of slot items.
    pub(super) fn bind_slot_option(&mut self, scrutinee: Option<&ast::Expr>, pat: &ast::Pat) {
        let Some(shape) = scrutinee.and_then(|s| self.option_slot_shape(s)) else {
            return;
        };
        if let ast::Pat::TupleStructPat(tp) = pat
            && tp
                .path()
                .and_then(|p| p.segment())
                .is_some_and(|s| s.syntax().text() == "Some")
            && let Some(inner) = tp.fields().next()
        {
            self.bind_slot_shape(&inner, &shape);
        }
    }
}
