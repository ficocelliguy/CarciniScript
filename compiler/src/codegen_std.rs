//! Codegen for the std surface in `typeck_std`: string searching and editing, integer
//! arithmetic variants, `Vec` searching and rearranging, `char::from_digit` and friends, and
//! `std::mem::{swap, replace, take}`. A child module of `codegen`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

use super::{Codegen, Frag, default_value, is_pure, js_string, strip_refs};
use crate::ty::{IntKind, Ty};

impl Codegen<'_> {
    /// Whether the one argument of `x.wrapping_add(y)` on a BigInt kind is surely a BigInt:
    /// typed as the kind, or an integer literal (`big_arg` emits it with the `n` suffix).
    fn big_arg_ok(&self, m: &ast::MethodCallExpr, k: IntKind) -> bool {
        let Some(list) = m.arg_list() else {
            return false;
        };
        let mut args = list.args();
        let Some(x) = args.next() else { return false };
        if args.next().is_some() {
            return false;
        }
        matches!(&x, ast::Expr::Literal(l)
            if matches!(l.kind(), ast::LiteralKind::IntNumber(n) if n.value().is_ok()))
            || matches!(self.ty_of(x.syntax()), Some(Ty::Int(k2)) if k2 == k)
    }

    fn big_arg(&mut self, m: &ast::MethodCallExpr) -> Option<Frag> {
        let x = m.arg_list()?.args().next()?;
        if let ast::Expr::Literal(l) = &x
            && let ast::LiteralKind::IntNumber(n) = l.kind()
            && let Ok(v) = n.value()
        {
            return Some(Frag::text(format!("{v}n")));
        }
        let (f, t) = self.emit_expr(&x, None);
        Some(self.by_value(&x, f, &t))
    }

    fn arg_frags(&mut self, m: &ast::MethodCallExpr) -> Vec<Frag> {
        let args: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let on_str = m
            .receiver()
            .is_some_and(|r| self.ty_of(r.syntax()) == Some(Ty::Str));
        args.iter()
            .map(|a| {
                let (f, t) = self.emit_expr(a, None);
                let f = self.by_value(a, f, &t);
                if on_str { self.char_set_pattern(a, f) } else { f }
            })
            .collect()
    }

    /// A `[char]` string pattern (`s.trim_matches(&[' ', ','])`) as the predicate the runtime's
    /// pattern helpers take.
    pub(super) fn char_set_pattern(&self, a: &ast::Expr, f: Frag) -> Frag {
        match self.ty_of(a.syntax()) {
            Some(Ty::Vec(e)) if *e == Ty::Char => f.wrap("κσ.$charPat(", ")"),
            _ => f,
        }
    }

    /// `recv = <value>`, for `String` methods that change the string (JS strings are immutable).
    fn reassign(&mut self, m: &ast::MethodCallExpr, rf: &Frag, value: Frag) -> Frag {
        if !m.receiver().is_some_and(|r| self.place_receiver(&r)) {
            let what = m
                .name_ref()
                .map(|n| n.text().to_string())
                .unwrap_or_default();
            self.unsupported(
                m.syntax(),
                &format!("`String::{what}` on this expression"),
            );
        }
        let mut f = Frag::text("(");
        f.append(rf.clone())
            .push(" = ")
            .append(value)
            .push(", undefined)");
        f
    }

    /// Methods from `typeck_std`; `None` when `method` isn't one of them.
    pub(super) fn emit_std_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: &Frag,
        rt: &Ty,
        method: &str,
    ) -> Option<(Frag, Ty)> {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        let loc = js_string(&self.location_at(name_off));
        let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let helper = |name: &str, rest: Vec<Frag>| {
            let mut all = vec![rf.clone()];
            all.extend(rest);
            let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
            f.append(Frag::join(all, ", ")).push(")");
            f
        };
        let f = match (rt, method) {
            // ---- strings
            (Ty::Str, "find" | "rfind") => {
                let a = self.arg_frags(m);
                helper(
                    "strFind",
                    [a, vec![Frag::text((method == "rfind").to_string())]].concat(),
                )
            }
            (Ty::Str, "split_once" | "rsplit_once") => {
                let a = self.arg_frags(m);
                helper(
                    "splitOnce",
                    [a, vec![Frag::text((method == "rsplit_once").to_string())]].concat(),
                )
            }
            (Ty::Str, "strip_prefix") => helper("stripPrefix", self.arg_frags(m)),
            (Ty::Str, "strip_suffix") => helper("stripSuffix", self.arg_frags(m)),
            (Ty::Str, "trim_matches" | "trim_start_matches" | "trim_end_matches") => {
                let a = self.arg_frags(m);
                let (start, end) = match method {
                    "trim_matches" => (true, true),
                    "trim_start_matches" => (true, false),
                    _ => (false, true),
                };
                helper(
                    "trimMatches",
                    [a, vec![Frag::text(format!("{start}, {end}"))]].concat(),
                )
            }
            (Ty::Str, "contains") => helper("strContains", self.arg_frags(m)),
            (Ty::Str, "starts_with") => helper("strStartsWith", self.arg_frags(m)),
            (Ty::Str, "ends_with") => helper("strEndsWith", self.arg_frags(m)),
            (Ty::Str, "replace") => helper(
                "strReplace",
                [self.arg_frags(m), vec![Frag::text("Infinity")]].concat(),
            ),
            (Ty::Str, "replacen") => helper("strReplace", self.arg_frags(m)),
            (Ty::Str, "char_indices") => helper("charIndices", vec![]),
            (Ty::Str, "bytes") => helper("bytes", vec![]),
            (Ty::Str, "rsplit") => helper("rsplit", self.arg_frags(m)),
            (Ty::Str, "matches") => helper("strMatches", self.arg_frags(m)),
            (Ty::Str, "split_terminator") => helper("splitTerminator", self.arg_frags(m)),
            (Ty::Str, "splitn") => helper(
                "splitn",
                [self.arg_frags(m), vec![Frag::text("false")]].concat(),
            ),
            (Ty::Str, "rsplitn") => helper(
                "splitn",
                [self.arg_frags(m), vec![Frag::text("true")]].concat(),
            ),
            (Ty::Str, "to_ascii_uppercase") => helper("asciiUpper", vec![]),
            (Ty::Str, "to_ascii_lowercase") => helper("asciiLower", vec![]),
            (Ty::Str, "to_owned") => rf.clone(),
            (Ty::Str, "eq_ignore_ascii_case") => helper("eqIgnoreAsciiCase", self.arg_frags(m)),
            (Ty::Str, "is_char_boundary") => helper("isCharBoundary", self.arg_frags(m)),
            (Ty::Str, "insert" | "insert_str") => {
                let a = self.arg_frags(m);
                let v = helper("strInsert", [a, vec![Frag::text(loc.clone())]].concat());
                self.reassign(m, rf, v)
            }
            (Ty::Str, "truncate") => {
                let a = self.arg_frags(m);
                let v = helper("strTruncate", [a, vec![Frag::text(loc.clone())]].concat());
                self.reassign(m, rf, v)
            }
            (Ty::Str, "clear") => self.reassign(m, rf, Frag::text("\"\"")),
            (Ty::Str, "pop" | "remove") => {
                // `[removed, rest]`, then the receiver takes the rest.
                let a = self.arg_frags(m);
                let call = if method == "pop" {
                    helper("strPop", vec![])
                } else {
                    helper("strRemove", [a, vec![Frag::text(loc.clone())]].concat())
                };
                if !m.receiver().is_some_and(|r| self.place_receiver(&r)) {
                    self.unsupported(
                        m.syntax(),
                        &format!("`String::{method}` on this expression"),
                    );
                }
                let mut f = Frag::text("(($r) => (");
                f.append(rf.clone())
                    .push(" = $r[1], $r[0]))(")
                    .append(call)
                    .push(")");
                f
            }

            (Ty::Str | Ty::Vec(_) | Ty::Dyn(_), "as_ref" | "borrow")
            | (Ty::Vec(_), "as_mut" | "borrow_mut") => rf.clone(),
            (Ty::Int(_), "is_multiple_of") => helper("isMultipleOf", self.arg_frags(m)),
            // ---- integers
            // `h.wrapping_mul(k)` on a BigInt kind with a BigInt argument: inline, wrapped to
            // the width (hash loops; `κσ.$big` parses the method name per call).
            (Ty::Int(k), m2 @ ("wrapping_add" | "wrapping_sub" | "wrapping_mul"))
                if k.is_big() && self.big_arg_ok(m, *k) =>
            {
                let b = self.big_arg(m)?;
                let op = match m2 {
                    "wrapping_add" => " + ",
                    "wrapping_sub" => " - ",
                    _ => " * ",
                };
                let mut f = rf.clone();
                f.push(op).append(b.wrap("(", ")"));
                super::wrap_big(f, *k)
            }
            // BigInt kinds: one runtime dispatcher, exact at the type's width.
            (Ty::Int(k), m2)
                if k.is_big()
                    && (m2.starts_with("checked_")
                        || m2.starts_with("saturating_")
                        || m2.starts_with("wrapping_")
                        || matches!(
                            m2,
                            "abs_diff"
                                | "count_ones"
                                | "count_zeros"
                                | "leading_zeros"
                                | "trailing_zeros"
                                | "is_power_of_two"
                                | "div_euclid"
                                | "isqrt"
                                | "ilog2"
                                | "ilog10"
                                | "is_positive"
                                | "is_negative"
                        )) =>
            {
                let mut all = vec![
                    Frag::text(js_string(m2)),
                    rf.clone(),
                    Frag::text(format!("{}, {}", k.bits(), k.is_signed())),
                ];
                all.extend(self.arg_frags(m));
                let mut f = Frag::mapped(name_off, "κσ.$big(");
                f.append(Frag::join(all, ", ")).push(")");
                f
            }
            (Ty::Int(k), m2)
                if m2.starts_with("checked_")
                    || m2.starts_with("saturating_")
                    || m2.starts_with("wrapping_") =>
            {
                let (kind, op) = m2.split_once('_')?;
                let (min, max) = k.min_max();
                let a = self.arg_frags(m);
                // (Parenthesized: `x.wrapping_sub(y + 1)` is `x - (y + 1)`.)
                let b = a
                    .into_iter()
                    .next()
                    .map_or_else(|| Frag::text("0"), |b| b.wrap("(", ")"));
                let raw = match op {
                    "add" | "add_signed" => Frag::text("(")
                        .append(rf.clone())
                        .push(" + ")
                        .append(b)
                        .push(")")
                        .clone(),
                    "sub" | "sub_signed" => Frag::text("(")
                        .append(rf.clone())
                        .push(" - ")
                        .append(b)
                        .push(")")
                        .clone(),
                    "mul" => Frag::text("(")
                        .append(rf.clone())
                        .push(" * ")
                        .append(b)
                        .push(")")
                        .clone(),
                    "div" => Frag::text("κσ.$checkedDiv(")
                        .append(rf.clone())
                        .push(", ")
                        .append(b)
                        .push(")")
                        .clone(),
                    "rem" => Frag::text("κσ.$checkedRem(")
                        .append(rf.clone())
                        .push(", ")
                        .append(b)
                        .push(")")
                        .clone(),
                    "pow" => Frag::text("(")
                        .append(rf.clone())
                        .push(" ** ")
                        .append(b)
                        .push(")")
                        .clone(),
                    "neg" => Frag::text("(-").append(rf.clone()).push(")").clone(),
                    "abs" => Frag::text("Math.abs(").append(rf.clone()).push(")").clone(),
                    _ => return None,
                };
                match kind {
                    "checked" => Frag::text("κσ.$checked(")
                        .append(raw)
                        .push(format!(", {min}, {max})"))
                        .clone(),
                    "saturating" => Frag::text("κσ.$saturate(")
                        .append(raw)
                        .push(format!(", {min}, {max})"))
                        .clone(),
                    _ => match k.wrap_parts() {
                        // Products of 32-bit values can lose low bits in a double; `Math.imul` doesn't.
                        Some((pre, post)) if op == "mul" && k.is_narrow() => {
                            let args: Vec<ast::Expr> =
                                m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
                            let bf = args
                                .first()
                                .map(|x| self.emit_expr(x, None).0)
                                .unwrap_or_else(|| Frag::text("0"));
                            let mut f = Frag::text(format!("{pre}Math.imul("));
                            f.append(rf.clone())
                                .push(", ")
                                .append(bf)
                                .push(format!("){post}"));
                            f
                        }
                        Some((pre, post)) => raw.wrap(pre, post),
                        None => raw,
                    },
                }
            }
            (Ty::Int(_), "abs_diff") => {
                let b = self
                    .arg_frags(m)
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| Frag::text("0"));
                let mut f = Frag::text("Math.abs(");
                f.append(rf.clone()).push(" - ").append(b).push(")");
                f
            }
            (Ty::Int(k), "count_ones" | "count_zeros" | "leading_zeros" | "trailing_zeros") => {
                let bits = int_bits(*k);
                helper(
                    match method {
                        "count_ones" => "countOnes",
                        "count_zeros" => "countZeros",
                        "leading_zeros" => "leadingZeros",
                        _ => "trailingZeros",
                    },
                    vec![Frag::text(bits.to_string())],
                )
            }
            (Ty::Int(_), "is_power_of_two") => helper("isPowerOfTwo", vec![]),
            (Ty::Float { .. }, "total_cmp") => helper("totalCmp", self.arg_frags(m)),
            (Ty::Int(_), "div_euclid") => helper("divEuclid", self.arg_frags(m)),
            (Ty::Int(_), "isqrt") => helper("isqrt", vec![]),
            (Ty::Int(_), "ilog2") => helper("ilog", vec![Frag::text("2")]),
            (Ty::Int(_), "ilog10") => helper("ilog", vec![Frag::text("10")]),
            (Ty::Int(_), "is_positive") => rf.clone().wrap("(", " > 0)"),
            (Ty::Int(_), "is_negative") => rf.clone().wrap("(", " < 0)"),
            (Ty::Float { f32 }, "to_bits") => helper(
                "transmute",
                vec![Frag::text(if *f32 {
                    "\"f32\", \"u32\""
                } else {
                    "\"f64\", \"u64\""
                })],
            ),
            (
                Ty::Float { f32 },
                m2 @ ("to_radians" | "to_degrees" | "recip" | "fract" | "exp2"),
            ) => {
                let f = match m2 {
                    // As in std: `x * (PI / 180)`, `x * (180 / PI)`.
                    "to_radians" => rf.clone().wrap("(", " * (Math.PI / 180))"),
                    "to_degrees" if *f32 => rf
                        .clone()
                        .wrap("(", " * 57.2957795130823208767981548141051703)"),
                    "to_degrees" => rf.clone().wrap("(", " * (180 / Math.PI))"),
                    "recip" => rf.clone().wrap("(1 / ", ")"),
                    "fract" => rf.clone().wrap("κσ.$fract(", ")"),
                    _ => rf.clone().wrap("(2 ** ", ")"),
                };
                if *f32 { f.wrap("Math.fround(", ")") } else { f }
            }
            (Ty::Float { .. }, "signum") => helper("fsignum", vec![]),
            (
                Ty::Float { .. },
                m2 @ ("exp_m1" | "ln_1p" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh" | "atanh"),
            ) => {
                let js = match m2 {
                    "exp_m1" => "expm1",
                    "ln_1p" => "log1p",
                    other => other,
                };
                rf.clone().wrap(&format!("Math.{js}("), ")")
            }
            (Ty::Float { .. }, "hypot") => {
                let mut f = Frag::text("Math.hypot(");
                f.append(rf.clone())
                    .push(", ")
                    .append(Frag::join(self.arg_frags(m), ", "))
                    .push(")");
                f
            }
            (Ty::Float { .. }, "rem_euclid") => helper("fremEuclid", self.arg_frags(m)),
            (Ty::Float { .. }, "div_euclid") => helper("fdivEuclid", self.arg_frags(m)),
            (Ty::Float { .. }, "copysign") => helper("copysign", self.arg_frags(m)),
            (Ty::Float { .. }, "log") => {
                let mut f = Frag::text("(Math.log(");
                f.append(rf.clone())
                    .push(") / Math.log(")
                    .append(Frag::join(self.arg_frags(m), ", "))
                    .push("))");
                f
            }
            (Ty::Float { .. }, "abs_sub") => {
                let mut f = Frag::text("Math.max(");
                f.append(rf.clone())
                    .push(" - ")
                    .append(Frag::join(self.arg_frags(m), ", "))
                    .push(", 0)");
                f
            }
            (Ty::Float { .. }, "mul_add") => {
                let mut all = self.arg_frags(m);
                all.resize(2, Frag::text("0"));
                let mut f = rf.clone().wrap("(", " * ");
                f.append(all[0].clone())
                    .push(" + ")
                    .append(all[1].clone())
                    .push(")");
                f
            }
            (Ty::Float { .. }, "is_sign_negative") => helper("isSignNegative", vec![]),
            (Ty::Float { .. }, "is_sign_positive") => {
                helper("isSignNegative", vec![]).wrap("!", "")
            }
            (Ty::Float { .. }, "is_infinite") => rf.clone().wrap("(Math.abs(", ") === Infinity)"),
            (Ty::Float { .. }, "is_normal") => helper("isNormal", vec![]),

            // ---- Vec
            (Ty::Vec(_), "binary_search") => {
                let a = self.arg_frags(m);
                let mut probe = Frag::text("(x) => κσ.$cmp(x, ");
                probe
                    .append(
                        a.into_iter()
                            .next()
                            .unwrap_or_else(|| Frag::text("undefined")),
                    )
                    .push(")");
                helper("binarySearchBy", vec![probe])
            }
            (Ty::Vec(_), "binary_search_by") => helper("binarySearchBy", self.arg_frags(m)),
            (Ty::Vec(_), "binary_search_by_key") => {
                let mut a = self.arg_frags(m).into_iter();
                let key = a.next().unwrap_or_else(|| Frag::text("undefined"));
                let f = a.next().unwrap_or_else(|| Frag::text("undefined"));
                let mut probe = Frag::text("((k, f) => (x) => κσ.$cmp(f(x), k))(");
                probe.append(key).push(", ").append(f).push(")");
                helper("binarySearchBy", vec![probe])
            }
            (Ty::Vec(_), "as_slice" | "as_mut_slice") => rf.clone(),
            (Ty::Vec(_), "split_first") => helper("splitFirst", vec![]),
            (Ty::Vec(_), "split_last") => helper("splitLast", vec![]),
            (Ty::Vec(_), "split_at") => helper(
                "splitAt",
                [self.arg_frags(m), vec![Frag::text(loc.clone())]].concat(),
            ),
            // Two views of `v`, so writes through either reach it.
            (Ty::Vec(_), "split_at_mut") => helper(
                "splitAtMut",
                [self.arg_frags(m), vec![Frag::text(loc.clone())]].concat(),
            ),
            (Ty::Vec(_), "starts_with") => helper("vecStartsWith", self.arg_frags(m)),
            (Ty::Vec(_), "ends_with") => helper("vecEndsWith", self.arg_frags(m)),
            (Ty::Vec(_), "repeat") => helper("vecRepeatAll", self.arg_frags(m)),
            (Ty::Vec(_), "is_sorted") => helper("isSorted", vec![]),
            (Ty::Vec(_), "drain") => {
                use ra_ap_syntax::ast::{RangeItem, RangeOp};
                let Some(ast::Expr::RangeExpr(r)) = m.arg_list().and_then(|a| a.args().next())
                else {
                    return None;
                };
                let lo = r
                    .start()
                    .map(|e| self.emit_expr(&e, None).0)
                    .unwrap_or_else(|| Frag::text("0"));
                let hi = r
                    .end()
                    .map(|e| self.emit_expr(&e, None).0)
                    .unwrap_or_else(|| Frag::text("null"));
                let incl = r.op_kind() == Some(RangeOp::Inclusive);
                helper("drain", vec![lo, hi, Frag::text(format!("{incl}, {loc}"))])
            }
            (Ty::Vec(_), "split_off") => helper(
                "splitOff",
                [self.arg_frags(m), vec![Frag::text(loc.clone())]].concat(),
            ),
            (Ty::Vec(_), "append") => helper("append", self.arg_frags(m)),
            (Ty::Vec(_), "fill") => helper("fill", self.arg_frags(m)),
            (Ty::Vec(_), "copy_from_slice" | "clone_from_slice") => helper(
                "copyFromSlice",
                [self.arg_frags(m), vec![Frag::text(loc.clone())]].concat(),
            ),
            (Ty::Vec(_), "rotate_left") => helper(
                "rotate",
                [self.arg_frags(m), vec![Frag::text(format!("true, {loc}"))]].concat(),
            ),
            (Ty::Vec(_), "rotate_right") => helper(
                "rotate",
                [self.arg_frags(m), vec![Frag::text(format!("false, {loc}"))]].concat(),
            ),
            (Ty::Vec(_), "swap_remove") => helper(
                "swapRemove",
                [self.arg_frags(m), vec![Frag::text(loc.clone())]].concat(),
            ),
            _ => return None,
        };
        Some((f, ty))
    }

    /// `char::from_digit(..)`, `String::with_capacity(..)`, `u64::from(x)`, `std::mem::*`.
    pub(super) fn emit_std_call(&mut self, c: &ast::CallExpr, segs: &[&str]) -> Option<(Frag, Ty)> {
        let tail = crate::ty::std_tail(segs);
        let tail = match tail {
            ["rc" | "sync", rest @ ..] => rest,
            t => t,
        };
        let start = self.off(c.syntax());
        let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        let args: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let emit = |cg: &mut Self, i: usize| -> Frag {
            match args.get(i) {
                Some(a) => {
                    let (f, t) = cg.emit_expr(a, None);
                    cg.by_value(a, f, &t)
                }
                None => Frag::text("undefined"),
            }
        };
        let call = |name: &str, parts: Vec<Frag>| {
            let mut f = Frag::mapped(start, format!("κσ.${name}("));
            f.append(Frag::join(parts, ", ")).push(")");
            f
        };
        let f = match tail {
            ["env", "args"] => call("envArgs", vec![]),
            ["process", "exit"] => {
                let code = emit(self, 0);
                call("exit", vec![code])
            }
            ["char", "from_digit"] => {
                let (d, r) = (emit(self, 0), emit(self, 1));
                call("fromDigit", vec![d, r])
            }
            ["char", "from_u32"] => {
                let n = emit(self, 0);
                call("fromU32", vec![n])
            }
            ["char", "from"] => {
                let n = emit(self, 0);
                Frag::mapped(start, "String.fromCharCode(")
                    .append(n)
                    .push(")")
                    .clone()
            }
            ["String", "with_capacity"] => {
                let _ = emit(self, 0);
                Frag::mapped(start, "\"\"")
            }
            [t, "from_str_radix"] if matches!(Ty::from_name(t), Ty::Int(k) if k.is_big()) => {
                let (s, r) = (emit(self, 0), emit(self, 1));
                let k = Ty::from_name(t).int_kind().unwrap_or(IntKind::I128);
                call(
                    "fromStrRadixBig",
                    vec![s, r, Frag::text(format!("{}, {}", k.is_signed(), k.bits()))],
                )
            }
            [t, "from_str_radix"] if matches!(Ty::from_name(t), Ty::Int(_)) => {
                let (s, r) = (emit(self, 0), emit(self, 1));
                let k = Ty::from_name(t).int_kind().unwrap_or(IntKind::I32);
                let (min, max) = k.min_max();
                call(
                    "fromStrRadix",
                    vec![s, r, Frag::text(format!("{min}, {max}"))],
                )
            }
            ["str" | "String", "from_utf8"] => {
                let b = emit(self, 0);
                call("fromUtf8", vec![b])
            }
            ["String", "from_utf8_lossy"] => {
                let b = emit(self, 0);
                call("fromUtf8Lossy", vec![b])
            }
            // `u128::from(x)`: a BigInt from a smaller kind.
            [t, "from"] if matches!(Ty::from_name(t), Ty::Int(k) if k.is_big()) => {
                let k = Ty::from_name(t).int_kind().unwrap_or(IntKind::I128);
                let x = emit(self, 0);
                let mut f = Frag::text("κσ.$to128(");
                f.append(x)
                    .push(format!(", {}, {})", k.is_signed(), k.bits()));
                f
            }
            [t, "from"] if matches!(Ty::from_name(t), Ty::Int(_) | Ty::Float { .. }) => {
                emit(self, 0)
            }
            ["hint", "black_box"] => emit(self, 0),
            ["Rc" | "Arc", "downgrade"] => self.emit_expr(args.first()?, None).0,
            ["Rc" | "Arc", "default"] => Frag::mapped(
                start,
                super::default_value(&ty).unwrap_or_else(|| "undefined".into()),
            ),
            ["Weak", "new"] => Frag::mapped(start, "null"),
            // (An error from the type checker.)
            ["Rc" | "Arc", "strong_count" | "weak_count"] => Frag::mapped(start, "0"),
            ["Rc" | "Arc", "ptr_eq"] => {
                let a = self.emit_expr(args.first()?, None).0;
                let b = self.emit_expr(args.get(1)?, None).0;
                let mut f = Frag::mapped(start, "(");
                f.append(a).push(" === ").append(b).push(")");
                f
            }
            ["DefaultHasher", "new" | "default"] => Frag::mapped(start, "new κσ.Hasher()"),
            ["char", "try_from"] => call("tryChar", vec![emit(self, 0)]),
            [t, "try_from"] if matches!(Ty::from_name(t), Ty::Int(_)) => {
                let Ty::Int(k) = Ty::from_name(t) else {
                    return None;
                };
                let v = emit(self, 0);
                call(
                    "tryInt",
                    vec![
                        v,
                        Frag::text(format!("{}, {}, {}", k.bits(), k.is_signed(), k.is_big())),
                    ],
                )
            }
            [t, "default"] if Ty::from_name(t) != Ty::Unknown => Frag::mapped(
                start,
                default_value(&Ty::from_name(t)).unwrap_or_else(|| "undefined".into()),
            ),
            ["iter", "repeat"] => call("iterRepeat", vec![emit(self, 0)]),
            ["iter", "once"] => call("iterOnce", vec![emit(self, 0)]),
            ["iter", "empty"] => call("iterEmpty", vec![]),
            ["iter", "repeat_with"] => call("repeatWith", vec![emit(self, 0)]),
            ["iter", "repeat_n"] => {
                let (x, n) = (emit(self, 0), emit(self, 1));
                call("itertools.repeatN", vec![x, n])
            }
            ["iter", "from_fn"] => call("fromFn", vec![emit(self, 0)]),
            ["iter", "successors"] => call("successors", vec![emit(self, 0), emit(self, 1)]),
            [t @ ("f64" | "f32"), "from_bits"] => {
                let names = if *t == "f32" {
                    "\"u32\", \"f32\""
                } else {
                    "\"u64\", \"f64\""
                };
                call("transmute", vec![emit(self, 0), Frag::text(names)])
            }
            // Raw pointers: `null`, identity, and cells for numbers (`[v]`).
            ["ptr", "null" | "null_mut"] => Frag::mapped(start, "null"),
            ["ptr", "eq" | "addr_eq"] => {
                let a = self.emit_expr(args.first()?, None).0;
                let b = self.emit_expr(args.get(1)?, None).0;
                let mut f = Frag::mapped(start, "(");
                f.append(a).push(" === ").append(b).push(")");
                f
            }
            ["ptr", "read"] => {
                let (a, t) = self.emit_expr(args.first()?, None);
                if crate::typeck::ptr_cellable(&t) {
                    a.wrap("(", ")[0]")
                } else {
                    a
                }
            }
            ["Box", "into_raw"] if crate::typeck::ptr_cellable(&ty) => emit(self, 0).wrap("[", "]"),
            ["Box", "into_raw" | "from_raw" | "leak"] => emit(self, 0),
            ["mem", "transmute"] => {
                let a = args.first()?;
                let (f, t) = self.emit_expr(a, None);
                let from = self.ty_of(a.syntax()).unwrap_or(t).default_lit();
                let (Some(from), Some(to)) = (
                    crate::typeck::transmute_name(&from),
                    crate::typeck::transmute_name(&ty),
                ) else {
                    return Some((f, ty));
                };
                if from == to {
                    f
                } else {
                    call(
                        "transmute",
                        vec![f, Frag::text(format!("\"{from}\", \"{to}\""))],
                    )
                }
            }
            ["drop"] | ["mem", "drop"] => call("drop", vec![emit(self, 0)]),
            // `size_of::<u32>()` / `size_of_val(&x)`: the width rustc gives the type (a JS value has
            // none of its own).
            ["mem", "size_of" | "align_of" | "size_of_val" | "align_of_val"]
            | ["size_of" | "align_of" | "size_of_val" | "align_of_val"] => {
                let t = if tail.last().is_some_and(|f| f.ends_with("_val")) {
                    args.first().and_then(|a| self.ty_of(strip_refs(a).syntax()))
                } else {
                    turbofish_type(c).map(|t| self.lower_type(&t))
                };
                match t.as_ref().and_then(byte_width) {
                    Some(n) => Frag::mapped(start, n.to_string()),
                    None => {
                        let what = format!(
                            "`{}` of `{}`",
                            tail.last().copied().unwrap_or("size_of"),
                            t.map(|t| t.describe()).unwrap_or_else(|| "this type".into())
                        );
                        self.unsupported(c.syntax(), &what);
                        Frag::text("0")
                    }
                }
            }
            ["mem", "swap"] => {
                let (a, b) = (args.first()?.clone(), args.get(1)?.clone());
                let (af, _) = self.emit_expr(&strip_refs(&a), None);
                let (bf, _) = self.emit_expr(&strip_refs(&b), None);
                if is_place_ref(&a) && is_place_ref(&b) {
                    // `[a, b] = [b, a]`
                    let mut f = Frag::mapped(start, "([");
                    f.append(af.clone())
                        .push(", ")
                        .append(bf.clone())
                        .push("] = [")
                        .append(bf)
                        .push(", ")
                        .append(af)
                        .push("], undefined)");
                    f
                } else {
                    // `&mut` parameters: swap the objects' contents.
                    call("swapContents", vec![af, bf])
                }
            }
            ["mem", "replace" | "take"] => {
                let a = args.first()?.clone();
                let (af, _) = self.emit_expr(&strip_refs(&a), None);
                let new = if tail[1] == "take" {
                    Frag::text(default_value(&ty).unwrap_or_else(|| "undefined".into()))
                } else {
                    emit(self, 1)
                };
                if is_place_ref(&a) {
                    // `(($old, $new) => (a = $new, $old))(a, value)`
                    let mut f = Frag::mapped(start, "(($o, $n) => (");
                    f.append(af.clone())
                        .push(" = $n, $o))(")
                        .append(af)
                        .push(", ")
                        .append(new)
                        .push(")");
                    f
                } else {
                    call("replaceContents", vec![af, new])
                }
            }
            _ => return None,
        };
        Some((f, ty))
    }
}

