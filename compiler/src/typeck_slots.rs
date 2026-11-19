//! `iter_mut()` over numbers, strings and bools outside a `for` loop: the iterator yields slots
//! (`κσ.$iterMutSlots`, each item's `[1]` reads and writes the element), and the bindings that
//! receive them (`if let Some(x) = it.next()`, `while let Some((i, x)) = it.next()`,
//! `for r in refs` over a collected `Vec<&mut T>`) read and write `x[1]`, so writes reach the
//! Vec. The checker follows where those slots go through adapters, `next()` and `collect()`.
//! A child module of `typeck`.

use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList, HasName, UnaryOp},
};

use super::{Checker, is_place_expr, strip_parens};
use crate::ty::Ty;

/// Where `place.as_mut()` of an `Option` of a number, string or bool stands for a slot over the
/// place (`κσ.$optSlot`): unwrapped into a deref or a kept position, kept itself (an argument,
/// a `let`, a return), handed to a closure (`.map(|n| ..)`, `.is_some_and(..)`), or matched or
/// unwrapped after an adapter (`if let Some(n) = o.as_mut().filter(..)`). Matched directly, the
/// bindings alias the place instead (`alias_place_bindings`).
pub(super) fn opt_slot_position(m: &ast::MethodCallExpr) -> bool {
    let mut top = m.clone();
    loop {
        let Some(parent) = method_parent(&top) else { break };
        match parent.name_ref().map(|n| n.text().to_string()).as_deref() {
            Some("filter" | "inspect" | "take_if") => top = parent,
            Some("map" | "and_then" | "is_some_and" | "is_none_or" | "map_or" | "map_or_else") => {
                return true;
            }
            _ => break,
        }
    }
    (top.syntax() != m.syntax() && super::matched_position(&top))
        || super::unwrapped_position(&top)
        || keep_position(top.syntax())
}

/// The method call `m` is the receiver of, if any.
fn method_parent(m: &ast::MethodCallExpr) -> Option<ast::MethodCallExpr> {
    let mut n = m.syntax().clone();
    while let Some(p) = n.parent().filter(|p| p.kind() == SyntaxKind::PAREN_EXPR) {
        n = p;
    }
    let p = ast::MethodCallExpr::cast(n.parent()?)?;
    p.receiver().is_some_and(|r| r.syntax() == &n).then_some(p)
}

