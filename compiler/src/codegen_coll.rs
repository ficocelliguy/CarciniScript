//! Codegen for `VecDeque`, `BinaryHeap`, `BTreeMap` and `BTreeSet` (see `typeck_coll`). Outside
//! this module they look like their view type (`ty_of` returns views), so only constructors and
//! the methods particular to a kind are handled here. A child module of `codegen`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

use super::{Codegen, Frag, strip_parens_expr};
use crate::ty::{Coll, Ty};

impl Codegen<'_> {
    /// `VecDeque::new()`, `BinaryHeap::from(v)`, `BTreeMap::from([..])`, ...
    pub(super) fn emit_coll_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[&str],
    ) -> Option<(Frag, Ty)> {
        let [.., ty, ctor] = segs else { return None };
        let kind = Coll::from_name(ty)?;
        let t = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        let start = self.off(c.syntax());
        let f = match *ctor {
            "from" => {
                let arg = self
                    .emit_args(c.arg_list(), &[])
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| Frag::text("[]"));
                match kind {
                    Coll::VecDeque => arg.wrap("[...", "]"),
                    Coll::BinaryHeap => arg.wrap("κσ.$heapFrom([...", "])"),
                    Coll::BTreeMap => arg.wrap("κσ.BTreeMap.from(", ")"),
                    Coll::BTreeSet => arg.wrap("κσ.BTreeSet.from(", ")"),
                }
            }
            _ => Frag::text(new_coll(kind)),
        };
        Some((f.at(start), t))
    }

    /// A method particular to `kind`, or None for one it shares with its view type.
    pub(super) fn emit_coll_method(
        &mut self,
        m: &ast::MethodCallExpr,
        kind: Coll,
        rf: Frag,
        method: &str,
    ) -> Option<(Frag, Ty)> {
        let js: &str = match (kind, method) {
            (Coll::VecDeque, "push_back") => "push",
            (Coll::VecDeque, "push_front") => "unshift",
            (Coll::VecDeque, "pop_back") => "pop",
            (Coll::VecDeque, "pop_front") => "shift",
            (
                Coll::VecDeque,
                "front" | "back" | "insert" | "remove" | "make_contiguous" | "append",
            ) => method,
            (
                Coll::BinaryHeap,
                "push" | "pop" | "peek" | "into_sorted_vec" | "into_vec" | "drain" | "into_iter"
                | "extend" | "append",
            ) => method,
            (Coll::BTreeSet, "first" | "last" | "pop_first" | "pop_last" | "append") => method,
            (
                Coll::BTreeMap,
                "first_key_value" | "last_key_value" | "pop_first" | "pop_last" | "append",
            ) => method,
            (Coll::BTreeMap | Coll::BTreeSet, "range") => {
                return Some(self.emit_btree_range(m, &rf));
            }
            _ => return None,
        };
        let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let args: Vec<ast::Expr> = m.arg_list().map(|l| l.args().collect()).unwrap_or_default();
        // `heap.extend(..)` emits its argument as an iterable below.
        let mut vals = Vec::new();
        if !(kind == Coll::BinaryHeap && js == "extend") {
            for a in &args {
                let (f, t) = self.emit_expr(a, None);
                vals.push(self.by_value(a, f, &t));
            }
        }
        let arg = |i: usize| {
            vals.get(i)
                .cloned()
                .unwrap_or_else(|| Frag::text("undefined"))
        };
        let call = |name: &str, rest: Vec<Frag>| {
            let mut all = vec![rf.clone()];
            all.extend(rest);
            let mut f = Frag::text(format!("κσ.${name}("));
            f.append(Frag::join(all, ", ")).push(")");
            f
        };
        let method_call = |name: &str, rest: Vec<Frag>| {
            let mut f = rf.clone();
            f.push(format!(".{name}("))
                .append(Frag::join(rest, ", "))
                .push(")");
            f
        };
        let f = match (kind, js) {
            (Coll::VecDeque, "push" | "unshift") => method_call(js, vec![arg(0)]),
            (Coll::VecDeque, "pop" | "shift") => method_call(js, vec![]).wrap("(", " ?? null)"),
            (Coll::VecDeque, "front") => rf.clone().wrap("(", "[0] ?? null)"),
            (Coll::VecDeque, "back") => rf.clone().wrap("(", ".at(-1) ?? null)"),
            (Coll::VecDeque, "insert") => {
                method_call("splice", vec![arg(0), Frag::text("0"), arg(1)])
            }
            (Coll::VecDeque, "remove") => call("dqRemove", vec![arg(0)]),
            (Coll::VecDeque, "make_contiguous") => rf.clone(),
            (Coll::VecDeque, "append") => {
                let mut f = rf.clone();
                f.push(".push(...").append(arg(0)).push(".splice(0))");
                f
            }
            (Coll::BinaryHeap, "push") => call("heapPush", vec![arg(0)]),
            (Coll::BinaryHeap, "pop") => call("heapPop", vec![]),
            (Coll::BinaryHeap, "peek") => rf.clone().wrap("(", "[0] ?? null)"),
            (Coll::BinaryHeap, "into_sorted_vec") => call("heapSorted", vec![]),
            (Coll::BinaryHeap, "into_vec") => rf.clone(),
            // (In the heap's array order, as std gives an arbitrary one.)
            (Coll::BinaryHeap, "into_iter") => rf.clone().wrap("κσ.$iter(", ")"),
            (Coll::BinaryHeap, "drain") => rf
                .clone()
                .wrap("new κσ.Iter(", ".splice(0)[Symbol.iterator]())"),
            (Coll::BinaryHeap, "extend") => {
                let items = self.iterable_arg(args.first().cloned());
                call("heapExtend", vec![items])
            }
            (Coll::BinaryHeap, "append") => call("heapAppend", vec![arg(0)]),
            (_, "append") => method_call("$append", vec![arg(0)]),
            (_, "first") => method_call("$first", vec![]),
            (_, "last") => method_call("$last", vec![]),
            (_, "pop_first") => method_call("$popFirst", vec![]),
            (_, "pop_last") => method_call("$popLast", vec![]),
            (_, "first_key_value") => method_call("$firstKeyValue", vec![]),
            (_, "last_key_value") => method_call("$lastKeyValue", vec![]),
            _ => return None,
        };
        let start = self.off(m.syntax());
        Some((f.at(start), ty))
    }
}

