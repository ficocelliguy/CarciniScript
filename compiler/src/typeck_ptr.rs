//! Raw pointers: `*const T` / `*mut T` are references that can be `null`. A pointer to a struct,
//! enum, Vec, .. is the object itself; a pointer to a number, string, bool or Option is a cell
//! (`[value]`), like a `&mut` to one, so writes through it reach the place. `ptr::null()` is
//! `null`, `ptr::eq` compares identity, and `transmute` reinterprets same-size numbers. Pointer
//! arithmetic and casts between pointers and integers are rejected: values have no memory layout.
//! A child module of `typeck`.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericArgs},
};

use super::{Checker, VarKind, simple_place, strip_parens};
use crate::ty::{IntKind, Ty};

/// What the type checker found out about raw pointers, for codegen.
#[derive(Default)]
pub struct Ptrs {
    /// Locals and parameters holding a raw pointer, by declaration range.
    pub locals: HashSet<(usize, usize)>,
    /// Names of struct fields declared with a raw pointer type.
    pub fields: HashSet<String>,
    /// `let` bindings of pointers to cells (numbers, strings, bools, Options): the variable holds
    /// the cell (or `null`), by name range.
    pub cells: HashSet<(usize, usize)>,
    /// Pointer method calls (`p.is_null()`, `p.as_ref()`, ..), by the call's range: whether the
    /// pointee lives in a cell.
    pub methods: HashMap<(usize, usize), bool>,
    /// Assignments `p = q` to a pointer that holds a cell: they re-point it, by the `=`'s range.
    pub repoints: HashSet<(usize, usize)>,
    /// Struct fields holding a pointer to a number, string or bool: a slot over the place, or
    /// null (also in `mut_ref_fields`).
    pub cell_fields: HashSet<String>,
    /// Functions and methods declared `-> *mut T` / `-> *const T`, by name.
    pub ret_fns: HashSet<String>,
}

/// Whether a type is a raw pointer (`*const T` / `*mut T`).
pub fn is_ptr_type(t: &ast::Type) -> bool {
    match t {
        ast::Type::PtrType(_) => true,
        ast::Type::ParenType(p) => p.ty().is_some_and(|t| is_ptr_type(&t)),
        _ => false,
    }
}

/// The value an `unsafe { .. }` block (or parentheses) around `e` evaluates to.
pub fn unsafe_tail(e: &ast::Expr) -> ast::Expr {
    match strip_parens(e) {
        ast::Expr::BlockExpr(b)
            if matches!(b.modifier(), Some(ast::BlockModifier::Unsafe(_)))
                && b.stmt_list()
                    .is_some_and(|s| s.statements().next().is_none()) =>
        {
            match b.stmt_list().and_then(|s| s.tail_expr()) {
                Some(t) => unsafe_tail(&t),
                None => strip_parens(e),
            }
        }
        other => other,
    }
}

/// Values JS can't point at (they need a cell), once the type is known.
pub fn cellable(t: &Ty) -> bool {
    matches!(
        t,
        Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str | Ty::Option(_)
    )
}

/// `ptr::null()`, `std::ptr::null_mut::<T>()`, `Box::into_raw(b)`: the std function a call names.
fn ptr_fn(c: &ast::CallExpr) -> Option<String> {
    let ast::Expr::PathExpr(p) = c.expr()? else {
        return None;
    };
    let segs: Vec<String> = p
        .path()?
        .segments()
        .filter_map(|s| s.name_ref())
        .map(|n| n.text().to_string())
        .collect();
    let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
    let tail = match segs.as_slice() {
        ["std" | "core", rest @ ..] => rest,
        all => all,
    };
    match tail {
        ["ptr", f @ ("null" | "null_mut")] | [f @ ("null" | "null_mut")] => Some(f.to_string()),
        ["Box", "into_raw"] => Some("into_raw".into()),
        _ => None,
    }
}

/// The size in bytes of a type `transmute` can reinterpret, and its runtime name.
fn layout(t: &Ty) -> Option<(u32, &'static str)> {
    Some(match t {
        Ty::Int(k) => match k {
            IntKind::I8 => (1, "i8"),
            IntKind::U8 => (1, "u8"),
            IntKind::I16 => (2, "i16"),
            IntKind::U16 => (2, "u16"),
            IntKind::I32 => (4, "i32"),
            IntKind::U32 => (4, "u32"),
            IntKind::I64 | IntKind::Isize => (8, "i64"),
            IntKind::U64 | IntKind::Usize => (8, "u64"),
            IntKind::I128 => (16, "i128"),
            IntKind::U128 => (16, "u128"),
        },
        Ty::Float { f32: true } => (4, "f32"),
        Ty::Float { f32: false } => (8, "f64"),
        Ty::Char => (4, "char"),
        Ty::Bool => (1, "bool"),
        _ => return None,
    })
}

