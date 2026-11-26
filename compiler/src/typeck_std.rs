//! More of std's surface: string searching, splitting and editing, integer arithmetic variants
//! (`checked_*`, `saturating_*`, `wrapping_*`, bit counts), `Vec` searching and rearranging,
//! `char::from_digit` and friends, and `std::mem::{swap, replace, take}`. A child module of
//! `typeck`.

use ra_ap_syntax::{AstNode, ast};

use super::{Checker, Mutation, USIZE, VarKind};
use crate::ty::{IntKind, Ty};

const U32: Ty = Ty::Int(IntKind::U32);

/// An iterator over `elem`s.
fn iter_of(elem: Ty) -> Ty {
    Ty::Iter {
        elem: Box::new(elem),
        mutable: false,
    }
}

/// The unsigned type of the same width (`i32::abs_diff` returns `u32`).
fn unsigned(k: IntKind) -> IntKind {
    match k {
        IntKind::I8 => IntKind::U8,
        IntKind::I16 => IntKind::U16,
        IntKind::I32 => IntKind::U32,
        IntKind::I64 => IntKind::U64,
        IntKind::Isize => IntKind::Usize,
        other => other,
    }
}

impl Checker<'_> {
    /// A string pattern: `&str`, `char`, a `char` predicate closure, or a `[char]` (any of them).
    pub(super) fn str_pattern(&mut self, a: Option<&ast::Expr>) {
        let Some(a) = a else { return };
        let t = self.infer(a, None);
        match self.shallow(&t) {
            Ty::Str | Ty::Char | Ty::Unknown => {}
            Ty::Vec(e) if matches!(self.shallow(&e), Ty::Char | Ty::Unknown | Ty::Var(_)) => {
                self.unify(&e, &Ty::Char);
            }
            Ty::Closure { params, ret } if params.len() == 1 => {
                self.unify(&params[0], &Ty::Char);
                if !self.unify(&ret, &Ty::Bool) {
                    self.error(a.syntax(), "a pattern closure must return `bool`");
                }
            }
            Ty::Var(_) => {
                // An untyped closure: give it the predicate type.
                let want = Ty::Closure {
                    params: vec![Ty::Char],
                    ret: Box::new(Ty::Bool),
                };
                self.unify(&t, &want);
            }
            other => {
                let msg = format!(
                    "expected a `&str`, `char`, `&[char]` or `|c: char| -> bool` pattern, found `{}`",
                    self.describe(&other)
                );
                self.error(a.syntax(), msg);
            }
        }
    }

    /// Methods from this module; `None` when `method` isn't one of them.
    pub(super) fn std_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        rt: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let a0 = args.first();
        let a1 = args.get(1);
        let count = |c: &mut Self, n: usize| c.arg_count(m.syntax(), "method", n, args.len());
        let opt = |t: Ty| Ty::Option(Box::new(t));
        let iter = |t: Ty| Ty::Iter {
            elem: Box::new(t),
            mutable: false,
        };
        Some(match (self.shallow(rt), method) {
            // ---- strings
            (Ty::Str, "find" | "rfind") => {
                count(self, 1);
                self.str_pattern(a0);
                opt(USIZE)
            }
            (Ty::Str, "split_once" | "rsplit_once") => {
                count(self, 1);
                self.str_pattern(a0);
                opt(Ty::Tuple(vec![Ty::Str, Ty::Str]))
            }
            (Ty::Str, "strip_prefix" | "strip_suffix") => {
                count(self, 1);
                self.str_pattern(a0);
                opt(Ty::Str)
            }
            (Ty::Str, "trim_matches" | "trim_start_matches" | "trim_end_matches") => {
                count(self, 1);
                self.str_pattern(a0);
                Ty::Str
            }
            (Ty::Str, "contains" | "starts_with" | "ends_with") => {
                count(self, 1);
                self.str_pattern(a0);
                Ty::Bool
            }
            (Ty::Str, "replace") => {
                count(self, 2);
                self.str_pattern(a0);
                if let Some(a) = a1 {
                    self.check(a, &Ty::Str);
                }
                Ty::Str
            }
            (Ty::Str, "replacen") => {
                count(self, 3);
                self.str_pattern(a0);
                if let Some(a) = a1 {
                    self.check(a, &Ty::Str);
                }
                if let Some(a) = args.get(2) {
                    self.check(a, &USIZE);
                }
                Ty::Str
            }
            (Ty::Str, "char_indices") => {
                count(self, 0);
                iter(Ty::Tuple(vec![USIZE, Ty::Char]))
            }
            (Ty::Str, "bytes") => {
                count(self, 0);
                iter(Ty::Int(IntKind::U8))
            }
            (Ty::Str, "rsplit" | "matches" | "split_terminator") => {
                count(self, 1);
                self.str_pattern(a0);
                iter(Ty::Str)
            }
            (Ty::Str, "splitn" | "rsplitn") => {
                count(self, 2);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                self.str_pattern(a1);
                iter(Ty::Str)
            }
            // (`into_owned()`: what a `Cow<str>` gives; `replace` and `from_utf8_lossy` give a
            // `String` here.)
            (Ty::Str, "to_ascii_uppercase" | "to_ascii_lowercase" | "to_owned" | "into_owned") => {
                count(self, 0);
                Ty::Str
            }
            (Ty::Str, "eq_ignore_ascii_case") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Str);
                }
                Ty::Bool
            }
            (Ty::Str, "is_ascii") => {
                count(self, 0);
                Ty::Bool
            }
            (Ty::Str, "is_char_boundary") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                Ty::Bool
            }
            (Ty::Str, "insert" | "insert_str" | "pop" | "truncate" | "clear" | "remove") => {
                self.check_mutable(recv, Mutation::Borrow);
                match method {
                    "insert" | "insert_str" => {
                        count(self, 2);
                        if let Some(a) = a0 {
                            self.check(a, &USIZE);
                        }
                        if let Some(a) = a1 {
                            self.check(
                                a,
                                if method == "insert" {
                                    &Ty::Char
                                } else {
                                    &Ty::Str
                                },
                            );
                        }
                        Ty::Unit
                    }
                    "pop" => {
                        count(self, 0);
                        opt(Ty::Char)
                    }
                    "remove" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &USIZE);
                        }
                        Ty::Char
                    }
                    "truncate" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &USIZE);
                        }
                        Ty::Unit
                    }
                    _ => {
                        count(self, 0);
                        Ty::Unit
                    }
                }
            }

            // `s.as_ref()` / `v.borrow()` (`AsRef<str>`, `AsRef<[T]>`): the value itself.
            (Ty::Str | Ty::Vec(_) | Ty::Dyn(_), "as_ref" | "borrow") => {
                count(self, 0);
                rt.clone()
            }
            // `b.as_mut()` / `b.borrow_mut()` for `B: AsMut<[u8]>`: the same `Vec`.
            (Ty::Vec(_), "as_mut" | "borrow_mut") => {
                count(self, 0);
                rt.clone()
            }
            // ---- integers
            // (`unchecked_*` overflows into undefined behavior; here it wraps.)
            (Ty::Int(k), m2)
                if m2.starts_with("checked_")
                    || m2.starts_with("saturating_")
                    || m2.starts_with("wrapping_")
                    || matches!(m2, "unchecked_add" | "unchecked_sub" | "unchecked_mul") =>
            {
                let op = m2.split_once('_').map(|x| x.1).unwrap_or("");
                match op {
                    "add" | "sub" | "mul" | "div" | "rem" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &Ty::Int(k));
                        }
                    }
                    "pow" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &U32);
                        }
                    }
                    "neg" | "abs" => count(self, 0),
                    // `index.checked_add_signed(-1)`: the signed kind of the same width.
                    "add_signed" | "sub_signed" if !k.is_signed() => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &Ty::Int(signed_of(k)));
                        }
                    }
                    _ => return None,
                }
                if m2.starts_with("checked_") {
                    opt(Ty::Int(k))
                } else {
                    Ty::Int(k)
                }
            }
            (Ty::Int(k), "abs_diff") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Int(k));
                }
                Ty::Int(unsigned(k))
            }
            (Ty::Int(_), "count_ones" | "count_zeros" | "leading_zeros" | "trailing_zeros") => {
                count(self, 0);
                U32
            }
            (Ty::Int(_), "is_power_of_two") => {
                count(self, 0);
                Ty::Bool
            }
            (Ty::Int(k), "is_multiple_of") if !k.is_signed() => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Int(k));
                }
                Ty::Bool
            }
            (Ty::Int(k), "div_euclid") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Int(k));
                }
                Ty::Int(k)
            }
            (Ty::Int(k), "isqrt") => {
                count(self, 0);
                Ty::Int(k)
            }
            (Ty::Int(_), "ilog2" | "ilog10") => {
                count(self, 0);
                U32
            }
            (Ty::Int(k), "is_positive" | "is_negative") if k.is_signed() => {
                count(self, 0);
                Ty::Bool
            }
            (Ty::Float { f32 }, "to_bits") => {
                count(self, 0);
                Ty::Int(if f32 { IntKind::U32 } else { IntKind::U64 })
            }
            (
                Ty::Float { f32 },
                "hypot" | "rem_euclid" | "div_euclid" | "copysign" | "log" | "abs_sub",
            ) => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Float { f32 });
                }
                Ty::Float { f32 }
            }
            (Ty::Float { f32 }, "mul_add") => {
                count(self, 2);
                for a in args.iter().take(2) {
                    self.check(a, &Ty::Float { f32 });
                }
                Ty::Float { f32 }
            }
            (
                Ty::Float { f32 },
                "to_radians" | "to_degrees" | "recip" | "fract" | "signum" | "exp2" | "exp_m1"
                | "ln_1p" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh",
            ) => {
                count(self, 0);
                Ty::Float { f32 }
            }
            (
                Ty::Float { .. },
                "is_sign_negative" | "is_sign_positive" | "is_infinite" | "is_normal",
            ) => {
                count(self, 0);
                Ty::Bool
            }
            (Ty::Float { f32 }, "total_cmp") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Float { f32 });
                }
                Ty::Ordering
            }

            // ---- Vec
            (Ty::Vec(e), "binary_search") => {
                count(self, 1);
                self.require_ord(m.syntax(), &e, "binary_search");
                if let Some(a) = a0 {
                    self.check(a, &e);
                }
                Ty::Result(Box::new(USIZE), Box::new(USIZE))
            }
            (Ty::Vec(e), "binary_search_by") => {
                count(self, 1);
                self.closure_arg(a0, vec![*e], Ty::Ordering);
                Ty::Result(Box::new(USIZE), Box::new(USIZE))
            }
            (Ty::Vec(e), "binary_search_by_key") => {
                count(self, 2);
                let k = self.fresh(VarKind::General);
                if let Some(a) = a0 {
                    self.check(a, &k);
                }
                self.closure_arg(a1, vec![*e], k);
                Ty::Result(Box::new(USIZE), Box::new(USIZE))
            }
            (Ty::Vec(e), "as_slice" | "as_mut_slice") => {
                count(self, 0);
                Ty::Vec(e)
            }
            (Ty::Vec(e), "split_first" | "split_last") => {
                count(self, 0);
                opt(Ty::Tuple(vec![(*e).clone(), Ty::Vec(e)]))
            }
            (Ty::Vec(e), "split_at" | "split_at_mut") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                Ty::Tuple(vec![Ty::Vec(e.clone()), Ty::Vec(e)])
            }
            (Ty::Vec(e), "starts_with" | "ends_with") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Vec(e));
                }
                Ty::Bool
            }
            (Ty::Vec(e), "repeat") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                Ty::Vec(e)
            }
            (Ty::Vec(_), "is_sorted") => {
                // (`PartialOrd` is enough: floats too, where a NaN makes it `false`.)
                count(self, 0);
                Ty::Bool
            }
            (
                Ty::Vec(e),
                "drain" | "split_off" | "append" | "fill" | "rotate_left" | "rotate_right"
                | "swap_remove" | "copy_from_slice" | "clone_from_slice",
            ) => {
                self.check_mutable(recv, Mutation::Borrow);
                match method {
                    "drain" => {
                        count(self, 1);
                        if let Some(ast::Expr::RangeExpr(r)) = a0 {
                            use ra_ap_syntax::ast::RangeItem;
                            for b in [r.start(), r.end()].into_iter().flatten() {
                                self.check(&b, &USIZE);
                            }
                        } else if let Some(a) = a0 {
                            self.error(
                                a.syntax(),
                                "`drain` takes a range: `v.drain(..)`, `v.drain(1..3)`",
                            );
                        }
                        iter(*e)
                    }
                    "split_off" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &USIZE);
                        }
                        Ty::Vec(e)
                    }
                    "append" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            if let ast::Expr::RefExpr(r) = a
                                && let Some(inner) = r.expr()
                            {
                                self.check_mutable(&inner, Mutation::Borrow);
                            }
                            self.check(a, &Ty::Vec(e));
                        }
                        Ty::Unit
                    }
                    "fill" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &e);
                        }
                        Ty::Unit
                    }
                    "copy_from_slice" | "clone_from_slice" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &Ty::Vec(e));
                        }
                        Ty::Unit
                    }
                    "swap_remove" => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &USIZE);
                        }
                        *e
                    }
                    _ => {
                        count(self, 1);
                        if let Some(a) = a0 {
                            self.check(a, &USIZE);
                        }
                        Ty::Unit
                    }
                }
            }
            _ => return None,
        })
    }

    /// `char::from_digit`, `String::with_capacity`, `u32::from(x)`, `std::mem::swap` and friends.
    pub(super) fn std_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let tail = match names {
            ["std" | "core", rest @ ..] => rest,
            all => all,
        };
        // `std::rc::Rc::downgrade` / `sync::Arc::..`
        let tail = match tail {
            ["rc" | "sync", rest @ ..] => rest,
            t => t,
        };
        let count = |c2: &mut Self, n: usize| c2.arg_count(c.syntax(), "function", n, args.len());
        let a0 = args.first();
        let a1 = args.get(1);
        Some(match tail {
            // The script's arguments (the host's, `κσ.$setArgs`), its name first.
            ["env", "args" | "args_os"] => {
                count(self, 0);
                Ty::Iter {
                    elem: Box::new(Ty::Str),
                    mutable: false,
                }
            }
            ["process", "exit"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Int(IntKind::I32));
                }
                Ty::Never
            }
            ["char", "from_digit"] => {
                count(self, 2);
                for a in args.iter().take(2) {
                    self.check(a, &U32);
                }
                Ty::Option(Box::new(Ty::Char))
            }
            ["char", "from_u32"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &U32);
                }
                Ty::Option(Box::new(Ty::Char))
            }
            ["char", "from"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Int(IntKind::U8));
                }
                Ty::Char
            }
            // `u32::from_str_radix("ff", 16)`
            [t, "from_str_radix"] if matches!(Ty::from_name(t), Ty::Int(_)) => {
                count(self, 2);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Str);
                }
                if let Some(a) = a1 {
                    self.check(a, &U32);
                }
                Ty::Result(
                    Box::new(Ty::from_name(t)),
                    Box::new(Ty::Extern("ParseIntError".into())),
                )
            }
            // `str::from_utf8(&bytes)`, `String::from_utf8(bytes)`
            ["str" | "String", "from_utf8"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Vec(Box::new(Ty::Int(IntKind::U8))));
                }
                Ty::Result(Box::new(Ty::Str), Box::new(Ty::Extern("Utf8Error".into())))
            }
            ["String", "from_utf8_lossy"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Vec(Box::new(Ty::Int(IntKind::U8))));
                }
                Ty::Str
            }
            // (Valid UTF-8 is the caller's promise; invalid bytes decode as `from_utf8_lossy`.)
            ["str" | "String", "from_utf8_unchecked"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Vec(Box::new(Ty::Int(IntKind::U8))));
                }
                Ty::Str
            }
            ["String", "with_capacity"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                Ty::Str
            }
            [t, "from"]
                if Ty::from_name(t) != Ty::Unknown
                    && matches!(Ty::from_name(t), Ty::Int(_) | Ty::Float { .. }) =>
            {
                // Lossless numeric conversion: `u64::from(x)`, `f64::from(x)`.
                count(self, 1);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                Ty::from_name(t)
            }
            // `Rc::downgrade(&rc)`: a `Weak` is the value itself (or `null`).
            // `Rc::default()`: the value's own default (an `Rc` is the value here).
            ["Rc" | "Arc" | "Box", "default"] => {
                count(self, 0);
                self.fresh(VarKind::General)
            }
            ["Rc" | "Arc", "downgrade"] => {
                count(self, 1);
                let t = a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown);
                Ty::Option(Box::new(t))
            }
            ["Weak", "new"] => {
                count(self, 0);
                Ty::Option(Box::new(self.fresh(VarKind::General)))
            }
            ["Rc" | "Arc", "ptr_eq"] => {
                count(self, 2);
                let t = self.fresh(VarKind::General);
                for a in args.iter().take(2) {
                    self.check(a, &t);
                }
                Ty::Bool
            }
            ["Rc" | "Arc", f @ ("strong_count" | "weak_count")] => {
                let msg = format!(
                    "`Rc::{f}` isn't supported: JS values have no reference counts (the garbage collector frees them)"
                );
                self.error(c.syntax(), msg);
                for a in args {
                    self.infer(a, None);
                }
                USIZE
            }
            ["DefaultHasher", "new" | "default"] => {
                count(self, 0);
                Ty::Hasher
            }
            // `u8::try_from(300)`: `Err(TryFromIntError(()))` when out of range.
            // `char::try_from(0x41u32)`
            ["char", "try_from"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &U32);
                }
                Ty::Result(
                    Box::new(Ty::Char),
                    Box::new(Ty::Extern("CharTryFromError".into())),
                )
            }
            [t, "try_from"] if matches!(Ty::from_name(t), Ty::Int(_)) => {
                count(self, 1);
                if let Some(a) = a0 {
                    let s = self.infer(a, None);
                    let ok = matches!(self.shallow(&s), Ty::Int(_) | Ty::Unknown | Ty::Var(_));
                    if !ok {
                        let msg = format!(
                            "the trait bound `{t}: TryFrom<{}>` is not satisfied",
                            self.describe(&s)
                        );
                        self.error(a.syntax(), msg);
                    }
                }
                Ty::Result(
                    Box::new(Ty::from_name(t)),
                    Box::new(Ty::Extern("TryFromIntError".into())),
                )
            }
            // `u32::default()`, `String::default()`
            [t, "default"] if Ty::from_name(t) != Ty::Unknown => {
                count(self, 0);
                Ty::from_name(t)
            }
            // `std::iter::repeat(x)` and friends.
            ["iter", "repeat" | "once"] => {
                count(self, 1);
                let t = a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown);
                iter_of(t)
            }
            ["iter", "empty"] => {
                count(self, 0);
                iter_of(self.fresh(VarKind::General))
            }
            ["iter", "repeat_n"] => {
                count(self, 2);
                let t = a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown);
                if let Some(a) = a1 {
                    self.check(a, &USIZE);
                }
                iter_of(t)
            }
            ["iter", "repeat_with"] => {
                count(self, 1);
                let t = self.fresh(VarKind::General);
                self.closure_arg(a0, vec![], t.clone());
                iter_of(t)
            }
            ["iter", "from_fn"] => {
                count(self, 1);
                let t = self.fresh(VarKind::General);
                self.closure_arg(a0, vec![], Ty::Option(Box::new(t.clone())));
                iter_of(t)
            }
            ["iter", "successors"] => {
                count(self, 2);
                let t = self.fresh(VarKind::General);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Option(Box::new(t.clone())));
                }
                self.closure_arg(a1, vec![t.clone()], Ty::Option(Box::new(t.clone())));
                iter_of(t)
            }
            // `f64::from_bits(u)` / `f32::from_bits(u)`
            [t @ ("f64" | "f32"), "from_bits"] => {
                count(self, 1);
                let f32 = *t == "f32";
                if let Some(a) = a0 {
                    self.check(a, &Ty::Int(if f32 { IntKind::U32 } else { IntKind::U64 }));
                }
                Ty::Float { f32 }
            }
            // `std::hint::black_box(x)`: `x`, opaque to the optimizer (there's none here).
            ["hint", "black_box"] => {
                count(self, 1);
                match a0 {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                }
            }
            // `mem::forget(x)`: `x` is moved and never dropped.
            ["mem", "forget"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                Ty::Unit
            }
            // `panic::catch_unwind(|| ..)`: `Ok` of the closure's value, or `Err` of the panic's
            // payload. (Also of `AssertUnwindSafe(|| ..)`, the unwind prelude's.)
            ["panic", "catch_unwind"] => {
                count(self, 1);
                let t = match a0.map(|a| self.infer(a, None)).map(|t| self.zonk(&t)) {
                    Some(Ty::Closure { ret, .. }) => *ret,
                    Some(Ty::Adt(n, args)) if n == "AssertUnwindSafe" => match args.first() {
                        Some(Ty::Closure { ret, .. }) => (**ret).clone(),
                        _ => Ty::Unknown,
                    },
                    _ => Ty::Unknown,
                };
                Ty::Result(Box::new(t), Box::new(Ty::Dyn("Any".into())))
            }
            // `drop(x)`: `x` is dropped now.
            ["drop"] | ["mem", "drop"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                Ty::Unit
            }
            // `size_of::<T>()` / `align_of::<T>()` / `size_of_val(&x)` as values (in a const
            // argument, `consteval` works them out): the type's width in bytes.
            ["mem", "size_of" | "align_of"] | ["size_of" | "align_of"] => {
                count(self, 0);
                USIZE
            }
            ["mem", "size_of_val" | "align_of_val"] | ["size_of_val" | "align_of_val"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                USIZE
            }
            ["mem", "swap"] => {
                count(self, 2);
                let t = self.fresh(VarKind::General);
                for a in args.iter().take(2) {
                    if let ast::Expr::RefExpr(r) = a
                        && let Some(inner) = r.expr()
                    {
                        self.check_mutable(&inner, Mutation::Borrow);
                    }
                    self.check(a, &t);
                }
                Ty::Unit
            }
            ["mem", "replace"] => {
                count(self, 2);
                let t = match a0 {
                    Some(a) => {
                        if let ast::Expr::RefExpr(r) = a
                            && let Some(inner) = r.expr()
                        {
                            self.check_mutable(&inner, Mutation::Borrow);
                        }
                        self.infer(a, None)
                    }
                    None => Ty::Unknown,
                };
                if let Some(a) = a1 {
                    self.check(a, &t);
                }
                t
            }
            ["mem", "take"] => {
                count(self, 1);
                let t = match a0 {
                    Some(a) => {
                        if let ast::Expr::RefExpr(r) = a
                            && let Some(inner) = r.expr()
                        {
                            self.check_mutable(&inner, Mutation::Borrow);
                        }
                        self.infer(a, None)
                    }
                    None => Ty::Unknown,
                };
                if !self.zonk(&t).has_default()
                    && !matches!(self.zonk(&t), Ty::Adt(..) | Ty::Var(_))
                {
                    let msg = format!(
                        "the trait bound `{}: Default` is not satisfied",
                        self.describe(&t)
                    );
                    self.error(c.syntax(), msg);
                }
                t
            }
            _ => return None,
        })
    }
}

/// The signed kind of an integer kind's width (`usize` → `isize`).
pub(super) fn signed_of(k: IntKind) -> IntKind {
    match k {
        IntKind::U8 => IntKind::I8,
        IntKind::U16 => IntKind::I16,
        IntKind::U32 => IntKind::I32,
        IntKind::U64 => IntKind::I64,
        IntKind::U128 => IntKind::I128,
        IntKind::Usize => IntKind::Isize,
        other => other,
    }
}