/// Whether the expression at `n` is where a `&mut i32`-style value is kept rather than read: a
/// `let` initializer, a `return`, a call's argument, a struct field's value, the right side of an
/// assignment, or the tail of a function body or of an `if` / `match` / block in one of those.
/// A `get_mut`-style call there may yield its slot.
pub(super) fn keep_position(n: &SyntaxNode) -> bool {
    let mut n = n.clone();
    loop {
        let Some(p) = n.parent() else { return false };
        match p.kind() {
            SyntaxKind::PAREN_EXPR => {}
            SyntaxKind::LET_STMT => {
                return ast::LetStmt::cast(p)
                    .and_then(|l| l.initializer())
                    .is_some_and(|i| i.syntax() == &n);
            }
            SyntaxKind::RETURN_EXPR | SyntaxKind::ARG_LIST | SyntaxKind::RECORD_EXPR_FIELD => {
                return true;
            }
            SyntaxKind::BIN_EXPR => {
                return ast::BinExpr::cast(p).is_some_and(|b| {
                    matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { op: None }))
                        && b.rhs().is_some_and(|r| r.syntax() == &n)
                });
            }
            SyntaxKind::STMT_LIST => {
                let is_tail = ast::StmtList::cast(p.clone())
                    .and_then(|l| l.tail_expr())
                    .is_some_and(|t| t.syntax() == &n);
                if !is_tail {
                    return false;
                }
                let Some(block) = p.parent() else { return false };
                match block.parent().map(|pp| pp.kind()) {
                    Some(SyntaxKind::FN) => return true,
                    Some(SyntaxKind::CLOSURE_EXPR) | None => return false,
                    _ => {}
                }
                n = block;
            }
            SyntaxKind::MATCH_ARM => {
                let arm_expr = ast::MatchArm::cast(p.clone())
                    .and_then(|a| a.expr())
                    .is_some_and(|e| e.syntax() == &n);
                if !arm_expr {
                    return false;
                }
                // arm → arm list → match
                let Some(list) = p.parent() else { return false };
                let Some(me) = list.parent() else { return false };
                n = me;
            }
            SyntaxKind::IF_EXPR => {
                // (The condition isn't a branch.)
                let is_branch = ast::IfExpr::cast(p.clone()).is_some_and(|i| {
                    i.then_branch().is_some_and(|b| b.syntax() == &n)
                        || i.else_branch().is_some_and(|b| match b {
                            ast::ElseBranch::Block(b) => b.syntax() == &n,
                            ast::ElseBranch::IfExpr(e) => e.syntax() == &n,
                        })
                });
                if !is_branch {
                    return false;
                }
                n = p;
            }
            _ => return false,
        }
    }
}

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
    /// What a function declared `-> &mut T` / `-> Option<&mut T>` returns when `T` is a number,
    /// string, bool or `Option` (`written`: the return type as written, `ret`: lowered): a slot
    /// over the place (`Some(false)`), or `null` / a slot (`Some(true)`). `None` for other
    /// return types: a struct or collection is shared by reference anyway.
    pub(super) fn ret_slot_kind(&self, written: Option<ast::Type>, ret: &Ty) -> Option<bool> {
        let w = written?;
        if !super::written_mut_ref(&w) {
            return None;
        }
        if matches!(w, ast::Type::RefType(_)) {
            return self.cellable(ret).then_some(false);
        }
        match self.zonk(ret) {
            Ty::Option(inner) if self.cellable(&inner) => Some(true),
            _ => None,
        }
    }

    /// Which parameters are written `Option<&mut T>` with `T` a number, string, bool or
    /// `Option`: they hold `null` or a slot.
    pub(super) fn opt_slot_params(&self, f: &ast::Fn, params: &[Ty]) -> Vec<bool> {
        f.param_list()
            .map(|pl| {
                pl.params()
                    .zip(params)
                    .map(|(p, t)| {
                        p.ty().is_some_and(|w| {
                            matches!(w, ast::Type::PathType(_)) && super::written_mut_ref(&w)
                        }) && matches!(self.zonk(t), Ty::Option(inner) if self.cellable(&inner))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `e` sits where a `&mut T` value (`T` a number, string, bool or `Option`) is kept rather
    /// than read (`keep_position`): the slot itself is wanted. Marks what codegen needs
    /// (`place_slots` for `&mut s.count`, a box for `&mut local`, `slot_keeps` so a local's slot
    /// is passed on and a slot-valued call isn't read) and returns whether `e` yields a slot.
    /// Walks into `if` / `match` / block tails.
    pub(super) fn keep_slot(&mut self, e: &ast::Expr) -> bool {
        let e = strip_parens(e);
        match &e {
            ast::Expr::RefExpr(r) => {
                if r.mut_token().is_none() {
                    return false;
                }
                let Some(inner) = r.expr() else { return false };
                let inner = strip_parens(&inner);
                let Some(t) = self.raw.get(&self.range(inner.syntax())).cloned() else {
                    return false;
                };
                if !self.cellable(&t) {
                    return false;
                }
                match &inner {
                    // (`&mut v[a..b]` is a view, not a slot.)
                    ast::Expr::IndexExpr(ix)
                        if matches!(ix.index(), Some(ast::Expr::RangeExpr(_))) =>
                    {
                        false
                    }
                    ast::Expr::FieldExpr(_) | ast::Expr::IndexExpr(_) => {
                        let rr = self.range(r.syntax());
                        self.place_slots.insert(rr);
                        true
                    }
                    // `&mut x` of a local: its box (a slot over it); `&mut r` / `&mut *r` of a
                    // `&mut` local: its slot or cell.
                    ast::Expr::PathExpr(p) => self.keep_local_ref(r, p),
                    ast::Expr::PrefixExpr(pe) if pe.op_kind() == Some(UnaryOp::Deref) => {
                        match pe.expr().map(|x| strip_parens(&x)) {
                            Some(ast::Expr::PathExpr(p)) => self.keep_local_ref(r, &p),
                            _ => false,
                        }
                    }
                    _ => false,
                }
            }
            // A `&mut` local passed on as the `&mut` itself (`return r`, `W { n: r }`, `q = r`).
            ast::Expr::PathExpr(p) => {
                if self.is_ref_local(&e) {
                    let pr = self.range(p.syntax());
                    self.slot_keeps.insert(pr);
                    true
                } else {
                    false
                }
            }
            // A call returning a slot, `o.unwrap()` of an `Option<&mut i32>` value, a `&mut i32`
            // field: kept as is (not read through `[1]`).
            ast::Expr::CallExpr(_) | ast::Expr::MethodCallExpr(_) | ast::Expr::FieldExpr(_) => {
                let r = self.range(e.syntax());
                let slot = self.slot_values.contains(&r)
                    || self.value_slot_shape(&e) == Some(SlotShape::Slot)
                    || self.fn_ret_slot(&e) == Some(false);
                if slot {
                    self.slot_keeps.insert(r);
                }
                slot
            }
            ast::Expr::IfExpr(i) => {
                let mut any = false;
                if let Some(b) = i.then_branch() {
                    any |= self.keep_slot(&ast::Expr::BlockExpr(b));
                }
                match i.else_branch() {
                    Some(ast::ElseBranch::Block(b)) => any |= self.keep_slot(&ast::Expr::BlockExpr(b)),
                    Some(ast::ElseBranch::IfExpr(e)) => any |= self.keep_slot(&ast::Expr::IfExpr(e)),
                    None => {}
                }
                any
            }
            ast::Expr::MatchExpr(m) => {
                let arms: Vec<ast::Expr> = m
                    .match_arm_list()
                    .into_iter()
                    .flat_map(|l| l.arms())
                    .filter_map(|a| a.expr())
                    .collect();
                let mut any = false;
                for a in arms {
                    any |= self.keep_slot(&a);
                }
                any
            }
            ast::Expr::BlockExpr(b) => match b.stmt_list().and_then(|l| l.tail_expr()) {
                Some(t) => self.keep_slot(&t),
                None => false,
            },
            _ => false,
        }
    }

    /// `&mut x` (`r`: the `&mut`, `p`: `x`) kept as a slot: a plain local goes into a box, which
    /// codegen hands out as a slot over it; a `&mut` local's slot or cell is passed on.
    fn keep_local_ref(&mut self, r: &ast::RefExpr, p: &ast::PathExpr) -> bool {
        let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return false;
        };
        let Some(local) = self.lookup(name.text().to_string().as_str()).cloned() else {
            return false;
        };
        if local.reference == Some(true) {
            let pr = self.range(p.syntax());
            self.slot_keeps.insert(pr);
        } else {
            self.box_if_needed(&ast::Expr::PathExpr(p.clone()));
        }
        let rr = self.range(r.syntax());
        self.slot_keeps.insert(rr);
        true
    }

    /// A local holding a `&mut` to a number, string, bool or `Option` (a slot or a cell).
    pub(super) fn is_ref_local(&self, e: &ast::Expr) -> bool {
        let ast::Expr::PathExpr(p) = strip_parens(e) else {
            return false;
        };
        let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return false;
        };
        self.lookup(name.text().to_string().as_str())
            .is_some_and(|l| l.reference == Some(true) && self.cellable(&l.ty))
    }

    /// `e` sits where an `Option<&mut T>` value is kept: `Some(&mut place)`, `place.as_mut()`,
    /// a `get_mut`-style call, a call of one of the file's functions returning one, a local
    /// holding one, or `if` / `match` / blocks of those. Marks what codegen needs and returns
    /// whether `e` yields `null` or a slot. (`None` alone is nothing to mark: false.)
    pub(super) fn keep_opt_slot(&mut self, e: &ast::Expr) -> bool {
        let e = strip_parens(e);
        match &e {
            ast::Expr::CallExpr(c) => {
                if let Some(ast::Expr::PathExpr(p)) = c.expr()
                    && p.syntax().text() == "Some"
                    && let Some(a) = c.arg_list().and_then(|l| l.args().next())
                {
                    return self.keep_slot(&a);
                }
                self.fn_ret_slot(&e) == Some(true)
            }
            ast::Expr::PathExpr(_) => self.option_slot_shape(&e).is_some(),
            ast::Expr::MethodCallExpr(m) => {
                let name = m.name_ref().map(|n| n.text().to_string()).unwrap_or_default();
                if name == "as_mut" && self.as_mut_of_place(&e) {
                    return self.mark_as_mut_slot_kept(m);
                }
                // `o.as_mut().filter(..)`: the same `Option<&mut T>`, handed on.
                if matches!(name.as_str(), "filter" | "inspect" | "take_if")
                    && let Some(r) = m.receiver()
                {
                    return self.keep_opt_slot(&r);
                }
                self.option_slot_shape(&e) == Some(SlotShape::Slot)
            }
            ast::Expr::IfExpr(i) => {
                let mut any = false;
                if let Some(b) = i.then_branch() {
                    any |= self.keep_opt_slot(&ast::Expr::BlockExpr(b));
                }
                match i.else_branch() {
                    Some(ast::ElseBranch::Block(b)) => {
                        any |= self.keep_opt_slot(&ast::Expr::BlockExpr(b))
                    }
                    Some(ast::ElseBranch::IfExpr(e)) => {
                        any |= self.keep_opt_slot(&ast::Expr::IfExpr(e))
                    }
                    None => {}
                }
                any
            }
            ast::Expr::MatchExpr(m) => {
                let arms: Vec<ast::Expr> = m
                    .match_arm_list()
                    .into_iter()
                    .flat_map(|l| l.arms())
                    .filter_map(|a| a.expr())
                    .collect();
                let mut any = false;
                for a in arms {
                    any |= self.keep_opt_slot(&a);
                }
                any
            }
            ast::Expr::BlockExpr(b) => match b.stmt_list().and_then(|l| l.tail_expr()) {
                Some(t) => self.keep_opt_slot(&t),
                None => false,
            },
            _ => false,
        }
    }

    /// `place.as_mut()` of an `Option` of a number, string or bool, kept as an
    /// `Option<&mut T>` value: `κσ.$optSlot` over the place (a local's needs the local boxed).
    fn mark_as_mut_slot_kept(&mut self, m: &ast::MethodCallExpr) -> bool {
        let Some(recv) = m.receiver() else { return false };
        let recv = strip_parens(&recv);
        let Some(t) = self.raw.get(&self.range(recv.syntax())).cloned() else {
            return false;
        };
        let Ty::Option(inner) = self.zonk(&t) else {
            return false;
        };
        if !self.zonk(&inner).is_js_primitive() || !is_place_expr(&recv) {
            return false;
        }
        if matches!(recv, ast::Expr::PathExpr(_)) {
            self.box_if_needed(&recv);
        }
        let r = self.range(m.syntax());
        self.slot_calls.insert(r);
        true
    }

    /// What a call of one of the file's functions or methods returns, by its declared type:
    /// `Some(false)` for `-> &mut i32` (a slot), `Some(true)` for `-> Option<&mut i32>`.
    pub(super) fn fn_ret_slot(&self, e: &ast::Expr) -> Option<bool> {
        match strip_parens(e) {
            ast::Expr::CallExpr(c) => {
                let ast::Expr::PathExpr(p) = c.expr()? else {
                    return None;
                };
                let name = p.path()?.as_single_name_ref()?.text().to_string();
                self.fns.get(name.as_str())?.ret_slot
            }
            ast::Expr::MethodCallExpr(m) => {
                let recv = m.receiver()?;
                let rt = self.raw.get(&self.range(recv.syntax()))?;
                let Ty::Adt(adt, _) = self.zonk(rt) else {
                    return None;
                };
                let name = m.name_ref()?.text().to_string();
                self.adts.method(&adt, &name)?.ret_slot
            }
            _ => None,
        }
    }

    /// Whether assigning to `e` goes through a `*` of a slot-valued expression (`*w.n += 1` with
    /// `n: &mut i32`, `*s.at(i) = 0`): a write through the reference, which needs no `mut`.
    pub(super) fn through_slot(&self, e: &ast::Expr) -> bool {
        let mut cur = strip_parens(e);
        loop {
            cur = match &cur {
                ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => {
                    let Some(inner) = p.expr() else { return false };
                    let inner = strip_parens(&inner);
                    if self.slot_values.contains(&self.range(inner.syntax())) {
                        return true;
                    }
                    inner
                }
                ast::Expr::FieldExpr(f) => match f.expr() {
                    Some(b) => strip_parens(&b),
                    None => return false,
                },
                ast::Expr::IndexExpr(ix) => match ix.base() {
                    Some(b) => strip_parens(&b),
                    None => return false,
                },
                _ => return false,
            };
        }
    }

    /// `v.iter_mut()` of numbers, strings or bools (element type `elem`): a slot iterator unless
    /// it's looped over by index.
    pub(super) fn note_iter_mut(&mut self, m: &ast::MethodCallExpr, elem: &Ty) {
        let z = self.zonk(elem);
        if z.is_js_primitive() && !iter_mut_indexed(m) {
            let r = self.range(m.syntax());
            self.slot_calls.insert(r);
        }
    }

    /// `map.values_mut()` / `map.iter_mut()` of numbers, strings or bools (value type `v`): a slot
    /// iterator (`κσ.$mapMutSlots`) unless a `for` loop iterates it directly.
    pub(super) fn note_map_iter_mut(&mut self, m: &ast::MethodCallExpr, v: &Ty) {
        let mut n = m.syntax().clone();
        while let Some(p) = n
            .parent()
            .filter(|p| p.kind() == ra_ap_syntax::SyntaxKind::PAREN_EXPR)
        {
            n = p;
        }
        let for_iterable = n
            .parent()
            .and_then(ast::ForExpr::cast)
            .and_then(|f| f.iterable())
            .is_some_and(|it| it.syntax() == &n);
        if self.zonk(v).is_js_primitive() && !for_iterable {
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
                    // A map's `iter_mut()` yields `(k, slot)`.
                    "iter_mut" if self.slot_calls.contains(&self.range(m.syntax())) => {
                        let map = self.raw.get(&self.range(recv.syntax())).is_some_and(|t| {
                            match self.zonk(t) {
                                Ty::Map(..) => true,
                                // (`BTreeMap`, `IndexMap`: a map underneath.)
                                Ty::Coll(_, view) => matches!(*view, Ty::Map(..)),
                                _ => false,
                            }
                        });
                        Some(if map {
                            SlotShape::Tuple(vec![None, Some(SlotShape::Slot)])
                        } else {
                            SlotShape::Slot
                        })
                    }
                    "values_mut" if self.slot_calls.contains(&self.range(m.syntax())) => {
                        Some(SlotShape::Slot)
                    }
                    // (`for v in map.values_mut()` writes through the map's own slots.)
                    "values_mut" => None,
                    // A collected `Vec<&mut T>`.
                    "iter" | "iter_mut" | "into_iter" => self.vec_slot_shape(&recv),
                    "skip" | "take" | "rev" | "step_by" | "peekable" | "by_ref" | "fuse"
                    | "cycle" | "filter" | "skip_while" | "take_while" | "inspect" => {
                        self.iter_slot_shape(&recv)
                    }
                    // `rows.iter_mut().flat_map(|row| row.iter_mut())`: the closure's slots.
                    "flat_map" => {
                        let arg = m.arg_list()?.args().next()?;
                        let ast::Expr::ClosureExpr(c) = strip_parens(&arg) else {
                            return None;
                        };
                        let mut body = c.body()?;
                        while let ast::Expr::BlockExpr(b) = &body {
                            body = b.tail_expr()?;
                        }
                        self.iter_slot_shape(&body)
                    }
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
                    _ => self.returned_slots(e),
                }
            }
            ast::Expr::PathExpr(p) => {
                let n = p.path()?.as_single_name_ref()?.text().to_string();
                match self.slot_locals.get(&n) {
                    Some((false, s)) => Some(s.clone()),
                    Some(_) => None,
                    // A parameter: `it: impl Iterator<Item = &mut i32>`.
                    None => self.returned_slots(e),
                }
            }
            ast::Expr::RefExpr(r) => self.vec_slot_shape(&r.expr()?),
            // A call, or a field holding one (`self.it.next()`).
            _ => self.returned_slots(e),
        }
    }

    /// A `&mut` iterator over numbers, strings or bools that comes from elsewhere: a call
    /// returning one (`-> impl Iterator<Item = &mut i32>`, `-> slice::IterMut<'_, String>`), a
    /// parameter or a field. It was made as a slot iterator (`v.iter_mut()` that isn't looped over
    /// by index), so the items are slots.
    fn returned_slots(&self, e: &ast::Expr) -> Option<SlotShape> {
        let t = self.raw.get(&self.range(e.syntax()))?;
        match self.zonk(t) {
            Ty::Iter {
                elem,
                mutable: true,
            } if elem.is_js_primitive() => Some(SlotShape::Slot),
            _ => None,
        }
    }

    /// What a `for` loop over `e` binds: a slot iterator's items, or a collected `Vec<&mut T>`'s.
    pub(super) fn for_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        self.iter_slot_shape(e).or_else(|| self.vec_slot_shape(e))
    }

    /// The item shape of a local Vec collected from a slot iterator (`refs`, `&mut refs`).
    pub(super) fn vec_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
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
        // A local that holds one (`let big = it.find(..);`).
        if let ast::Expr::PathExpr(p) = strip_parens(e) {
            let n = p.path()?.as_single_name_ref()?.text().to_string();
            return self.slot_options.get(&n).cloned();
        }
        // One of the file's functions declared `-> Option<&mut i32>`.
        if self.fn_ret_slot(e) == Some(true) {
            return Some(SlotShape::Slot);
        }
        let ast::Expr::MethodCallExpr(m) = strip_parens(e) else {
            return None;
        };
        match m.name_ref()?.text().to_string().as_str() {
            "next" | "next_back" | "nth" | "nth_back" | "last" | "find" | "max" | "min"
            | "max_by_key" | "min_by_key" | "max_by" | "min_by" => {
                self.iter_slot_shape(&m.receiver()?)
            }
            // `v.get_mut(i)` / `map.get_mut(k)` / `self.timer.as_mut()` of numbers, strings or
            // bools where a slot stands in for the result (`slot_calls`).
            "get_mut" | "first_mut" | "last_mut" | "as_mut" | "node_weight_mut"
            | "edge_weight_mut" | "peek_mut"
                if self.slot_calls.contains(&self.range(m.syntax())) =>
            {
                Some(SlotShape::Slot)
            }
            "get_index_mut" if self.slot_calls.contains(&self.range(m.syntax())) => {
                Some(SlotShape::Tuple(vec![None, Some(SlotShape::Slot)]))
            }
            // Adapters that hand the same `Option<&mut T>` on.
            "filter" | "inspect" | "take_if" => self.option_slot_shape(&m.receiver()?),
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
        self.slot_options.remove(&n);
        let Some(init) = init else { return };
        // `let o = self.timer.as_mut();` / `let o = Some(&mut x);` / `let o = s.find("a");`:
        // `o` holds `null` or a slot.
        if self.keep_opt_slot(init) {
            self.slot_options.insert(n, SlotShape::Slot);
            return;
        }
        if let Some(s) = self.option_slot_shape(init) {
            self.slot_options.insert(n, s);
            return;
        }
        // (`let r = &mut x;` of a local, `let r = unsafe { &mut *p };` of a pointer: `r` is
        // the local's cell, handled with the `let`.)
        if let ast::Expr::RefExpr(r) = strip_parens(&super::ptr::unsafe_tail(init))
            && let Some(inner) = r.expr()
            && let inner = match strip_parens(&inner) {
                ast::Expr::PrefixExpr(pe) if pe.op_kind() == Some(UnaryOp::Deref) => {
                    pe.expr().map(|x| strip_parens(&x))
                }
                other => Some(other),
            }
            && matches!(inner, Some(ast::Expr::PathExpr(_)))
        {
            return;
        }
        // `let r = it.next().unwrap();` / `let r = s.at(0);` / `let r = &mut s.count;`: a slot.
        if self.keep_slot(init) {
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