/// The runtime name of a type `transmute` handles (`"f32"`, `"u64"`, ..).
pub fn transmute_name(t: &Ty) -> Option<&'static str> {
    layout(t).map(|l| l.1)
}

impl Checker<'_> {
    /// Whether `e` is a raw pointer: a pointer local or field, a cast to a pointer type,
    /// `ptr::null()`, `Box::into_raw(..)`.
    pub(super) fn is_ptr(&self, e: &ast::Expr) -> bool {
        match strip_parens(e) {
            ast::Expr::PathExpr(p) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()))
                .and_then(|l| l.decl)
                .is_some_and(|d| self.ptrs.locals.contains(&d)),
            ast::Expr::CastExpr(c) => c.ty().is_some_and(|t| is_ptr_type(&t)),
            ast::Expr::FieldExpr(f) => f
                .name_ref()
                .is_some_and(|n| self.ptrs.fields.contains(n.text().to_string().as_str())),
            ast::Expr::CallExpr(c) => ptr_fn(&c).is_some() || self.ptr_ret_call(&c),
            ast::Expr::MethodCallExpr(m) => m
                .name_ref()
                .is_some_and(|n| self.ptrs.ret_fns.contains(n.text().to_string().as_str())),
            _ => false,
        }
    }

    /// The value of a raw pointer field holding a number, string or bool (`Counter { p: &mut x
    /// }`, `p: &s`, `p: q`, `p: ptr::null()`, `p: Box::into_raw(..)`): kept as a slot over the
    /// place, as for a `&mut i32` field (`&s` too, and a pointer local's cell).
    pub(super) fn keep_ptr_slot(&mut self, e: &ast::Expr) {
        let e = unsafe_tail(e);
        let e = match &e {
            ast::Expr::CastExpr(c) => match c.expr() {
                Some(x) => unsafe_tail(&x),
                None => return,
            },
            _ => e,
        };
        if let ast::Expr::CallExpr(c) = &e
            && ptr_fn(c).is_some()
        {
            return;
        }
        self.shared_slot_ok = true;
        self.keep_slot(&e);
        self.shared_slot_ok = false;
    }

    /// A call of one of your functions declared `-> *mut T` / `-> *const T` (by its last path
    /// segment: `f()`, `S::f()`).
    fn ptr_ret_call(&self, c: &ast::CallExpr) -> bool {
        let Some(ast::Expr::PathExpr(p)) = c.expr() else {
            return false;
        };
        p.path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .is_some_and(|n| self.ptrs.ret_fns.contains(n.text().to_string().as_str()))
    }

    /// `e`, a pointer to a number, string, bool or Option, is passed around as its cell: mark
    /// what it points at to live in one.
    pub(super) fn ptr_cell(&mut self, e: &ast::Expr) {
        match strip_parens(e) {
            ast::Expr::RefExpr(r) => {
                let Some(inner) = r.expr() else { return };
                match strip_parens(&inner) {
                    ast::Expr::PathExpr(p) => {
                        let local = p
                            .path()
                            .and_then(|p| p.as_single_name_ref())
                            .and_then(|n| self.lookup(n.text().to_string().as_str()).cloned());
                        match local {
                            // `&*p` / `&r` with `r` a reference: the cell it holds.
                            Some(l) if l.reference.is_some() => {
                                let rr = self.range(p.syntax());
                                self.cell_args.insert(rr);
                            }
                            Some(_) => self.box_if_needed(&inner),
                            None => self.error(
                                inner.syntax(),
                                "a raw pointer to a `static` or `const` isn't supported yet",
                            ),
                        }
                    }
                    // `&mut *q`: q's cell.
                    ast::Expr::PrefixExpr(pe) if pe.op_kind() == Some(ast::UnaryOp::Deref) => {
                        if let Some(inner) = pe.expr() {
                            self.ptr_cell(&inner);
                        }
                    }
                    // `&mut v[i] as *mut i32`: a getter/setter cell over the slot.
                    other @ (ast::Expr::FieldExpr(_) | ast::Expr::IndexExpr(_))
                        if simple_place(&other) =>
                    {
                        let rr = self.range(r.syntax());
                        self.slot_args.insert(rr);
                    }
                    other => self.error(
                        other.syntax(),
                        "a raw pointer to a number, string or bool here isn't supported yet (point at a local variable)",
                    ),
                }
            }
            ast::Expr::PathExpr(p) => {
                if let Some(l) = p
                    .path()
                    .and_then(|p| p.as_single_name_ref())
                    .and_then(|n| self.lookup(n.text().to_string().as_str()).cloned())
                    && l.reference.is_some()
                {
                    let rr = self.range(p.syntax());
                    self.cell_args.insert(rr);
                }
            }
            ast::Expr::CastExpr(c) => {
                if let Some(inner) = c.expr() {
                    self.ptr_cell(&inner);
                }
            }
            ast::Expr::CallExpr(c) if ptr_fn(&c).is_some() => {}
            // A call of your `-> *mut i32` (it returns the slot), a pointer field (it holds one).
            ast::Expr::CallExpr(_) | ast::Expr::MethodCallExpr(_) | ast::Expr::FieldExpr(_)
                if self.is_ptr(e) =>
            {
                self.keep_slot(e);
            }
            // `unsafe { &mut *q }`
            ast::Expr::BlockExpr(b) => {
                if let Some(tail) = b.stmt_list().and_then(|s| s.tail_expr()) {
                    self.ptr_cell(&tail);
                }
            }
            other => self.error(
                other.syntax(),
                "a raw pointer to a number, string or bool from here isn't supported yet (point at a local variable)",
            ),
        }
    }

    /// A `let` binding a raw pointer: remember it, and (for a cell's pointer) how its value is
    /// passed.
    pub(super) fn ptr_let(&mut self, l: &ast::LetStmt, name: &ast::Name, ty: &Ty) -> bool {
        let declared = l.ty().is_some_and(|t| is_ptr_type(&t));
        let init = l.initializer();
        // `let r = unsafe { &mut *p };`: `r` holds p's cell too.
        let reborrow = init.as_ref().is_some_and(|i| match unsafe_tail(i) {
            ast::Expr::RefExpr(r) => match r.expr().map(|e| strip_parens(&e)) {
                Some(ast::Expr::PrefixExpr(pe)) => {
                    pe.op_kind() == Some(ast::UnaryOp::Deref)
                        && pe.expr().is_some_and(|x| self.is_ptr(&x))
                }
                _ => false,
            },
            _ => false,
        });
        if !declared && !reborrow && !init.as_ref().is_some_and(|i| self.is_ptr(i)) {
            return false;
        }
        let d = self.range(name.syntax());
        self.ptrs.locals.insert(d);
        if self.cellable(ty) {
            self.ptrs.cells.insert(d);
            // (`let q = h.count_ptr();`: the slot it returns is the cell, read through `[0]`.)
            self.slot_bindings.remove(&d);
            if let Some(init) = &init {
                self.ptr_cell(init);
            }
        }
        true
    }

    /// `c.p = &mut y` with `p: *mut i32`: the field takes the new slot (after both sides are
    /// checked).
    pub(super) fn ptr_assign_field(&mut self, b: &ast::BinExpr) {
        if let (Some(ast::Expr::FieldExpr(f)), Some(rhs)) = (b.lhs(), b.rhs())
            && f
                .name_ref()
                .is_some_and(|n| self.ptrs.cell_fields.contains(n.text().to_string().as_str()))
        {
            let r = self.range(f.syntax());
            self.slot_keeps.insert(r);
            self.keep_ptr_slot(&rhs);
        }
    }

    /// `p = q` with `p` a pointer local holding a cell: re-point it.
    pub(super) fn ptr_assign(&mut self, b: &ast::BinExpr) {
        let (Some(ast::Expr::PathExpr(p)), Some(rhs)) = (b.lhs(), b.rhs()) else {
            return;
        };
        let Some(local) = p
            .path()
            .and_then(|p| p.as_single_name_ref())
            .and_then(|n| self.lookup(n.text().to_string().as_str()).cloned())
        else {
            return;
        };
        if local.decl.is_some_and(|d| self.ptrs.cells.contains(&d)) {
            let r = self.range(b.syntax());
            self.ptrs.repoints.insert(r);
            self.ptr_cell(&rhs);
        }
    }

    /// `p.is_null()`, `p.as_ref()`, `p.as_mut()` on a raw pointer; `None` for other methods.
    pub(super) fn ptr_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        if !self.is_ptr(recv) {
            return None;
        }
        let t = self.infer(recv, None);
        let in_cell = self.cellable(&t);
        let out = match method {
            "is_null" => Ty::Bool,
            "as_ref" | "as_mut" => {
                if matches!(self.zonk(&t), Ty::Option(_)) {
                    self.error(
                        m.syntax(),
                        "`as_ref()` on a pointer to an `Option` isn't supported (`Option<Option<T>>` has no JS representation)",
                    );
                }
                // `if let Some(n) = p.as_mut()` of a number, string or bool: `n` binds a slot
                // over the place (`κσ.$ptrSlot`), so writes through `*n` reach it.
                if method == "as_mut" && in_cell {
                    let r = self.range(m.syntax());
                    self.slot_calls.insert(r);
                }
                Ty::Option(Box::new(t))
            }
            "read" => t,
            "write" => {
                if let Some(a) = args.first() {
                    self.check(a, &t);
                }
                if !in_cell {
                    self.error(
                        m.syntax(),
                        "`write()` through a pointer to a struct or collection isn't supported yet; assign `*p = ..`",
                    );
                }
                Ty::Unit
            }
            "add" | "sub" | "offset" | "wrapping_add" | "wrapping_sub" | "wrapping_offset"
            | "byte_add" | "byte_sub" | "offset_from" | "addr" | "cast" | "cast_mut"
            | "cast_const" => {
                let msg = format!(
                    "`{method}()` on a raw pointer isn't supported: values have no memory layout to point into (use indices into the Vec)"
                );
                self.error(m.syntax(), msg);
                for a in args {
                    self.infer(a, None);
                }
                return Some(Ty::Unknown);
            }
            _ => return None,
        };
        if in_cell {
            self.ptr_cell(recv);
        }
        let r = self.range(m.syntax());
        self.ptrs.methods.insert(r, in_cell);
        Some(out)
    }

    /// `ptr::null()`, `ptr::eq(a, b)`, `Box::into_raw(b)`, `Box::from_raw(p)`,
    /// `mem::transmute(x)`.
    pub(super) fn ptr_call(
        &mut self,
        c: &ast::CallExpr,
        tail: &[&str],
        args: &[ast::Expr],
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        let count = |c2: &mut Self, n: usize| c2.arg_count(c.syntax(), "function", n, args.len());
        let turbofish: Vec<ast::Type> = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.generic_arg_list())
                .map(|l| {
                    l.generic_args()
                        .filter_map(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        Some(match tail {
            ["ptr", "null" | "null_mut"] => {
                count(self, 0);
                match turbofish.first() {
                    Some(t) => self.lower_type(t),
                    None => match expected {
                        Some(t) => t.clone(),
                        None => self.fresh(VarKind::General),
                    },
                }
            }
            ["ptr", "eq" | "addr_eq"] => {
                count(self, 2);
                let t = self.fresh(VarKind::General);
                for a in args.iter().take(2) {
                    self.check(a, &t);
                }
                if self.cellable(&t) {
                    for a in args.iter().take(2) {
                        self.ptr_cell(a);
                    }
                }
                Ty::Bool
            }
            ["ptr", "read"] => {
                count(self, 1);
                let t = match args.first() {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                };
                if let Some(a) = args.first()
                    && self.cellable(&t)
                {
                    self.ptr_cell(a);
                }
                t
            }
            ["Box", "into_raw" | "leak"] => {
                count(self, 1);
                match args.first() {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                }
            }
            ["Box", "from_raw"] => {
                count(self, 1);
                match args.first() {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                }
            }
            ["mem", "transmute"] => {
                count(self, 1);
                let from = match (args.first(), turbofish.first()) {
                    (Some(a), Some(t)) => {
                        let t = self.lower_type(t);
                        self.check(a, &t)
                    }
                    (Some(a), None) => self.infer(a, None),
                    (None, _) => Ty::Unknown,
                };
                let to = match (turbofish.get(1), expected) {
                    (Some(t), _) => self.lower_type(t),
                    (None, Some(t)) => t.clone(),
                    (None, None) => self.fresh(VarKind::General),
                };
                let (f, t) = (self.zonk(&from).default_lit(), self.zonk(&to));
                if matches!(t, Ty::Var(_)) {
                    self.error(
                        c.syntax(),
                        "type annotations needed: `transmute`'s target type (annotate the binding)",
                    );
                    return Some(to);
                }
                match (layout(&f), layout(&t)) {
                    (Some((a, _)), Some((b, _))) if a == b => {}
                    (Some((a, _)), Some((b, _))) => {
                        let msg = format!(
                            "cannot transmute between types of different sizes, or dependently-sized types (`{}` is {} bits, `{}` is {} bits)",
                            self.describe(&f),
                            a * 8,
                            self.describe(&t),
                            b * 8
                        );
                        self.error(c.syntax(), msg);
                    }
                    _ => {
                        let msg = format!(
                            "`transmute` from `{}` to `{}` isn't supported: values have no memory layout to reinterpret (only numbers, `char` and `bool` of the same size)",
                            self.describe(&f),
                            self.describe(&t)
                        );
                        self.error(c.syntax(), msg);
                    }
                }
                to
            }
            _ => return None,
        })
    }
}
