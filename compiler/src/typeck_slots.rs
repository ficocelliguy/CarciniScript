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

/// Whether `f`, declared `-> &mut T` / `-> Option<&mut T>` with `T` a type parameter, returns
/// a slot whatever `T` is: one of your free functions or inherent methods. (A trait method's
/// result is also reached through `deref_mut` / `index_mut` / `dyn` calls, which don't read a
/// slot, and the bundled preludes, after the program, return what their JS helpers give.)
fn generic_slot_fn(f: &ast::Fn, src: &str) -> bool {
    let in_trait = f.syntax().ancestors().any(|a| {
        ast::Trait::can_cast(a.kind())
            || ast::Impl::cast(a).is_some_and(|i| i.trait_().is_some())
    });
    let at = usize::from(f.syntax().text_range().start());
    !in_trait
        && !src
            .get(..at)
            .unwrap_or_default()
            .lines()
            .any(|l| l.starts_with("// (") && l.ends_with("prelude)"))
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
pub(crate) fn keep_position(n: &SyntaxNode) -> bool {
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

impl SlotShape {
    /// The JS form `κσ.$readSlots` takes: `1` for a slot, an array (`0` where there is none) for
    /// a tuple.
    pub(super) fn js(&self) -> String {
        match self {
            SlotShape::Slot => "1".into(),
            SlotShape::Tuple(fields) => {
                let fs: Vec<String> = fields
                    .iter()
                    .map(|f| f.as_ref().map_or_else(|| "0".into(), SlotShape::js))
                    .collect();
                format!("[{}]", fs.join(", "))
            }
        }
    }
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
    /// return types: a struct or collection is shared by reference anyway. A type parameter
    /// `T` counts too in your own functions and inherent methods (`generic_slot_fn`).
    pub(super) fn ret_slot_kind(&self, f: &ast::Fn, ret: &Ty) -> Option<bool> {
        let w = f.ret_type()?.ty()?;
        // `-> *mut i32` / `-> *const i32`: the slot over the place, as `-> &mut i32` returns.
        if super::is_ptr_type(&w) {
            return self.cellable(ret).then_some(false);
        }
        if !super::written_mut_ref(&w) {
            return None;
        }
        let referent = |t: &Ty| {
            self.cellable(t) || matches!(self.zonk(t), Ty::Param(_)) && generic_slot_fn(f, self.src)
        };
        if matches!(w, ast::Type::RefType(_)) {
            return referent(ret).then_some(false);
        }
        match self.zonk(ret) {
            Ty::Option(inner) if referent(&inner) => Some(true),
            _ => None,
        }
    }

    /// What a call of a function declared with slot return `kind` (`ret`: its return type as
    /// declared, `inst`: the call's) hands back. With `T` a type parameter (`fn first_mut<T>(v:
    /// &mut [T]) -> Option<&mut T>`) the function always returns a slot, which the call keeps
    /// when `T` is a number, string, bool or `Option` there (or still a type parameter); `None`
    /// when it reads the value out (`slot_unwraps`).
    pub(super) fn call_ret_slot(&self, kind: Option<bool>, ret: &Ty, inst: &Ty) -> Option<bool> {
        let kind = kind?;
        let strip = |t: Ty| match t {
            Ty::Option(i) if kind => *i,
            t => t,
        };
        if !matches!(strip(ret.clone()), Ty::Param(_)) {
            return Some(kind);
        }
        self.cell_param(&strip(self.zonk(inst))).then_some(kind)
    }

    /// After a call of a function declared with slot return `kind` (`ret`: as declared, `inst`:
    /// the call's type): a `-> &mut i32` call is a slot value; a generic one whose `T` is a
    /// struct or collection here is read out of its slot.
    pub(super) fn note_call_slot(&mut self, e: &ast::Expr, kind: Option<bool>, ret: &Ty, inst: &Ty) {
        let r = self.range(e.syntax());
        match (kind, self.call_ret_slot(kind, ret, inst)) {
            (_, Some(false)) => {
                self.slot_values.insert(r);
            }
            (Some(k), None) => {
                self.slot_unwraps.insert(r, k);
            }
            _ => {}
        }
    }

    /// A number, string, bool or `Option`, or (while checking what a generic `-> &mut T`
    /// returns, which is always a slot) a type parameter.
    fn slot_elem(&self, t: &Ty) -> bool {
        self.cellable(t) || (self.slot_param_ok && matches!(self.zonk(t), Ty::Param(_)))
    }

    /// `ret` returned from a function declared `-> &mut T` / `-> Option<&mut T>` (`opt`) whose
    /// referent is a number, string, bool, `Option` or type parameter: kept as a slot.
    pub(super) fn keep_ret_slot(&mut self, ret: &ast::Expr, opt: bool) {
        if !self.ret_slot_param {
            if opt {
                self.keep_opt_slot(ret);
            } else {
                self.keep_slot(ret);
            }
            return;
        }
        self.slot_param_ok = true;
        self.keep_generic_ret(ret, opt);
        self.slot_param_ok = false;
    }

    /// What a generic `-> &mut T` returns is a slot on every path: each branch's slot, or one
    /// holding the value where it isn't a slot (`m.get_mut(k)` of a map; `slot_wraps`), which
    /// writes reach when `T` is a struct or collection, as before.
    fn keep_generic_ret(&mut self, e: &ast::Expr, opt: bool) {
        let e = strip_parens(e);
        match &e {
            ast::Expr::IfExpr(i) => {
                if let Some(b) = i.then_branch() {
                    self.keep_generic_ret(&ast::Expr::BlockExpr(b), opt);
                }
                match i.else_branch() {
                    Some(ast::ElseBranch::Block(b)) => {
                        self.keep_generic_ret(&ast::Expr::BlockExpr(b), opt)
                    }
                    Some(ast::ElseBranch::IfExpr(e)) => {
                        self.keep_generic_ret(&ast::Expr::IfExpr(e), opt)
                    }
                    None => {}
                }
            }
            ast::Expr::MatchExpr(m) => {
                let arms: Vec<ast::Expr> = m
                    .match_arm_list()
                    .into_iter()
                    .flat_map(|l| l.arms())
                    .filter_map(|a| a.expr())
                    .collect();
                for a in arms {
                    self.keep_generic_ret(&a, opt);
                }
            }
            ast::Expr::BlockExpr(b) => {
                if let Some(t) = b.stmt_list().and_then(|l| l.tail_expr()) {
                    self.keep_generic_ret(&t, opt);
                }
            }
            ast::Expr::PathExpr(p) if opt && p.syntax().text() == "None" => {}
            _ => {
                let r = self.range(e.syntax());
                if self.raw.get(&r).is_some_and(|t| self.zonk(t) == Ty::Never) {
                    return;
                }
                let kept = if opt {
                    self.keep_opt_slot(&e)
                } else {
                    self.keep_slot(&e)
                };
                if !kept {
                    self.slot_wraps.insert(r, opt);
                }
            }
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
                if r.mut_token().is_none() && !self.shared_slot_ok {
                    return false;
                }
                let Some(inner) = r.expr() else { return false };
                let inner = strip_parens(&inner);
                // `&mut refs[i]` of a `Vec<&mut i32>`: the item's slot, passed on.
                let ir = self.range(inner.syntax());
                if matches!(inner, ast::Expr::IndexExpr(_)) && self.slot_values.contains(&ir) {
                    self.slot_keeps.insert(ir);
                    return true;
                }
                let Some(t) = self.raw.get(&self.range(inner.syntax())).cloned() else {
                    return false;
                };
                if !self.slot_elem(&t) {
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
            // field, an item of a `Vec<&mut i32>`: kept as is (not read through `[1]`).
            ast::Expr::CallExpr(_)
            | ast::Expr::MethodCallExpr(_)
            | ast::Expr::FieldExpr(_)
            | ast::Expr::IndexExpr(_)
            | ast::Expr::TryExpr(_) => {
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
            .is_some_and(|l| {
                l.reference == Some(true)
                    && (self.cellable(&l.ty)
                        // A `&mut T` parameter: a cell.
                        || self.slot_elem(&l.ty) && l.decl.is_some_and(|d| self.boxed.contains(&d)))
            })
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
                if matches!(name.as_str(), "as_mut" | "as_deref_mut") && self.as_mut_of_place(&e) {
                    return self.mark_as_mut_slot_kept(m);
                }
                // `v.first_mut()` of a `Vec<T>` returned from a generic `-> Option<&mut T>`.
                if self.slot_param_ok
                    && matches!(name.as_str(), "get_mut" | "first_mut" | "last_mut")
                    && let Some(r) = m.receiver()
                    && let Some(t) = self.raw.get(&self.range(r.syntax()))
                    && matches!(self.zonk(t), Ty::Vec(el) if matches!(*el, Ty::Param(_)))
                {
                    let r = self.range(m.syntax());
                    self.slot_calls.insert(r);
                    return true;
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
        if !self.prim(&self.zonk(&inner)) || !is_place_expr(&recv) {
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
                if let Some(sig) = self.assoc_call_sig(&c) {
                    let inst = self.raw.get(&self.range(c.syntax())).unwrap_or(&Ty::Unknown);
                    return self.call_ret_slot(sig.ret_slot, &sig.ret, inst);
                }
                let ast::Expr::PathExpr(p) = c.expr()? else {
                    return None;
                };
                let name = p.path()?.as_single_name_ref()?.text().to_string();
                let sig = self.fns.get(name.as_str())?;
                let inst = self.raw.get(&self.range(c.syntax())).unwrap_or(&Ty::Unknown);
                self.call_ret_slot(sig.ret_slot, &sig.ret, inst)
            }
            ast::Expr::MethodCallExpr(m) => {
                let recv = m.receiver()?;
                let rt = self.raw.get(&self.range(recv.syntax()))?;
                let Ty::Adt(adt, _) = self.zonk(rt) else {
                    return None;
                };
                let name = m.name_ref()?.text().to_string();
                let sig = self.adts.method(&adt, &name)?;
                let inst = self.raw.get(&self.range(m.syntax())).unwrap_or(&Ty::Unknown);
                self.call_ret_slot(sig.ret_slot, &sig.ret, inst)
            }
            _ => None,
        }
    }

    /// The signature of the file's associated function or method that a path call names
    /// (`Bx::get(&mut b)`, `Self::at(this, 0)`).
    pub(super) fn assoc_call_sig(&self, c: &ast::CallExpr) -> Option<&crate::adt::Method> {
        let ast::Expr::PathExpr(p) = c.expr()? else {
            return None;
        };
        let segs = super::adt_check::path_segs(p.path());
        if segs.len() < 2 {
            return None;
        }
        match self.resolve(&segs)? {
            crate::adt::Resolved::Assoc(adt, m) => self.adts.method(&adt, &m),
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
        if self.prim(&z) && !iter_mut_indexed(m) {
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
        if self.prim(&self.zonk(v)) && !for_iterable {
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
            } if self.prim(&elem) => Some(SlotShape::Slot),
            // `-> impl Iterator<Item = (usize, &mut u8)>`: the call's items hold slots where
            // the declared type says.
            Ty::Iter {
                elem,
                mutable: true,
            } if matches!(*elem, Ty::Tuple(_)) => {
                let key = match strip_parens(e) {
                    ast::Expr::CallExpr(c) => match c.expr()? {
                        ast::Expr::PathExpr(p) => p.path()?.as_single_name_ref()?.text().to_string(),
                        _ => return None,
                    },
                    ast::Expr::MethodCallExpr(m) => {
                        let rt = self.raw.get(&self.range(m.receiver()?.syntax()))?;
                        let Ty::Adt(adt, _) = self.zonk(rt) else {
                            return None;
                        };
                        format!("{adt}::{}", m.name_ref()?.text())
                    }
                    _ => return None,
                };
                self.ret_item_slots.get(&key).cloned()
            }
            // Your own iterator with `type Item = &'a mut T` (`for w in self.words_mut()`): its
            // `next()` hands on the slots of the `iter_mut()` it wraps.
            Ty::Adt(n, targs) if self.adts.iter_mut_items.contains(&n) => {
                let map = self.adt_subst(&n, &targs);
                let item = super::generic::subst(&self.adts.iter_items[&n], &map);
                self.prim(&item).then_some(SlotShape::Slot)
            }
            _ => None,
        }
    }

    /// `f` declared `-> impl Iterator<Item = (usize, &mut T)>` with `T` a number, string, bool
    /// or `Option`: its items' slot shape, under `key` (`returned_slots`).
    pub(super) fn note_ret_item_slots(&mut self, f: &ast::Fn, key: String) {
        use ra_ap_syntax::ast::HasGenericArgs;
        let Some(mut ret) = f.ret_type().and_then(|r| r.ty()) else {
            return;
        };
        // `Box<dyn Iterator<..>>`
        if let ast::Type::PathType(p) = &ret
            && let Some(seg) = p.path().and_then(|p| p.segment())
            && seg.name_ref().is_some_and(|n| n.text() == "Box")
            && let Some(ast::GenericArg::TypeArg(a)) =
                seg.generic_arg_list().and_then(|l| l.generic_args().next())
            && let Some(t) = a.ty()
        {
            ret = t;
        }
        let item = super::iterator_item(&ret);
        if let Some(Some(t @ ast::Type::TupleType(_))) = item
            && let Some(shape) = self.written_slot_elem(&t)
        {
            self.ret_item_slots.insert(key, shape);
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

    /// `tagged[i]` / `&tagged[i]` / `&mut tagged[i]` of a `Vec<(usize, &mut i32)>` of slots: the
    /// index expression and the item's shape.
    pub(super) fn slot_tuple_item(&self, e: &ast::Expr) -> Option<(ast::IndexExpr, SlotShape)> {
        match strip_parens(e) {
            ast::Expr::RefExpr(r) => self.slot_tuple_item(&r.expr()?),
            ast::Expr::IndexExpr(ix) => match self.vec_slot_shape(&ix.base()?)? {
                s @ SlotShape::Tuple(_) => Some((ix, s)),
                SlotShape::Slot => None,
            },
            _ => None,
        }
    }

    /// `{:?}` of a whole value holding slots: a Vec of them (`refs`, `&tagged`), an item of one
    /// (`tagged[1]`) or a local holding an item (`t` of `for t in &tagged`). Marks it to be
    /// copied out (`slot_reads`).
    pub(super) fn note_slot_read(&mut self, e: &ast::Expr) {
        let js = if let Some((ix, s)) = self.slot_tuple_item(e) {
            let r = self.range(ix.syntax());
            self.whole_slot_items.insert(r);
            s.js()
        } else if let Some(s) = self.vec_slot_shape(e) {
            format!("{{e: {}}}", s.js())
        } else if let ast::Expr::PathExpr(p) = strip_parens(e)
            && let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
            && let Some(js) = self.local_slot_read(n.text().to_string().as_str())
        {
            js
        } else {
            return;
        };
        let r = self.range(e.syntax());
        self.slot_reads.insert(r, js);
    }

    /// The `κσ.$readSlots` shape of a local holding a Vec of slots, an item of one or an `Option`
    /// of a slot.
    pub(super) fn local_slot_read(&self, name: &str) -> Option<String> {
        match self.slot_locals.get(name) {
            Some((true, s)) => Some(format!("{{e: {}}}", s.js())),
            // A kept `Option` of a slot (`let o = m.get_mut(k);`): `None`, or the value read.
            _ if let Some(s) = self.slot_options.get(name) => Some(format!("{{o: {}}}", s.js())),
            _ => self.slot_items.get(name).map(SlotShape::js),
        }
    }

    /// A use of a local holding `m.get_mut(k).ok_or(e)`: `r?`, `r.unwrap()` / `expect`,
    /// `r.is_ok()` / `is_err()`, or matched (`match r` / `if let Ok(v) = r`); anything else would
    /// see the slot, so it's an error.
    pub(super) fn check_slot_result_use(&mut self, p: &ast::PathExpr) {
        let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return;
        };
        if !self
            .slot_results
            .contains_key(name.text().to_string().as_str())
        {
            return;
        }
        let mut n = p.syntax().clone();
        while let Some(up) = n.parent().filter(|x| x.kind() == SyntaxKind::PAREN_EXPR) {
            n = up;
        }
        // (A macro argument, `println!("{:?}", r)`, has no parent: not one of those.)
        let ok = n.parent().is_some_and(|parent| match parent.kind() {
            SyntaxKind::TRY_EXPR | SyntaxKind::MATCH_EXPR | SyntaxKind::LET_EXPR => true,
            SyntaxKind::LET_STMT => {
                ast::LetStmt::cast(parent.clone()).is_some_and(|l| l.let_else().is_some())
            }
            SyntaxKind::METHOD_CALL_EXPR => ast::MethodCallExpr::cast(parent).is_some_and(|m| {
                m.receiver().is_some_and(|r| r.syntax() == &n)
                    && m.name_ref().is_some_and(|x| {
                        matches!(x.text().to_string().as_str(), "unwrap" | "expect" | "is_ok" | "is_err")
                    })
            }),
            _ => false,
        });
        if !ok {
            let msg = format!(
                "a `Result` of `get_mut` over numbers, strings or bools kept in a local is only matched (`if let Ok(v) = {n}`), `?`-ed or unwrapped here",
                n = name.text()
            );
            self.error(p.syntax(), msg);
        }
    }

    /// A local holding `Ok(slot)` or an error (`let r = m.get_mut(k).ok_or(e);`): the slot's shape.
    pub(super) fn result_slot_local(&self, e: &ast::Expr) -> Option<SlotShape> {
        let ast::Expr::PathExpr(p) = strip_parens(e) else {
            return None;
        };
        let n = p.path()?.as_single_name_ref()?.text().to_string();
        self.slot_results.get(&n).cloned()
    }

    /// The shape of what an `Option` of an item holds: `it.next()`, `.nth(n)`, `.last()`, ..
    pub(super) fn option_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        // (`unsafe { p.as_mut() }` of a raw pointer: the call.)
        let e = &super::ptr::unsafe_tail(e);
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
            "get_mut" | "first_mut" | "last_mut" | "as_mut" | "as_deref_mut" | "node_weight_mut"
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
        match strip_parens(e) {
            // `let s = o?;` of an `Option<&mut String>` parameter: the slot too.
            ast::Expr::TryExpr(t) => self.option_slot_shape(&t.expr()?),
            ast::Expr::MethodCallExpr(m) => match m.name_ref()?.text().to_string().as_str() {
                "unwrap" | "expect" | "unwrap_or" => self.option_slot_shape(&m.receiver()?),
                _ => None,
            },
            _ => None,
        }
    }

    /// `let name = init;`: a slot binding (`it.next().unwrap()`), or a local that holds a slot
    /// iterator or a Vec of slots, recorded for later bindings. Before the local is declared.
    pub(super) fn note_slot_let(&mut self, name: &ast::Name, init: Option<&ast::Expr>) {
        let n = name.text().to_string();
        self.slot_locals.remove(&n);
        self.slot_options.remove(&n);
        self.slot_results.remove(&n);
        self.slot_items.remove(&n);
        let Some(init) = init else { return };
        // `let r = m.get_mut(k).ok_or(e);`: `r` holds `Ok(slot)` or the error.
        if let ast::Expr::MethodCallExpr(m) = strip_parens(init)
            && m.name_ref()
                .is_some_and(|x| matches!(x.text(), "ok_or" | "ok_or_else"))
            && let Some(ast::Expr::MethodCallExpr(x)) = m.receiver().map(|r| strip_parens(&r))
            && self.slot_calls.contains(&self.range(x.syntax()))
        {
            self.slot_results.insert(n, SlotShape::Slot);
            return;
        }
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

    /// `let (evens, odds): (Vec<&mut i32>, Vec<&mut i32>) = v.iter_mut().partition(..);`: both
    /// locals hold Vecs of slots. After the pattern's names are declared.
    pub(super) fn note_slot_tuple_let(&mut self, pat: &ast::Pat, init: Option<&ast::Expr>) {
        let (ast::Pat::TuplePat(tp), Some(init)) = (super::strip_pat(pat), init) else {
            return;
        };
        let ast::Expr::MethodCallExpr(m) = strip_parens(init) else {
            return;
        };
        if !m.name_ref().is_some_and(|n| n.text() == "partition") {
            return;
        }
        let Some(shape) = m.receiver().and_then(|r| self.iter_slot_shape(&r)) else {
            return;
        };
        for p in tp.fields() {
            if let ast::Pat::IdentPat(ip) = p
                && let Some(n) = ip.name()
            {
                self.slot_locals
                    .insert(n.text().to_string(), (true, shape.clone()));
            }
        }
    }

    /// Whether `recv` is a local holding a Vec of slots (`let mut refs: Vec<&mut i32>`).
    pub(super) fn slot_vec_local(&self, recv: &ast::Expr) -> bool {
        let ast::Expr::PathExpr(p) = strip_parens(recv) else {
            return false;
        };
        let Some(n) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return false;
        };
        self.slot_locals
            .get(n.text().to_string().as_str())
            .is_some_and(|(vec, s)| *vec && matches!(s, SlotShape::Slot))
    }

    /// The slot shape a written `Vec<&mut T>` / `Vec<(usize, &mut T)>` gives its elements, for a
    /// `T` kept in cells (a number, string, bool or `Option`); None when there is none.
    pub(super) fn written_slot_shape(&mut self, t: &ast::Type) -> Option<SlotShape> {
        use ra_ap_syntax::ast::HasGenericArgs;
        let ast::Type::PathType(p) = t else {
            return None;
        };
        let seg = p.path()?.segment()?;
        if seg.name_ref()?.text() != "Vec" {
            return None;
        }
        let ast::GenericArg::TypeArg(a) = seg.generic_arg_list()?.generic_args().next()? else {
            return None;
        };
        self.written_slot_elem(&a.ty()?)
    }

    /// `let t = (1, &mut x);` (or `let t: (usize, &mut i32) = ..`) of a number, string, bool or
    /// `Option` `x`: the tuple holds a slot over `x` there, so writes through `t.1` reach it.
    /// Marks the `&mut`s to be kept as slots and returns the tuple's shape. After the
    /// initializer is checked.
    pub(super) fn tuple_let_slot_shape(
        &mut self,
        written: Option<&ast::Type>,
        init: Option<&ast::Expr>,
    ) -> Option<SlotShape> {
        let shape = match written {
            Some(t @ ast::Type::TupleType(_)) => self.written_slot_elem(t),
            Some(_) => None,
            None => init.and_then(|i| self.literal_slot_shape(i)),
        }?;
        if let Some(i) = init {
            self.keep_slot_shape(i, &shape);
        }
        Some(shape)
    }

    /// `show(&t)` / `Some(&t)` of a local tuple holding slots (`let t = (1, &mut x);`): nothing
    /// can be written through a `&(usize, &mut i32)`, so it's a copy with the slots read.
    /// (`let u = &t;` takes the shape on instead.)
    pub(super) fn note_slot_tuple_ref(&mut self, p: &ast::PathExpr) {
        let Some(n) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return;
        };
        let Some(s @ SlotShape::Tuple(_)) = self.slot_items.get(n.text().to_string().as_str())
        else {
            return;
        };
        let js = s.js();
        let Some(re) = p
            .syntax()
            .ancestors()
            .skip(1)
            .find(|x| x.kind() != SyntaxKind::PAREN_EXPR)
            .and_then(ast::RefExpr::cast)
        else {
            return;
        };
        if re.mut_token().is_some()
            || re
                .syntax()
                .parent()
                .is_some_and(|x| x.kind() == SyntaxKind::LET_STMT)
        {
            return;
        }
        let r = self.range(re.syntax());
        self.slot_reads.insert(r, js);
    }

    /// The slot shape of a tuple whose fields are read: `tagged[1]` of a `Vec<(usize, &mut
    /// i32)>`, a local holding one (`t` of `let t = (1, &mut x);`), or a tuple field of either
    /// (`(w.1).1`).
    pub(super) fn tuple_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        match strip_parens(e) {
            ast::Expr::RefExpr(r) => self.tuple_slot_shape(&r.expr()?),
            ast::Expr::IndexExpr(ix) => self.vec_slot_shape(&ix.base()?),
            ast::Expr::PathExpr(p) => {
                let n = p.path()?.as_single_name_ref()?;
                self.slot_items.get(n.text().to_string().as_str()).cloned()
            }
            ast::Expr::FieldExpr(f) => {
                let i = f.name_ref()?.as_tuple_field()?;
                match self.tuple_slot_shape(&f.expr()?)? {
                    SlotShape::Tuple(fields) => match fields.into_iter().nth(i)?? {
                        s @ SlotShape::Tuple(_) => Some(s),
                        SlotShape::Slot => None,
                    },
                    SlotShape::Slot => None,
                }
            }
            _ => None,
        }
    }

    /// The slot shape of a tuple literal's `&mut` fields over numbers, strings, bools and
    /// `Option`s (`(1, &mut x)`).
    fn literal_slot_shape(&self, e: &ast::Expr) -> Option<SlotShape> {
        let ast::Expr::TupleExpr(t) = strip_parens(e) else {
            return None;
        };
        let fields: Vec<Option<SlotShape>> = t
            .fields()
            .map(|f| match strip_parens(&f) {
                ast::Expr::RefExpr(r) if r.mut_token().is_some() => {
                    let inner = r.expr()?;
                    let ty = self.raw.get(&self.range(inner.syntax()))?;
                    self.cellable(ty).then_some(SlotShape::Slot)
                }
                f @ ast::Expr::TupleExpr(_) => self.literal_slot_shape(&f),
                _ => None,
            })
            .collect();
        fields.iter().any(Option::is_some).then_some(SlotShape::Tuple(fields))
    }

    /// A parameter `t: (usize, &mut i32)` / `t: &mut (usize, &mut i32)`: the slot shape of the
    /// tuple it receives.
    pub(super) fn param_tuple_slot_shape(&mut self, t: &ast::Type) -> Option<SlotShape> {
        match t {
            // (A `&(usize, &mut i32)` gets a copy with the slots read.)
            ast::Type::RefType(r) if r.mut_token().is_some() => match r.ty()? {
                tt @ ast::Type::TupleType(_) => self.written_slot_elem(&tt),
                _ => None,
            },
            ast::Type::TupleType(_) => self.written_slot_elem(t),
            _ => None,
        }
    }

    /// `&mut T` → a slot; `(A, &mut T)` → a tuple with slots in those positions.
    fn written_slot_elem(&mut self, t: &ast::Type) -> Option<SlotShape> {
        match t {
            ast::Type::RefType(r) if r.mut_token().is_some() => {
                let ty = self.lower_type(&r.ty()?);
                self.cellable(&ty).then_some(SlotShape::Slot)
            }
            ast::Type::TupleType(tt) => {
                let fields: Vec<Option<SlotShape>> =
                    tt.fields().map(|f| self.written_slot_elem(&f)).collect();
                fields.iter().any(Option::is_some).then_some(SlotShape::Tuple(fields))
            }
            _ => None,
        }
    }

    /// `let tagged: Vec<(usize, &mut i32)> = vec![(0, &mut c)];` (or an array literal): the
    /// literal's `&mut` places are kept as slots where the written type says so.
    pub(super) fn note_slot_literal(&mut self, init: &ast::Expr, shape: &SlotShape) {
        match strip_parens(init) {
            ast::Expr::ArrayExpr(a) => {
                for e in a.exprs() {
                    self.keep_slot_shape(&e, shape);
                }
            }
            ast::Expr::MacroExpr(me) => {
                let Some(mc) = me.macro_call() else { return };
                if crate::macros::macro_name(&mc).as_deref() != Some("vec") {
                    return;
                }
                for a in crate::macros::split_args(self.src, self.base, &mc) {
                    let (expr, _) = crate::macros::parse_arg(&a);
                    if let Some(expr) = expr {
                        let saved = std::mem::replace(&mut self.base, a.start);
                        self.keep_slot_shape(&expr, shape);
                        self.base = saved;
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn keep_slot_shape(&mut self, e: &ast::Expr, shape: &SlotShape) {
        match (strip_parens(e), shape) {
            (_, SlotShape::Slot) => {
                self.keep_slot(e);
            }
            (ast::Expr::TupleExpr(t), SlotShape::Tuple(fields)) => {
                for (f, s) in t.fields().zip(fields) {
                    if let Some(s) = s {
                        self.keep_slot_shape(&f, s);
                    }
                }
            }
            _ => {}
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
            // `for t in &tagged`: `t.1` is a slot.
            (ast::Pat::IdentPat(ip), SlotShape::Tuple(_)) if ip.pat().is_none() => {
                if let Some(n) = ip.name() {
                    self.slot_items.insert(n.text().to_string(), shape.clone());
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
        // (Also `Ok(p)` against a local holding `m.get_mut(k).ok_or(e)`.)
        let (shape, variant) = match scrutinee.and_then(|s| self.option_slot_shape(s)) {
            Some(s) => (s, "Some"),
            // `Some(&mut x)` itself, of a number, string or `Option` (`let Some(p) = Some(&mut x)
            // else { .. }`).
            None if let Some(s) = scrutinee
                && let ast::Expr::CallExpr(c) = strip_parens(s)
                && c.expr().is_some_and(|f| f.syntax().text() == "Some")
                && self.keep_opt_slot(s) =>
            {
                (SlotShape::Slot, "Some")
            }
            None => match scrutinee.and_then(|s| self.result_slot_local(s)) {
                Some(s) => (s, "Ok"),
                None => return,
            },
        };
        if let ast::Pat::TupleStructPat(tp) = pat
            && tp
                .path()
                .and_then(|p| p.segment())
                .is_some_and(|s| s.syntax().text() == variant)
            && let Some(inner) = tp.fields().next()
        {
            self.bind_slot_shape(&inner, &shape);
        }
    }
}
