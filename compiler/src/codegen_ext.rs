//! Codegen for closures, iterators, `Ordering`, and std methods taking closures. A child module
//! of `codegen`, so it works directly on the code generator's state.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList, HasName},
};

use super::{
    Codegen, Dest, Frag, Local, Out, PatCode, is_pure, needs_hoist, simple_tail, strip_refs,
};
use crate::ty::{IntKind, Ty};
use crate::typeck::is_none_binding;

impl Codegen<'_> {
    /// Emit statements into a separate buffer, one indent deeper, and return them as a fragment
    /// (with their source mappings) for embedding in an expression: a closure's block body.
    pub(super) fn capture(&mut self, f: impl FnOnce(&mut Self)) -> Frag {
        let inner = Out {
            code: String::new(),
            line: 0,
            col: 0,
            indent: self.out.indent + 1,
            mappings: Vec::new(),
            marks: Vec::new(),
        };
        let saved = std::mem::replace(&mut self.out, inner);
        f(self);
        let inner = std::mem::replace(&mut self.out, saved);
        let mut frag = Frag::default();
        let mut pos = 0;
        for (at, src) in inner.marks {
            if at > pos {
                frag.parts.push((None, inner.code[pos..at].to_string()));
                pos = at;
            }
            frag.parts.push((Some(src), String::new()));
        }
        frag.parts.push((None, inner.code[pos..].to_string()));
        frag
    }

    // ------------------------------------------------------------ closures

    /// `|a, b| body` → `(a, b) => body`, or `(a, b) => { ... }` when the body needs statements.
    pub(super) fn emit_closure(&mut self, c: &ast::ClosureExpr) -> Frag {
        self.closure_with(c, |cg, params| {
            let mut js = Vec::new();
            let mut binds = Vec::new();
            for p in params {
                match p.pat() {
                    Some(ast::Pat::IdentPat(ip))
                        if ip.pat().is_none() && !is_none_binding(&ip) && ip.name().is_some() =>
                    {
                        let n = ip.name().unwrap();
                        let name = cg.fresh(n.text());
                        cg.declare(
                            n.text(),
                            Some(n.syntax()),
                            Local {
                                js: name.clone(),
                                ty: Ty::Unknown,
                            },
                        );
                        js.push(name);
                    }
                    Some(ast::Pat::WildcardPat(_)) | None => js.push(cg.temp()),
                    Some(pat) => {
                        // Destructuring parameter: bind inside the body.
                        let t = cg.temp();
                        let mut pc = PatCode::default();
                        cg.lower_pat(&pat, &t, &mut pc);
                        binds.extend(pc.binds);
                        js.push(t);
                    }
                }
            }
            (js, binds)
        })
    }

    /// A closure whose Rust parameters alias JS places instead of being JS parameters: e.g.
    /// `|v| *v += 1` called with a map slot becomes `($s) => { $s[1] = $s[1] + 1 }`.
    /// `subjects[i]` lists the place for parameter `i`, or one place per field of a tuple pattern.
    pub(super) fn emit_closure_aliased(
        &mut self,
        c: &ast::ClosureExpr,
        js_params: Vec<String>,
        subjects: Vec<Vec<String>>,
    ) -> Frag {
        self.closure_with(c, move |cg, params| {
            for (p, places) in params.iter().zip(subjects) {
                let Some(pat) = p.pat() else { continue };
                let mut pc = PatCode::default();
                match (&pat, places.as_slice()) {
                    (_, [place]) => cg.lower_pat(&pat, place, &mut pc),
                    (ast::Pat::TuplePat(tp), places) => {
                        for (field, place) in tp.fields().zip(places) {
                            cg.lower_pat(&field, place, &mut pc);
                        }
                    }
                    _ => {}
                }
                cg.alias_binds(pc.binds);
            }
            (js_params, Vec::new())
        })
    }

    /// Shared closure lowering. `params` declares the Rust parameters (in the closure's scope) and
    /// returns the JS parameter names plus any `const` bindings to emit at the top of the body.
    fn closure_with(
        &mut self,
        c: &ast::ClosureExpr,
        params: impl FnOnce(&mut Self, &[ast::Param]) -> (Vec<String>, Vec<(ast::Name, bool, String)>),
    ) -> Frag {
        let start = self.off(c.syntax());
        let ret = match self.ty_of(c.syntax()) {
            Some(Ty::Closure { ret, .. }) => *ret,
            _ => Ty::Unknown,
        };
        let rust_params: Vec<ast::Param> = c
            .param_list()
            .map(|l| l.params().collect())
            .unwrap_or_default();
        self.scopes.push(HashMap::new());
        let saved_ret = std::mem::replace(&mut self.ret_ty, ret.clone());
        let saved_async = std::mem::replace(&mut self.in_async, false);
        let saved_loops = std::mem::take(&mut self.loop_dests);
        let (js_params, binds) = params(self, &rust_params);
        let head = format!("({}) => ", js_params.join(", "));
        let body = c.body();
        let expression_body = binds.is_empty()
            && body.as_ref().is_some_and(|b| match b {
                ast::Expr::BlockExpr(blk) => simple_tail(blk).is_some(),
                other => !needs_hoist(other.syntax()),
            });
        let frag = match (&body, expression_body) {
            (Some(b), true) => {
                let b = match b {
                    ast::Expr::BlockExpr(blk) => simple_tail(blk).unwrap(),
                    other => other.clone(),
                };
                let (f, _) = self.emit_expr(&b, Some(&ret));
                Frag::mapped(start, head).append(f).clone()
            }
            _ => {
                let inner = self.capture(|cg| {
                    cg.emit_binds(binds);
                    match &body {
                        Some(ast::Expr::BlockExpr(blk))
                            if blk.label().is_none() && blk.modifier().is_none() =>
                        {
                            cg.emit_block_body(blk, &Dest::Return, Some(&ret));
                        }
                        Some(other) => {
                            cg.emit_into(other, &Dest::Return, Some(&ret));
                        }
                        None => {}
                    }
                });
                let mut f = Frag::mapped(start, format!("{head}{{\n"));
                f.append(inner)
                    .push(format!("{}}}", "  ".repeat(self.out.indent)));
                f
            }
        };
        self.ret_ty = saved_ret;
        self.in_async = saved_async;
        self.loop_dests = saved_loops;
        self.scopes.pop();
        frag
    }

    /// An argument as a closure (default lowering) or other function value.
    fn fn_arg(&mut self, arg: Option<ast::Expr>) -> Frag {
        match arg {
            Some(a) => self.emit_expr(&a, None).0,
            None => Frag::text("undefined"),
        }
    }

    /// An argument that's iterable in JS: maps iterate their entries, options yield 0 or 1 items.
    pub(super) fn iterable_arg(&mut self, arg: Option<ast::Expr>) -> Frag {
        let Some(a) = arg else {
            return Frag::text("[]");
        };
        let (f, t) = self.emit_expr(&a, None);
        match t {
            Ty::Map(..) => f.wrap("", ".$entries()"),
            Ty::Option(_) => f.wrap("κσ.$optIter(", ")"),
            _ => f,
        }
    }

    // ------------------------------------------------------------ ranges & Ordering

    /// A range as a value: an iterator. `κσ.$range(lo, hi, inclusive, chars)`.
    pub(super) fn emit_range_value(&mut self, r: &ast::RangeExpr) -> Frag {
        use ra_ap_syntax::ast::{RangeItem, RangeOp};
        let lo = r.start().map(|e| self.emit_expr(&e, None));
        let hi = r.end().map(|e| self.emit_expr(&e, None));
        let is_char = matches!(lo.as_ref().or(hi.as_ref()).map(|(_, t)| t), Some(Ty::Char));
        let inclusive = r.op_kind() == Some(RangeOp::Inclusive);
        let mut f = Frag::text("κσ.$range(");
        f.append(lo.map(|x| x.0).unwrap_or_else(|| Frag::text("0")))
            .push(", ")
            .append(hi.map(|x| x.0).unwrap_or_else(|| Frag::text("null")))
            .push(format!(", {inclusive}, {is_char})"));
        f
    }

    // ------------------------------------------------------------ methods

    /// `v.iter_mut().for_each(|x| ..)` and `v.iter_mut().enumerate().for_each(|(i, x)| ..)`:
    /// by index, with `x` aliasing `v[i]` so writes stick. Lowered before the receiver because
    /// `iter_mut()` has no value outside a loop.
    pub(super) fn iter_mut_for_each(&mut self, m: &ast::MethodCallExpr) -> Option<Frag> {
        if m.name_ref()?.text() != "for_each" {
            return None;
        }
        let mut recv = m.receiver()?;
        let mut enumerate = false;
        if let ast::Expr::MethodCallExpr(e) = &recv
            && e.name_ref()?.text() == "enumerate"
        {
            enumerate = true;
            recv = e.receiver()?;
        }
        let ast::Expr::MethodCallExpr(im) = &recv else {
            return None;
        };
        if im.name_ref()?.text() != "iter_mut" {
            return None;
        }
        let vec = im.receiver()?;
        let (vf, _) = self.emit_expr(&vec, None);
        let place = if is_pure(&strip_refs(&vec)) {
            vf.plain()
        } else {
            let t = self.temp();
            self.line(
                Frag::text(format!("const {t} = "))
                    .append(vf)
                    .push(";")
                    .clone(),
            );
            t
        };
        let i = self.temp();
        let element = format!("{place}[{i}]");
        let subjects = if enumerate {
            vec![vec![i.clone(), element]]
        } else {
            vec![vec![element]]
        };
        let closure = match m.arg_list()?.args().next()? {
            ast::Expr::ClosureExpr(c) => self.emit_closure_aliased(&c, vec![i], subjects),
            other => {
                // A function value gets a copy of the element; it can't write back.
                let (f, _) = self.emit_expr(&other, None);
                let call = if enumerate {
                    format!("[{i}, {place}[{i}]]")
                } else {
                    format!("{place}[{i}]")
                };
                Frag::text(format!("({i}) => "))
                    .append(f)
                    .push(format!("({call})"))
                    .clone()
            }
        };
        let mut f = Frag::mapped(self.off(m.syntax()), format!("κσ.$forEachIndex({place}, "));
        f.append(closure).push(")");
        Some(f)
    }

    /// Methods that take closures or produce iterators; `None` when `method` isn't one.
    pub(super) fn emit_method_ext(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: &Frag,
        rt: &Ty,
        method: &str,
    ) -> Option<(Frag, Ty)> {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        let args: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let a0 = args.first().cloned();
        let a1 = args.get(1).cloned();
        // Raw, so `collect` can build a `VecDeque` / `BTreeMap` / ..
        let result_ty = self.raw_ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        // `κσ.$name(recv, rest...)`
        let helper = |name: &str, rest: Vec<Frag>| {
            let mut all = vec![rf.clone()];
            all.extend(rest);
            let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
            f.append(Frag::join(all, ", ")).push(")");
            f
        };
        // `recv.$name(rest...)`
        let call = |name: &str, rest: Vec<Frag>| {
            let mut f = rf.clone();
            f.push(".")
                .append(Frag::mapped(name_off, format!("${name}(")));
            f.append(Frag::join(rest, ", ")).push(")");
            f
        };
        let out = |f: Frag| Some((f, result_ty.view()));
        if let Some(r) = self.emit_std_method(m, rf, rt, method) {
            return Some(r);
        }
        match (rt, method) {
            (Ty::Result(t, e), m2) => return self.emit_result_method(m, rf, t, e, m2),
            (Ty::Option(_), "ok_or" | "ok_or_else") => return Some(self.emit_ok_or(m, rf, method)),
            (Ty::Str, "parse") => return Some(self.emit_parse(m, rf)),
            (Ty::Formatter, "write_str") => {
                let s = self.fn_arg(m.arg_list().and_then(|a| a.args().next()));
                let mut f = Frag::text("κσ.$write(");
                f.append(rf.clone()).push(", ").append(s).push(", false)");
                return Some((f, result_ty.clone()));
            }
            _ => {}
        }
        match (rt, method) {
            // ---- sources
            (Ty::Str, "chars") => out(helper("iter", vec![])),
            (Ty::Str, "split") => {
                let p = self.fn_arg(a0);
                out(helper("split", vec![p]))
            }
            (Ty::Str, "split_whitespace") => out(helper("splitWhitespace", vec![])),
            (Ty::Str, "lines") => out(helper("lines", vec![])),
            (Ty::Vec(_) | Ty::Set(_), "iter" | "into_iter") => out(helper("iter", vec![])),
            // Over JS objects, the items are the objects themselves, so writes reach the Vec.
            (Ty::Vec(e), "iter_mut") if !e.is_js_primitive() && **e != Ty::Unknown => {
                out(helper("iter", vec![]))
            }
            (Ty::Str, "split_at") => {
                let n = self.fn_arg(a0);
                let loc = super::js_string(&self.location_at(name_off));
                out(helper("splitAt", vec![n, Frag::text(loc)]))
            }
            (Ty::Vec(_), "chunks_exact") => {
                let n = self.fn_arg(a0);
                out(helper("chunksExact", vec![n]))
            }
            (Ty::Vec(_), "windows" | "chunks") => {
                let n = self.fn_arg(a0);
                out(helper(method, vec![n]))
            }
            (Ty::Map(..), "iter" | "into_iter") => out(call("entries", vec![])),
            (Ty::Map(..), "keys" | "into_keys") => out(call("keys", vec![])),
            (Ty::Map(..), "values" | "into_values") => out(call("values", vec![])),

            // ---- Vec
            (Ty::Vec(_), "sort_by" | "sort_unstable_by") => {
                let f = self.fn_arg(a0);
                let mut s = rf.clone();
                s.push(".")
                    .append(Frag::mapped(name_off, "sort("))
                    .append(f)
                    .push(")");
                out(s)
            }
            (Ty::Vec(_), "sort_by_key" | "sort_unstable_by_key" | "sort_by_cached_key") => {
                let f = self.fn_arg(a0);
                out(helper("sortByKey", vec![f]))
            }
            (Ty::Vec(_), "retain") => {
                let f = self.fn_arg(a0);
                out(helper("retain", vec![f]))
            }
            (Ty::Vec(_), "dedup") => out(helper("dedup", vec![])),
            (Ty::Vec(_), "dedup_by_key") => {
                let f = self.fn_arg(a0);
                out(helper("dedupByKey", vec![f]))
            }
            (Ty::Vec(_), "extend") => {
                let src = self.iterable_arg(a0);
                out(helper("extend", vec![src]))
            }

            // ---- HashSet / HashMap
            (Ty::Set(_), "retain") => {
                let f = self.fn_arg(a0);
                out(call("retain", vec![f]))
            }
            (Ty::Set(_), "extend") | (Ty::Map(..), "extend") => {
                let src = self.iterable_arg(a0);
                out(call("extend", vec![src]))
            }
            (Ty::Set(_), "union" | "intersection" | "difference" | "symmetric_difference") => {
                let other = self.fn_arg(a0);
                let js = match method {
                    "symmetric_difference" => "symmetricDifference",
                    m2 => m2,
                };
                out(call(js, vec![other]))
            }
            (Ty::Map(..), "retain") => {
                // The closure's `v` is `&mut V`: alias the slot so writes stick.
                let f = match a0 {
                    Some(ast::Expr::ClosureExpr(c)) => {
                        let s = self.temp();
                        let subjects = vec![vec![format!("{s}[0]")], vec![format!("{s}[1]")]];
                        self.emit_closure_aliased(&c, vec![s], subjects)
                    }
                    other => {
                        let f = self.fn_arg(other);
                        let s = self.temp();
                        Frag::text(format!("({s}) => "))
                            .append(f)
                            .push(format!("({s}[0], {s}[1])"))
                            .clone()
                    }
                };
                out(call("retain", vec![f]))
            }

            // ---- Option
            (Ty::Option(_), m2) if crate::typeck::is_option_closure_method(m2) => {
                let first = self.fn_arg(a0);
                let rest: Vec<Frag> = a1
                    .map(|a| vec![self.emit_expr(&a, None).0])
                    .unwrap_or_default();
                let js = match m2 {
                    "map" | "and_then" => "optMap",
                    "filter" => "optFilter",
                    "unwrap_or_else" | "or_else" => "optOrElse",
                    "map_or" => "optMapOr",
                    "map_or_else" => "optMapOrElse",
                    "is_some_and" => "optIsSomeAnd",
                    "is_none_or" => "optIsNoneOr",
                    "or" => "optOr",
                    "zip" => "optZip",
                    "inspect" => "optInspect",
                    _ => unreachable!(),
                };
                out(helper(js, [vec![first], rest].concat()))
            }

            // ---- bool: `b.then(|| x)` / `b.then_some(x)` (the value is only evaluated when true)
            (Ty::Bool, "then" | "then_some") => {
                let v = match (&a0, method) {
                    (Some(a), "then") => {
                        let f = self.fn_arg(Some(a.clone()));
                        f.wrap("(", ")()")
                    }
                    (Some(a), _) => {
                        let (f, ty) = self.emit_expr(a, None);
                        self.by_value(a, f, &ty)
                    }
                    _ => Frag::text("undefined"),
                };
                let mut f = rf.clone().wrap("(", " ? ");
                f.append(v).push(" : null)");
                out(f)
            }

            // ---- char
            (Ty::Char, m2) if crate::typeck::char_predicate(m2).is_some() => {
                let re = crate::typeck::char_predicate(m2).unwrap();
                let mut f = Frag::mapped(name_off, format!("{re}.test("));
                f.append(rf.clone()).push(")");
                out(f)
            }
            (Ty::Char, "to_ascii_uppercase" | "to_ascii_lowercase") => {
                let upper = method == "to_ascii_uppercase";
                out(helper(
                    if upper { "asciiUpper" } else { "asciiLower" },
                    vec![],
                ))
            }
            (Ty::Char, "to_uppercase" | "to_lowercase") => {
                let js = if method == "to_uppercase" {
                    "toUpperCase"
                } else {
                    "toLowerCase"
                };
                let mut f = Frag::mapped(name_off, "κσ.$iter(");
                f.append(rf.clone()).push(format!(".{js}())"));
                out(f)
            }
            (Ty::Char, "to_digit" | "is_digit") => {
                let radix = self.fn_arg(a0);
                let f = helper("toDigit", vec![radix]);
                out(if method == "is_digit" {
                    f.wrap("(", " != null)")
                } else {
                    f
                })
            }

            // ---- comparisons
            (_, "cmp") => {
                let other = self.fn_arg(a0);
                out(helper("cmp", vec![other]))
            }
            (_, "partial_cmp") => {
                let other = self.fn_arg(a0);
                out(helper("partialCmp", vec![other]))
            }
            (Ty::Ordering, m2) => {
                let f = match m2 {
                    "reverse" => helper("ordReverse", vec![]),
                    "then" => {
                        let o = self.fn_arg(a0);
                        helper("ordThen", vec![o])
                    }
                    "then_with" => {
                        let o = self.fn_arg(a0);
                        helper("ordThenWith", vec![o])
                    }
                    "is_lt" => rf.clone().wrap("(", " < 0)"),
                    "is_le" => rf.clone().wrap("(", " <= 0)"),
                    "is_gt" => rf.clone().wrap("(", " > 0)"),
                    "is_ge" => rf.clone().wrap("(", " >= 0)"),
                    "is_eq" => rf.clone().wrap("(", " === 0)"),
                    "is_ne" => rf.clone().wrap("(", " !== 0)"),
                    _ => return None,
                };
                out(f)
            }

            // ---- iterators
            (Ty::Iter { elem, .. }, m2) => {
                let elem = (**elem).clone();
                self.emit_iter_method(m2, rf, &elem, a0, a1, &result_ty, &call)
            }
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_iter_method(
        &mut self,
        method: &str,
        rf: &Frag,
        elem: &Ty,
        a0: Option<ast::Expr>,
        a1: Option<ast::Expr>,
        result_ty: &Ty,
        call: &dyn Fn(&str, Vec<Frag>) -> Frag,
    ) -> Option<(Frag, Ty)> {
        let simple = |js: &str| -> &'static str {
            match js {
                "map" => "map",
                "filter" => "filter",
                "filter_map" => "filterMap",
                "enumerate" => "enumerate",
                "rev" => "rev",
                "cycle" => "cycle",
                "take" => "take",
                "skip" => "skip",
                "step_by" => "stepBy",
                "take_while" => "takeWhile",
                "skip_while" => "skipWhile",
                "inspect" => "inspect",
                "count" => "count",
                "as_str" => "asStr",
                "to_string" => "toString",
                "last" => "last",
                "nth" => "nth",
                "next" => "next",
                "min" => "min",
                "max" => "max",
                "min_by_key" => "minByKey",
                "max_by_key" => "maxByKey",
                "min_by" => "minBy",
                "max_by" => "maxBy",
                "any" => "any",
                "all" => "all",
                "find" => "find",
                "position" => "position",
                "rposition" => "rposition",
                "partition" => "partition",
                "find_map" => "findMap",
                "reduce" => "reduce",
                "for_each" => "forEach",
                _ => "",
            }
        };
        let f = match method {
            "cloned" | "copied" => {
                if elem.is_js_primitive() {
                    rf.clone()
                } else {
                    call("map", vec![Frag::text("κσ.$clone")])
                }
            }
            "flat_map" => {
                // A closure returning an Option keeps the `Some`s, like filter_map.
                let returns_option = matches!(
                    a0.as_ref().and_then(|a| self.ty_of(a.syntax())),
                    Some(Ty::Closure { ret, .. }) if matches!(*ret, Ty::Option(_))
                );
                let f = self.fn_arg(a0);
                call(
                    if returns_option {
                        "filterMap"
                    } else {
                        "flatMap"
                    },
                    vec![f],
                )
            }
            "flatten" => {
                if matches!(elem, Ty::Option(_)) {
                    call("filterMap", vec![Frag::text("(x) => x")])
                } else {
                    call("flatten", vec![])
                }
            }
            "zip" | "chain" => {
                let other = self.iterable_arg(a0);
                call(method, vec![other])
            }
            "fold" => {
                let init = self.fn_arg(a0);
                let f = self.fn_arg(a1);
                call("fold", vec![init, f])
            }
            "sum" | "product" => {
                let (mut op, start) = arith_fold(method, result_ty);
                // Debug builds: a sum or product past the type's range panics.
                if super::overflow_checks()
                    && let Ty::Int(k) = result_ty
                {
                    let (min, max) = super::int_bounds(*k);
                    let sign = if method == "sum" { "+" } else { "*" };
                    // (rustc reports it inside core's iterator code.)
                    let loc = super::js_string("<std>");
                    op = format!("(a, b) => κσ.$ckArith(a, b, \"{sign}\", {min}, {max}, {loc})");
                }
                call(method, vec![Frag::text(op), Frag::text(start)])
            }
            "collect" => {
                let into = |t: &Ty| match t {
                    Ty::Set(_) => "collectSet",
                    Ty::Map(..) => "collectMap",
                    Ty::Str => "collectString",
                    Ty::Coll(crate::ty::Coll::BTreeSet, _) => "collectBTreeSet",
                    Ty::Coll(crate::ty::Coll::BTreeMap, _) => "collectBTreeMap",
                    Ty::Coll(crate::ty::Coll::BinaryHeap, _) => "collectHeap",
                    _ => "collectVec",
                };
                match result_ty {
                    Ty::Result(c, _) => call(
                        "collectResult",
                        vec![Frag::text(format!("\"${}\"", into(c)))],
                    ),
                    Ty::Option(c) => call(
                        "collectOption",
                        vec![Frag::text(format!("\"${}\"", into(c)))],
                    ),
                    t => call(into(t), vec![]),
                }
            }
            m2 if !simple(m2).is_empty() => {
                let args: Vec<Frag> = [a0, a1]
                    .into_iter()
                    .flatten()
                    .map(|a| self.emit_expr(&a, None).0)
                    .collect();
                call(simple(m2), args)
            }
            _ => return None,
        };
        Some((f, result_ty.clone()))
    }
}