/// `&mut x`, `&mut v[i]`, `&mut self.f`: a place JS can assign.
fn is_place_ref(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::RefExpr(r) => r.expr().is_some_and(|i| is_pure(&i)),
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| is_place_ref(&i)),
        _ => false,
    }
}

/// The type a call names with a turbofish (`size_of::<u32>()`).
fn turbofish_type(c: &ast::CallExpr) -> Option<ast::Type> {
    use ra_ap_syntax::ast::HasGenericArgs;
    let ast::Expr::PathExpr(p) = c.expr()? else {
        return None;
    };
    p.path()?
        .segment()?
        .generic_arg_list()?
        .generic_args()
        .find_map(|a| match a {
            ast::GenericArg::TypeArg(t) => t.ty(),
            _ => None,
        })
}

/// `size_of::<T>()` for the types whose width is fixed in rustc: the number kinds, `bool`,
/// `char` and `()`.
fn byte_width(t: &Ty) -> Option<u32> {
    Some(match t {
        Ty::Int(k) => k.bits() / 8,
        Ty::IntLit => 4,
        Ty::Float { f32: true } => 4,
        Ty::Float { f32: false } => 8,
        Ty::Bool => 1,
        Ty::Char => 4,
        Ty::Unit => 0,
        _ => return None,
    })
}

fn int_bits(k: IntKind) -> u32 {
    match k {
        IntKind::I8 | IntKind::U8 => 8,
        IntKind::I16 | IntKind::U16 => 16,
        IntKind::I32 | IntKind::U32 => 32,
        _ => 64,
    }
}