impl Codegen<'_> {
    /// `map.range(lo..hi)` → `map.$range(lo, hi, inclusive)` (`undefined` for an open end).
    fn emit_btree_range(&mut self, m: &ast::MethodCallExpr, rf: &Frag) -> (Frag, Ty) {
        use ra_ap_syntax::ast::RangeItem;
        let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let range = m.arg_list().and_then(|l| l.args().next());
        let Some(ast::Expr::RangeExpr(r)) = range.map(|e| strip_parens_expr(&e)) else {
            return (Frag::text("undefined"), ty);
        };
        let mut bound = |b: Option<ast::Expr>| match b {
            Some(b) => {
                let (f, t) = self.emit_expr(&b, None);
                self.by_value(&b, f, &t)
            }
            None => Frag::text("undefined"),
        };
        let (lo, hi) = (bound(r.start()), bound(r.end()));
        let inclusive = r.op_kind() == Some(ast::RangeOp::Inclusive);
        let mut f = rf.clone();
        f.push(".$range(")
            .append(lo)
            .push(", ")
            .append(hi)
            .push(format!(", {inclusive})"));
        (f.at(self.off(m.syntax())), ty)
    }
}

/// A new, empty collection of `kind`.
pub(super) fn new_coll(kind: Coll) -> &'static str {
    match kind {
        Coll::VecDeque | Coll::BinaryHeap => "[]",
        Coll::BTreeMap => "new κσ.BTreeMap()",
        Coll::BTreeSet => "new κσ.BTreeSet()",
    }
}