/// The add/multiply function and starting value for `sum` / `product` of `ty`, wrapping like the
/// integer type does. Float sums start at -0.0, as in Rust.
fn arith_fold(method: &str, ty: &Ty) -> (String, &'static str) {
    let k = match ty {
        Ty::Int(k) => Some(*k),
        Ty::IntLit => Some(IntKind::I32),
        _ => None,
    };
    let wrap = |e: &str| match k.and_then(|k| k.wrap_parts()) {
        Some((pre, post)) => format!("{pre}{e}{post}"),
        None => e.to_string(),
    };
    match (method, ty) {
        ("sum", Ty::Int(k @ (IntKind::I128 | IntKind::U128))) => (
            format!("(a, b) => κσ.$w128(a + b, {})", k.is_signed()),
            "0n",
        ),
        ("product", Ty::Int(k @ (IntKind::I128 | IntKind::U128))) => (
            format!("(a, b) => κσ.$w128(a * b, {})", k.is_signed()),
            "1n",
        ),
        ("sum", Ty::Float { .. }) => ("(a, b) => a + b".into(), "-0"),
        ("sum", _) => (format!("(a, b) => {}", wrap("a + b")), "0"),
        ("product", Ty::Float { .. }) => ("(a, b) => a * b".into(), "1"),
        ("product", _) if k.is_some_and(|k| k.is_narrow()) => {
            (format!("(a, b) => {}", wrap("Math.imul(a, b)")), "1")
        }
        _ => ("(a, b) => a * b".into(), "1"),
    }
}
