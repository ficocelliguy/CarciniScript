//! More of std's surface: string searching, splitting and editing, integer arithmetic variants
//! (`checked_*`, `saturating_*`, `wrapping_*`, bit counts), `Vec` searching and rearranging,
//! `char::from_digit` and friends, and `std::mem::{swap, replace, take}`. A child module of
//! `typeck`.

use ra_ap_syntax::{AstNode, ast};

use super::{Checker, Mutation, USIZE, VarKind};
use crate::ty::{IntKind, Ty};

const U32: Ty = Ty::Int(IntKind::U32);

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
    /// A string pattern: `&str`, `char`, or a `char` predicate closure.
    fn str_pattern(&mut self, a: Option<&ast::Expr>) {
        let Some(a) = a else { return };
        let t = self.infer(a, None);
        match self.shallow(&t) {
            Ty::Str | Ty::Char | Ty::Unknown => {}
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
                    "expected a `&str`, `char` or `|c: char| -> bool` pattern, found `{}`",
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
            (Ty::Str, "to_ascii_uppercase" | "to_ascii_lowercase" | "to_owned") => {
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

            // ---- integers
            (Ty::Int(k), m2)
                if m2.starts_with("checked_")
                    || m2.starts_with("saturating_")
                    || m2.starts_with("wrapping_") =>
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
            (Ty::Vec(e), "split_at") => {
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
            (Ty::Vec(e), "is_sorted") => {
                count(self, 0);
                self.require_ord(m.syntax(), &e, "is_sorted");
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
        let count = |c2: &mut Self, n: usize| c2.arg_count(c.syntax(), "function", n, args.len());
        let a0 = args.first();
        let a1 = args.get(1);
        Some(match tail {
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
