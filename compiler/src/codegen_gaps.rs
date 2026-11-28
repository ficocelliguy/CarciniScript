//! Codegen for the std gaps in `typeck_gaps`: item-level `compile_error!`, and the methods and
//! associated functions listed there. A child module of `codegen`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

use super::adt_gen::prop;
use super::{Codegen, Frag, format_value, js_string};
use crate::macros;
use crate::ty::{IntKind, Ty};

impl Codegen<'_> {
    /// `compile_error!("..");` as an item: its message, unless it's under a `#[cfg]` that's off.
    pub(super) fn item_compile_error(&mut self, mc: &ast::MacroCall) {
        if macros::under_false_cfg(mc.syntax()) {
            return;
        }
        let args = macros::split_args(self.src, self.base, mc);
        let msg = args
            .first()
            .and_then(macros::format_string)
            .unwrap_or_else(|| "compile_error! invoked".into());
        self.error(mc.syntax(), msg);
    }

    /// `bag.iter()` where `iter` is your own method (a custom iterator, say): the loop goes over
    /// what it returns, not over `bag`.
    pub(super) fn own_iter_call(&self, e: &ast::Expr) -> bool {
        let ast::Expr::MethodCallExpr(m) = e else {
            return false;
        };
        let name = m
            .name_ref()
            .map(|n| n.text().to_string())
            .unwrap_or_default();
        let recv = m
            .receiver()
            .map(|r| self.ty_of(r.syntax()).unwrap_or(Ty::Unknown));
        matches!(recv, Some(Ty::Adt(n, _)) if self.types.adts.method(&n, &name).is_some())
    }

    fn gap_arg(&mut self, a: Option<&ast::Expr>) -> Frag {
        match a {
            Some(a) => {
                let (f, t) = self.emit_expr(a, None);
                self.by_value(a, f, &t)
            }
            None => Frag::text("undefined"),
        }
    }

    /// A range argument's `lo, hi, inclusive` (`hi` is `null` for `lo..`).
    pub(super) fn gap_range(&mut self, a: Option<&ast::Expr>) -> Option<Frag> {
        use ra_ap_syntax::ast::{RangeItem, RangeOp};
        let Some(ast::Expr::RangeExpr(r)) = a else {
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
        let mut f = lo;
        f.push(", ").append(hi).push(format!(", {incl}"));
        Some(f)
    }

    /// `x.fmt(f)` / `fmt::Display::fmt(&x, f)`: `x` formatted with `f`'s spec.
    pub(super) fn fmt_delegate(&mut self, x: Frag, ty: &Ty, f: Frag, debug: bool) -> Frag {
        let spec = crate::fmt::Spec {
            debug,
            ..Default::default()
        };
        let ty = ty.view();
        let base = format_value(Frag::text("v"), &ty, &spec, None, None);
        let kind = match &ty {
            Ty::Float { .. } => "float",
            Ty::Int(_) | Ty::IntLit => "int",
            Ty::Str => "str",
            _ => "other",
        };
        let mut out = Frag::text("κσ.$fmtDelegate(");
        out.append(f)
            .push(", ")
            .append(x)
            .push(", (v) => ")
            .append(base)
            .push(format!(", \"{kind}\", {debug})"));
        out
    }

    /// Methods from `typeck_gaps`; `None` when `method` isn't one of them.
    pub(super) fn emit_gaps_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: &Frag,
        rt: &Ty,
        method: &str,
    ) -> Option<(Frag, Ty)> {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        let loc = js_string(&self.location_at(name_off));
        let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let args: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let a0 = args.first();
        let a1 = args.get(1);
        let helper = |name: &str, rest: Vec<Frag>| {
            let mut all = vec![rf.clone()];
            all.extend(rest);
            let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
            f.append(Frag::join(all, ", ")).push(")");
            f
        };
        let f = match (rt, method) {
            (t, "fmt") if args.len() == 1 && !matches!(t, Ty::Formatter) => {
                let f = self.gap_arg(a0);
                let (has_display, has_debug) = match t {
                    Ty::Adt(n, _) => match self.types.adts.defs.get(n) {
                        Some(d) => (
                            d.display,
                            d.derives("Debug") || d.manual.iter().any(|x| x == "Debug"),
                        ),
                        None => (true, true),
                    },
                    _ => (true, true),
                };
                let debug =
                    crate::typeck::delegated_fmt_is_debug(m.syntax(), has_display, has_debug);
                let t = t.clone();
                self.fmt_delegate(rf.clone(), &t, f, debug)
            }
            // ---- Futures (`Pin` is erased): `fut.as_mut()`, and `fut.poll(cx)` on an async fn's
            // future (a promise) or a hand-written one passed as an `F: Future`.
            (Ty::Future(_), m2) if crate::typeck::pin_identity(m2) && args.is_empty() => rf.clone(),
            (Ty::Adt(n, _), m2)
                if crate::typeck::pin_identity(m2)
                    && args.is_empty()
                    && self.types.adts.method(n, m2).is_none() =>
            {
                rf.clone()
            }
            (Ty::Future(_), "poll") => {
                let cx = self.gap_arg(a0);
                helper(
                    "poll",
                    vec![cx, Frag::text(format!("\"{}\", TaskPoll", prop("poll")))],
                )
            }
            // ---- `OsStr` / `OsString`, `str::get(range)`
            (Ty::Str, "to_str") => rf.clone(),
            (
                Ty::Str,
                "to_string_lossy"
                | "to_os_string"
                | "as_os_str"
                | "into_os_string"
                | "into_boxed_str"
                | "as_encoded_bytes_str",
            ) => rf.clone(),
            (Ty::Str, "into_string") => helper("okOf", vec![]),
            (Ty::Str, "get") => {
                let r = self.gap_range(a0)?;
                helper("strGet", vec![r])
            }
            // ---- Vec and slices
            (Ty::Vec(_), "into_boxed_slice" | "into_vec") => rf.clone(),
            (Ty::Vec(_), "capacity") => {
                let mut f = rf.clone();
                f.push(".length");
                f
            }
            (Ty::Str, "capacity") => helper("utf8len", vec![]),
            (Ty::Vec(_), "reserve" | "reserve_exact" | "shrink_to") => {
                let a = self.gap_arg(a0);
                let mut f = Frag::text("void (");
                f.append(a).push(")");
                f
            }
            (Ty::Vec(_), "shrink_to_fit") => Frag::text("undefined"),
            (Ty::Vec(_), "resize") => {
                let (n, x) = (self.gap_arg(a0), self.gap_arg(a1));
                helper("resize", vec![n, x])
            }
            (Ty::Vec(_), "resize_with") => {
                let (n, x) = (self.gap_arg(a0), self.gap_arg(a1));
                helper("resizeWith", vec![n, x])
            }
            (Ty::Vec(_), "chunks_mut" | "chunks_exact_mut" | "rchunks_mut") => {
                let n = self.gap_arg(a0);
                let how = match method {
                    "chunks_mut" => "false, false",
                    "chunks_exact_mut" => "true, false",
                    _ => "false, true",
                };
                helper("chunksMut", vec![n, Frag::text(format!("{how}, {loc}"))])
            }
            (Ty::Vec(_), "copy_within") => {
                let r = self.gap_range(a0)?;
                let d = self.gap_arg(a1);
                helper("copyWithin", vec![r, d, Frag::text(loc)])
            }
            (Ty::Vec(_), "retain_mut") => {
                // The closure's `x` is `&mut T`: alias a cell, so writes stick.
                let f = match a0 {
                    Some(ast::Expr::ClosureExpr(c)) => {
                        let s = self.temp();
                        self.emit_closure_aliased(c, vec![s.clone()], vec![vec![format!("{s}[0]")]])
                    }
                    other => {
                        let f = self.gap_arg(other);
                        let s = self.temp();
                        let mut g = Frag::text(format!("({s}) => ("));
                        g.append(f).push(format!(")({s}[0])"));
                        g
                    }
                };
                helper("retainMut", vec![f])
            }
            (Ty::Vec(_), "splice") => {
                let r = self.gap_range(a0)?;
                let src = self.iterable_arg(a1.cloned());
                helper("splice", vec![r, src, Frag::text(loc)])
            }
            (Ty::Vec(_), "map") => {
                let f = self.gap_arg(a0);
                let mut g = rf.clone();
                g.push(".map((x) => (").append(f).push(")(x))");
                g
            }
            (Ty::Vec(_), "strip_prefix") => {
                helper("vecStrip", vec![self.gap_arg(a0), Frag::text("false")])
            }
            (Ty::Vec(_), "strip_suffix") => {
                helper("vecStrip", vec![self.gap_arg(a0), Frag::text("true")])
            }
            (Ty::Vec(_), "split_at_checked" | "split_at_mut_checked") => {
                let mid = self.gap_arg(a0);
                let mutable = method == "split_at_mut_checked";
                helper("splitAtChecked", vec![mid, Frag::text(format!("{mutable}, {loc}"))])
            }
            (Ty::Vec(_), "split_first_mut" | "split_last_mut") => {
                let last = method == "split_last_mut";
                helper("splitEndMut", vec![Frag::text(format!("{last}, {loc}"))])
            }
            (Ty::Vec(_), "partition_point") => helper("partitionPoint", vec![self.gap_arg(a0)]),
            (Ty::Vec(_), "is_sorted_by") => helper("sliceIsSortedBy", vec![self.gap_arg(a0)]),
            (Ty::Vec(_), "is_sorted_by_key") => {
                helper("sliceIsSortedByKey", vec![self.gap_arg(a0)])
            }
            (Ty::Vec(_), "dedup_by") => helper("dedupBy", vec![self.gap_arg(a0)]),
            (Ty::Vec(_), "split" | "split_inclusive" | "rsplit") => {
                let mode = match method {
                    "split" => "0",
                    "split_inclusive" => "1",
                    _ => "2",
                };
                helper("sliceSplit", vec![self.gap_arg(a0), Frag::text(mode)])
            }
            (Ty::Vec(_), "try_reserve" | "try_reserve_exact") => {
                let mut f = Frag::text("(void (");
                f.append(self.gap_arg(a0)).push("), κσ.$okOf(undefined))");
                f
            }
            (Ty::Vec(_), "first_chunk" | "last_chunk" | "first_chunk_mut" | "last_chunk_mut") => {
                use ra_ap_syntax::ast::HasGenericArgs;
                let n = match m.generic_arg_list()?.generic_args().next()? {
                    ast::GenericArg::ConstArg(c) => self.gap_arg(c.expr().as_ref()),
                    ast::GenericArg::TypeArg(t) => Frag::text(t.syntax().text().to_string()),
                    _ => return None,
                };
                let last = method.starts_with("last");
                let mutable = method.ends_with("_mut");
                helper("firstChunk", vec![n, Frag::text(format!("{last}, {mutable}, {loc}"))])
            }
            (Ty::Vec(_), "is_ascii") => helper("isAscii", vec![]),
            (Ty::Vec(_), "eq_ignore_ascii_case") => {
                helper("bytesEqIgnoreCase", vec![self.gap_arg(a0)])
            }
            // ---- char
            (Ty::Char, "len_utf8") => helper("utf8len", vec![]),
            (Ty::Char, "len_utf16") => {
                let mut f = rf.clone();
                f.push(".length");
                f
            }
            (Ty::Char, "encode_utf8") => helper("encodeUtf8", vec![self.gap_arg(a0)]),
            (Ty::Char, "escape_debug") => helper("escapeChar", vec![Frag::text("\"debug\"")]),
            (Ty::Char, "escape_default") => helper("escapeChar", vec![Frag::text("\"default\"")]),
            (Ty::Char, "escape_unicode") => helper("escapeChar", vec![Frag::text("\"unicode\"")]),
            // ---- integers
            (Ty::Int(k), "to_le_bytes" | "to_be_bytes" | "to_ne_bytes") => {
                let little = method != "to_be_bytes";
                helper(
                    "toBytes",
                    vec![Frag::text(format!("{}, {little}", k.bits()))],
                )
            }
            (Ty::Float { f32 }, "to_le_bytes" | "to_be_bytes" | "to_ne_bytes") => {
                let little = method != "to_be_bytes";
                helper("floatToBytes", vec![Frag::text(format!("{f32}, {little}"))])
            }
            (Ty::Int(k), "unsigned_abs") => {
                let _ = k;
                helper("uabs", vec![])
            }
            (Ty::Int(k), "next_power_of_two" | "checked_next_power_of_two") => {
                let checked = method.starts_with("checked");
                helper(
                    "nextPow2",
                    vec![Frag::text(format!("{}, {checked}", k.bits()))],
                )
            }
            (Ty::Int(_), "get") if args.is_empty() => rf.clone(),
            // (`NonZero<char>::get`.)
            (Ty::Char, "get") if args.is_empty() => rf.clone(),
            // Your own `Hasher`'s provided `write_u32(x)`: the bytes to its `write`.
            (Ty::Adt(n, _), m2)
                if m2.starts_with("write_")
                    && self.types.adts.method(n, "write").is_some()
                    && self.types.adts.method(n, "finish").is_some() =>
            {
                let x = self.gap_arg(a0);
                let bits = IntKind::from_name(&m2["write_".len()..])?.bits();
                helper("hasherWrite", vec![x, Frag::text(bits.to_string())])
            }
            // (`RandomState`'s hasher is std's, your own builder's may not be.)
            (Ty::Adt(..), "hash_one") => helper("hashOne", vec![self.gap_arg(a0)]),
            _ => return None,
        };
        Some((f, ty))
    }

    /// `x.type_id()`, and `is::<T>()` / `downcast_ref::<T>()` / `downcast_mut` / `downcast` on a
    /// `dyn Any`: a test of the value at run time (`typeck_any`).
    pub(super) fn emit_any_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        method: &str,
    ) -> Option<(Frag, Ty)> {
        let range = self.range_of(m.syntax());
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        if let Some(s) = self.types.any_strs.get(&range).cloned() {
            return Some((Frag::mapped(name_off, js_string(&s)), Ty::Str));
        }
        let t = self.types.any_checks.get(&range).cloned()?;
        let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let (rf, _) = self.emit_expr(recv, None);
        let test = self.any_test(&t, "v");
        let body = match method {
            "is" => test,
            "downcast_ref" | "downcast_mut" => format!("{test} ? v : null"),
            _ => format!("{test} ? κσ.Ok(v) : κσ.Err(v)"),
        };
        let mut f = Frag::mapped(name_off, format!("((v) => {body})("));
        f.append(rf).push(")");
        Some((f, ty))
    }

    /// A JS test of whether `v` holds a `t`.
    fn any_test(&self, t: &Ty, v: &str) -> String {
        match t {
            Ty::Adt(n, _) => format!("{v} instanceof {}", self.class(n)),
            Ty::Int(k) if k.is_big() => format!("typeof {v} === \"bigint\""),
            Ty::Int(k) => {
                let (lo, hi) = k.min_max();
                format!(
                    "(typeof {v} === \"number\" && Number.isInteger({v}) && {v} >= {lo} && {v} <= {hi})"
                )
            }
            Ty::Float { .. } => format!("typeof {v} === \"number\""),
            Ty::Bool => format!("typeof {v} === \"boolean\""),
            Ty::Char => format!("(typeof {v} === \"string\" && [...{v}].length === 1)"),
            Ty::Str => format!("typeof {v} === \"string\""),
            Ty::Unit => format!("{v} === undefined"),
            Ty::Vec(_) => format!("Array.isArray({v})"),
            Ty::Tuple(es) => format!("(Array.isArray({v}) && {v}.length === {})", es.len()),
            Ty::Map(..) => format!("{v} instanceof κσ.HashMap"),
            Ty::Set(_) => format!("{v} instanceof κσ.HashSet"),
            _ => "false".into(),
        }
    }

    /// Associated functions from `typeck_gaps`.
    pub(super) fn emit_gaps_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[&str],
    ) -> Option<(Frag, Ty)> {
        let tail = match segs {
            ["std" | "core", rest @ ..] => rest,
            all => all,
        };
        let start = self.off(c.syntax());
        let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        let args: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let a0 = args.first();
        let a1 = args.get(1);
        let call = |name: &str, parts: Vec<Frag>| {
            let mut f = Frag::mapped(start, format!("κσ.${name}("));
            f.append(Frag::join(parts, ", ")).push(")");
            f
        };
        let f = match tail {
            ["Option", "from"] => call("some", vec![self.gap_arg(a0)]),
            ["slice", "from_ref"] => {
                let mut f = Frag::mapped(start, "[");
                f.append(self.gap_arg(a0)).push("]");
                f
            }
            [
                int,
                m @ ("from_le_bytes" | "from_be_bytes" | "from_ne_bytes"),
            ] if IntKind::from_name(int).is_some() => {
                let k = IntKind::from_name(int)?;
                let b = self.gap_arg(a0);
                let little = *m != "from_be_bytes";
                call(
                    "fromBytes",
                    vec![
                        b,
                        Frag::text(format!(
                            "{}, {little}, {}, {}",
                            k.bits(),
                            k.is_signed(),
                            k.is_big()
                        )),
                    ],
                )
            }
            [float @ ("f32" | "f64"), m @ ("from_le_bytes" | "from_be_bytes" | "from_ne_bytes")] => {
                let b = self.gap_arg(a0);
                let little = *m != "from_be_bytes";
                call(
                    "floatFromBytes",
                    vec![b, Frag::text(format!("{}, {little}", *float == "f32"))],
                )
            }
            // (`char::__nz_new`, and `NonZero::new(x)` by `x`'s type.)
            ["char", "__nz_new"] => {
                let mut f = Frag::text("((x) => x === \"\\0\" ? null : x)(");
                f.append(self.gap_arg(a0)).push(")");
                f
            }
            [.., "NonZero", "new"] => {
                let zero = match a0.and_then(|a| self.ty_of(a.syntax())) {
                    Some(Ty::Char) => "\"\\0\"",
                    Some(Ty::Int(k)) if k.is_big() => "0n",
                    _ => "0",
                };
                let mut f = Frag::text(format!("((x) => x === {zero} ? null : x)("));
                f.append(self.gap_arg(a0)).push(")");
                f
            }
            ["char", "__nz_new_unchecked"] | [.., "NonZero", "new_unchecked"] => self.gap_arg(a0),
            [int, "__nz_new"] if IntKind::from_name(int).is_some() => {
                let k = IntKind::from_name(int)?;
                let x = self.gap_arg(a0);
                let zero = if k.is_big() { "0n" } else { "0" };
                let mut f = Frag::text("((x) => x == ");
                f.push(zero).push(" ? null : x)(").append(x).push(")");
                f
            }
            [int, "__nz_new_unchecked"] if IntKind::from_name(int).is_some() => self.gap_arg(a0),
            [.., "fmt", which @ ("Display" | "Debug"), "fmt"]
            | [which @ ("Display" | "Debug"), "fmt"] => {
                let debug = *which == "Debug";
                let (x, xt) = match a0 {
                    Some(a) => self.emit_expr(a, None),
                    None => (Frag::text("undefined"), Ty::Unknown),
                };
                let f = self.gap_arg(a1);
                self.fmt_delegate(x, &xt, f, debug)
            }
            [.., "Ref" | "RefMut", "map"] => {
                let (x, f) = (self.gap_arg(a0), self.gap_arg(a1));
                let mut g = Frag::text("(");
                g.append(f).push(")(").append(x).push(")");
                g
            }
            [.., "Ref" | "RefMut", "clone"] => self.gap_arg(a0),
            [.., "RandomState" | "BuildHasherDefault", "new" | "default"] => {
                Frag::mapped(start, "undefined")
            }
            [.., "Vec", "new_in" | "with_capacity_in"] => Frag::mapped(start, "[]"),
            // `TypeId::of::<T>()`, `type_name::<T>()`: the type's name (`typeck_any`).
            [.., "TypeId", "of"] | ["any", "type_name" | "type_name_of_val"] => {
                // (Without one, the checker has reported the type parameter.)
                match self.types.any_strs.get(&self.range_of(c.syntax())) {
                    Some(s) => Frag::mapped(start, js_string(s)),
                    None => Frag::mapped(start, "undefined"),
                }
            }
            [.., "Box", "new_in" | "pin_in"] => self.gap_arg(a0),
            [
                ..,
                "HashMap",
                "with_hasher" | "with_capacity_and_hasher" | "default",
            ] => Frag::mapped(start, "new κσ.HashMap()"),
            [
                ..,
                "HashSet",
                "with_hasher" | "with_capacity_and_hasher" | "default",
            ] => Frag::mapped(start, "new κσ.HashSet()"),
            _ => return None,
        };
        Some((f, ty))
    }
}
