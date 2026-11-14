//! `From` / `Into` conversions: `impl From<X> for Y`, `Y::from(x)`, `x.into()` and the error
//! conversion `?` does. A type may implement `From` several times, so each impl's `from` is its
//! own static method (`from$0`, `from$1`, ..); the checker picks one per call by the argument's
//! type and records it in `Types::conversions` for codegen. A child module of `typeck`.

use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::ArithOp};

use super::{Checker, VarKind};
use crate::adt::{Conv, FromImpl};
use crate::ty::{IntKind, Ty};

impl Checker<'_> {
    /// Whether `a` and `b` could be the same type once inference finishes (no side effects).
    pub(super) fn could_be(&self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        match (&a, &b) {
            (Ty::Unknown, _) | (_, Ty::Unknown) | (Ty::Never, _) | (_, Ty::Never) => true,
            (Ty::Var(_), Ty::Var(_)) => true,
            (Ty::Var(_), t) | (t, Ty::Var(_)) => {
                let v = if matches!(a, Ty::Var(_)) { &a } else { &b };
                match self.var_kind(v) {
                    Some(VarKind::Int) => matches!(t, Ty::Int(_) | Ty::IntLit),
                    Some(VarKind::Float) => matches!(t, Ty::Float { .. }),
                    _ => true,
                }
            }
            (Ty::Adt(x, xa), Ty::Adt(y, ya)) => {
                x == y
                    && xa.len() == ya.len()
                    && xa.iter().zip(ya).all(|(p, q)| self.could_be(p, q))
            }
            _ if std::mem::discriminant(&a) == std::mem::discriminant(&b) => {
                let (ia, ib) = (a.inner_types(), b.inner_types());
                match (&a, &b) {
                    (Ty::Int(x), Ty::Int(y)) => x == y,
                    (Ty::Float { f32: x }, Ty::Float { f32: y }) => x == y,
                    (Ty::Dyn(x), Ty::Dyn(y)) | (Ty::Extern(x), Ty::Extern(y)) => x == y,
                    _ => {
                        ia.len() == ib.len() && ia.iter().zip(&ib).all(|(p, q)| self.could_be(p, q))
                    }
                }
            }
            _ => false,
        }
    }

    /// The `impl From<X> for adt` that converts a `src`, if exactly one can; unifies `X` with it.
    fn pick_from(&mut self, adt: &str, src: &Ty) -> Option<String> {
        let impls: Vec<FromImpl> = self.adts.from_impls.get(adt).cloned().unwrap_or_default();
        let fits: Vec<&FromImpl> = impls
            .iter()
            .filter(|i| self.could_be(&i.src, src))
            .collect();
        let [one] = fits.as_slice() else { return None };
        self.unify(&one.src, src);
        Some(one.key.clone())
    }

    /// A call of a user trait method on a built-in type, for codegen.
    pub(super) fn record_ext(&mut self, node: &SyntaxNode, key: &str) {
        self.record_conv(
            node,
            Conv::Ext {
                key: key.to_string(),
            },
        );
    }

    fn record_conv(&mut self, node: &SyntaxNode, conv: Conv) {
        let r = self.range(node);
        self.conversions.insert(r, conv);
    }

    fn conv_impl_list(&self, adt: &str) -> String {
        let impls = self.adts.from_impls.get(adt).cloned().unwrap_or_default();
        impls
            .iter()
            .map(|i| format!("`From<{}>`", self.describe(&i.src)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `Y::from(x)` for a struct or enum with `impl From<X> for Y`.
    pub(super) fn conv_from_call(
        &mut self,
        c: &ast::CallExpr,
        adt: &str,
        args: &[ast::Expr],
    ) -> Ty {
        self.arg_count(c.syntax(), "function", 1, args.len());
        let Some(a) = args.first() else {
            return Ty::Unknown;
        };
        let at = self.infer(a, None);
        match self.pick_from(adt, &at) {
            Some(key) => self.record_conv(
                c.syntax(),
                Conv::From {
                    adt: adt.to_string(),
                    key,
                },
            ),
            None => {
                let msg = format!(
                    "the trait bound `{adt}: From<{}>` is not satisfied (`{adt}` implements {})",
                    self.describe(&at),
                    self.conv_impl_list(adt)
                );
                self.error(c.syntax(), msg);
            }
        }
        let targs = self.fresh_args(adt);
        Ty::Adt(adt.to_string(), targs)
    }

    /// `x.into()`: the target type usually comes from the context, so it is settled at the end
    /// of the function.
    pub(super) fn conv_into_call(&mut self, m: &ast::MethodCallExpr, recv: Ty) -> Ty {
        let target = self.fresh(VarKind::General);
        self.pending_intos
            .push((recv, target.clone(), m.syntax().clone(), self.base));
        target
    }

    pub(super) fn resolve_intos(&mut self) {
        let saved = self.base;
        for (src, target, node, base) in std::mem::take(&mut self.pending_intos) {
            // Calls inside macro arguments were parsed from the argument's text.
            self.base = base;
            let (s, t) = (self.shallow(&src), self.shallow(&target));
            let conv = match (&s, &t) {
                (_, Ty::Var(_)) if self.var_kind(&t) == Some(VarKind::General) => {
                    self.error(&node, "type annotations needed: say what `.into()` converts to (e.g. `let y: Y = x.into();`)");
                    continue;
                }
                (_, Ty::Adt(adt, _))
                    if self.adts.from_impls.contains_key(adt) && !self.could_be(&s, &t) =>
                {
                    let adt = adt.clone();
                    self.pick_from(&adt, &s).map(|key| Conv::From { adt, key })
                }
                _ if self.lossless(&s, &t) => Some(Conv::Identity),
                _ if self.could_be(&s, &t) => {
                    self.unify(&s, &t);
                    Some(Conv::Identity)
                }
                _ => None,
            };
            match conv {
                Some(c) => self.record_conv(&node, c),
                None => {
                    let msg = format!(
                        "the trait bound `{}: From<{}>` is not satisfied",
                        self.describe(&t),
                        self.describe(&s)
                    );
                    self.error(&node, msg);
                }
            }
        }
        self.base = saved;
    }

    /// Std conversions that are the identity in JS: `&str` → `String`, and lossless numeric
    /// widenings (`i32` → `i64`, `u8` → `u32`, `i32` → `f64`, ..).
    fn lossless(&mut self, s: &Ty, t: &Ty) -> bool {
        use IntKind::*;
        let bits = |k: IntKind| match k {
            I8 | U8 => 8,
            I16 | U16 => 16,
            I32 | U32 => 32,
            I64 | U64 | Isize | Usize => 64,
            I128 | U128 => 128,
        };
        let s = if self.var_kind(s) == Some(VarKind::Int) {
            Ty::IntLit
        } else {
            self.shallow(s)
        };
        match (&s, &self.shallow(t)) {
            (Ty::Str, Ty::Str) => true,
            (Ty::IntLit, Ty::Int(_) | Ty::Float { .. }) => true,
            (Ty::Int(a), Ty::Int(b)) => {
                *a == *b
                    || (a.is_signed() == b.is_signed()
                        && bits(*a) < bits(*b)
                        && !matches!(b, Isize | Usize))
                    || (!a.is_signed()
                        && b.is_signed()
                        && bits(*a) < bits(*b)
                        && !matches!(b, Isize))
                    || matches!((a, b), (U8 | U16, Usize) | (I8 | I16, Isize) | (U8, Isize))
            }
            (Ty::Int(a), Ty::Float { f32 }) => bits(*a) <= if *f32 { 16 } else { 32 },
            (Ty::Float { f32: true }, Ty::Float { .. }) => true,
            _ => false,
        }
    }

    /// `a + b` (or `a += b`) where `a` is a struct or enum: its `impl Add<R>` for `b`'s type.
    pub(super) fn op_overload(
        &mut self,
        node: &SyntaxNode,
        op: ArithOp,
        assign: bool,
        adt: &str,
        lt: &Ty,
        rhs: &ast::Expr,
    ) -> Ty {
        let base = match op {
            ArithOp::Add => "Add",
            ArithOp::Sub => "Sub",
            ArithOp::Mul => "Mul",
            ArithOp::Div => "Div",
            ArithOp::Rem => "Rem",
            ArithOp::BitAnd => "BitAnd",
            ArithOp::BitOr => "BitOr",
            ArithOp::BitXor => "BitXor",
            _ => "",
        };
        let tr = if assign {
            format!("{base}Assign")
        } else {
            base.to_string()
        };
        let rt = self.infer(rhs, None);
        let impls: Vec<crate::adt::OpImpl> = self
            .adts
            .op_impls
            .get(adt)
            .map(|v| v.iter().filter(|i| i.trait_name == tr).cloned().collect())
            .unwrap_or_default();
        let fits: Vec<&crate::adt::OpImpl> = impls
            .iter()
            .filter(|i| self.could_be(&i.rhs, &rt))
            .collect();
        if let [one] = fits.as_slice() {
            self.unify(&one.rhs, &rt);
            let key = one.key.clone();
            self.record_conv(node, Conv::Method { key: key.clone() });
            if assign {
                return Ty::Unit;
            }
            return self
                .adts
                .method(adt, &key)
                .map(|m| m.ret.clone())
                .unwrap_or(Ty::Unknown);
        }
        let (l, r) = (self.describe(lt), self.describe(&rt));
        let sym = format!("{op}{}", if assign { "=" } else { "" });
        let msg = if impls.is_empty() {
            format!(
                "cannot apply `{sym}` to `{l}` and `{r}` (add `impl {tr}<{r}> for {adt}` from `std::ops`)"
            )
        } else {
            format!(
                "no implementation for `{l} {sym} {r}` (`{adt}` implements {})",
                impls
                    .iter()
                    .map(|i| format!("`{tr}<{}>`", self.describe(&i.rhs)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        self.error(node, msg);
        Ty::Unknown
    }

    /// `grid[i]` where `grid` is a struct or enum: its `impl Index<I>` for `i`'s type. Reading
    /// works (also a field or method of the result); assigning to the element itself would need
    /// `IndexMut` to hand out a place, which JS can't do for numbers and strings.
    pub(super) fn index_overload(
        &mut self,
        ix: &ast::IndexExpr,
        adt: &str,
        bt: &Ty,
        index: &ast::Expr,
    ) -> Ty {
        let it = self.infer(index, None);
        let impls: Vec<crate::adt::OpImpl> = self
            .adts
            .op_impls
            .get(adt)
            .map(|v| {
                v.iter()
                    .filter(|i| i.trait_name == "Index")
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let fits: Vec<&crate::adt::OpImpl> = impls
            .iter()
            .filter(|i| self.could_be(&i.rhs, &it))
            .collect();
        let [one] = fits.as_slice() else {
            let msg = format!(
                "the type `{}` cannot be indexed by `{}` (add `impl Index<{}> for {adt}` from `std::ops`)",
                self.describe(bt),
                self.describe(&it),
                self.describe(&it)
            );
            self.error(ix.syntax(), msg);
            return Ty::Unknown;
        };
        self.unify(&one.rhs, &it);
        let key = one.key.clone();
        let assigned = ix
            .syntax()
            .parent()
            .and_then(ast::BinExpr::cast)
            .is_some_and(|b| {
                matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                    && b.lhs().is_some_and(|l| l.syntax() == ix.syntax())
            });
        // `names[i].push('!')`: a string element is a JS value, so a method on it needs the place.
        let elem_is_str = self
            .adts
            .method(adt, &key)
            .is_some_and(|m| self.shallow(&m.ret) == Ty::Str);
        let receiver = elem_is_str
            && ix
                .syntax()
                .parent()
                .and_then(ast::MethodCallExpr::cast)
                .is_some_and(|m| m.receiver().is_some_and(|r| r.syntax() == ix.syntax()));
        let has_index_mut = self
            .adts
            .op_impls
            .get(adt)
            .is_some_and(|v| v.iter().any(|i| i.trait_name == "IndexMut"));
        let assigned = assigned || (receiver && has_index_mut);
        if assigned {
            // `grid[i] = v`: `impl IndexMut<I>` for the same index type hands out the place.
            let index_mut = self.adts.op_impls.get(adt).and_then(|v| {
                v.iter()
                    .find(|i| i.trait_name == "IndexMut" && self.could_be(&i.rhs, &it))
                    .cloned()
            });
            match index_mut {
                Some(_) if self.opaque_index_mut.contains(adt) => {
                    let msg = format!(
                        "assigning through `{adt}`'s `index_mut` needs it to end in `&mut <place>` (like `&mut self.cells[i]`), so the write reaches the place"
                    );
                    self.error(ix.syntax(), msg);
                }
                Some(im) => {
                    let r = self.range(ix.syntax());
                    self.index_places.insert(r);
                    self.record_conv(
                        ix.syntax(),
                        Conv::Method {
                            key: im.key.clone(),
                        },
                    );
                    return self
                        .adts
                        .method(adt, &key)
                        .map(|m| m.ret.clone())
                        .unwrap_or(Ty::Unknown);
                }
                None => {
                    let msg = format!(
                        "cannot assign to data in an index of `{}` (add `impl IndexMut<{}> for {adt}` from `std::ops`)",
                        self.describe(bt),
                        self.describe(&it)
                    );
                    self.error(ix.syntax(), msg);
                }
            }
        }
        self.record_conv(ix.syntax(), Conv::Method { key: key.clone() });
        self.adts
            .method(adt, &key)
            .map(|m| m.ret.clone())
            .unwrap_or(Ty::Unknown)
    }

    /// `-a` / `!a` where `a` is a struct or enum: its `impl Neg` / `impl Not`.
    pub(super) fn unary_overload(&mut self, node: &SyntaxNode, neg: bool, adt: &str, t: &Ty) -> Ty {
        let tr = if neg { "Neg" } else { "Not" };
        let found = self
            .adts
            .op_impls
            .get(adt)
            .and_then(|v| v.iter().find(|i| i.trait_name == tr))
            .map(|i| i.key.clone());
        match found {
            Some(key) => {
                self.record_conv(node, Conv::Method { key: key.clone() });
                self.adts
                    .method(adt, &key)
                    .map(|m| m.ret.clone())
                    .unwrap_or(Ty::Unknown)
            }
            None => {
                let msg = format!(
                    "cannot apply unary operator `{}` to type `{}` (add `impl {tr} for {adt}` from `std::ops`)",
                    if neg { "-" } else { "!" },
                    self.describe(t)
                );
                self.error(node, msg);
                Ty::Unknown
            }
        }
    }

    /// `?` on a `Result<_, E1>` in a function returning `Result<_, E2>`: when `E2: From<E1>`,
    /// the error is converted. Returns whether a conversion was recorded.
    pub(super) fn try_convert(&mut self, t: &ast::TryExpr, e: &Ty, re: &Ty) -> bool {
        // Into `Box<dyn Error>`: std errors and your `impl Error` types, unchanged.
        if self.shallow(re) == Ty::Dyn("Error".into()) {
            let ok = match self.shallow(e) {
                Ty::Extern(n) => super::res::is_std_error(&n),
                Ty::Adt(n, _) => self
                    .adts
                    .defs
                    .get(&n)
                    .is_some_and(|d| d.implements("Error")),
                Ty::Dyn(tr) => tr == "Error",
                _ => false,
            };
            if ok {
                return true;
            }
        }
        let Ty::Adt(adt, _) = self.shallow(re) else {
            return false;
        };
        if self.could_be(e, re) || !self.adts.from_impls.contains_key(&adt) {
            return false;
        }
        match self.pick_from(&adt, e) {
            Some(key) => {
                self.record_conv(t.syntax(), Conv::From { adt, key });
                true
            }
            None => false,
        }
    }
}
