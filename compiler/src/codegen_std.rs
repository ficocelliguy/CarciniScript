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
    fn arg_frags(&mut self, m: &ast::MethodCallExpr) -> Vec<Frag> {
        let args: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        args.iter()
            .map(|a| {
                let (f, t) = self.emit_expr(a, None);
                self.by_value(a, f, &t)
            })
            .collect()
    }

    /// `recv = <value>`, for `String` methods that change the string (JS strings are immutable).
    fn reassign(&mut self, m: &ast::MethodCallExpr, rf: &Frag, value: Frag) -> Frag {
        if !m.receiver().is_some_and(|r| is_pure(&strip_refs(&r))) {
            let what = m
                .name_ref()
                .map(|n| n.text().to_string())
                .unwrap_or_default();
            self.unsupported(
                m.syntax(),
                &format!("`String::{what}` on this expression"),
                1,
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
                if !m.receiver().is_some_and(|r| is_pure(&strip_refs(&r))) {
                    self.unsupported(
                        m.syntax(),
                        &format!("`String::{method}` on this expression"),
                        1,
                    );
                }
                let mut f = Frag::text("(($r) => (");
                f.append(rf.clone())
                    .push(" = $r[1], $r[0]))(")
                    .append(call)
                    .push(")");
                f
            }

            // ---- integers
            (Ty::Int(k), m2)
                if m2.starts_with("checked_")
                    || m2.starts_with("saturating_")
                    || m2.starts_with("wrapping_") =>
            {
                let (kind, op) = m2.split_once('_')?;
                let (min, max) = k.min_max();
                let a = self.arg_frags(m);
                let b = a.into_iter().next().unwrap_or_else(|| Frag::text("0"));
                let raw = match op {
                    "add" => Frag::text("(")
                        .append(rf.clone())
                        .push(" + ")
                        .append(b)
                        .push(")")
                        .clone(),
                    "sub" => Frag::text("(")
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
        let tail = match segs {
            ["std" | "core", rest @ ..] => rest,
            all => all,
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
            [t, "from"] if matches!(Ty::from_name(t), Ty::Int(_) | Ty::Float { .. }) => {
                emit(self, 0)
            }
            ["hint", "black_box"] => emit(self, 0),
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

fn int_bits(k: IntKind) -> u32 {
    match k {
        IntKind::I8 | IntKind::U8 => 8,
        IntKind::I16 | IntKind::U16 => 16,
        IntKind::I32 | IntKind::U32 => 32,
        _ => 64,
    }
}
