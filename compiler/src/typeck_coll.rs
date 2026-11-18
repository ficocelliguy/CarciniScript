//! `VecDeque`, `BinaryHeap`, `BTreeMap` and `BTreeSet`. Each is a `Ty::Coll(kind, view)` whose
//! `view` is the type it behaves like at run time: a `Vec` (both are JS arrays), or a `HashMap` /
//! `HashSet` (the runtime's B-tree classes extend the hash classes and keep their keys sorted).
//! Methods particular to the kind are typed here; the rest of the view's methods are allowed
//! through when the kind has them too. A child module of `typeck`.

use ra_ap_syntax::{AstNode, ast};

use super::{Checker, Mutation, USIZE, VarKind};
use crate::ty::{Coll, Ty};

/// The view's methods that `kind` shares (same signature, same JS).
fn shared(kind: Coll, method: &str) -> bool {
    match kind {
        Coll::VecDeque => matches!(
            method,
            "len"
                | "is_empty"
                | "iter"
                | "iter_mut"
                | "into_iter"
                | "contains"
                | "clear"
                | "truncate"
                | "retain"
                | "extend"
                | "swap"
                | "get"
                | "drain"
        ),
        Coll::BinaryHeap => matches!(method, "len" | "is_empty" | "iter" | "clear"),
        // (Never a value's type: see `Coll::FixedArray`.)
        Coll::FixedArray(_) => false,
        Coll::BTreeMap | Coll::IndexMap => matches!(
            method,
            "insert"
                | "get"
                | "get_mut"
                | "remove"
                | "contains_key"
                | "len"
                | "is_empty"
                | "keys"
                | "values"
                | "values_mut"
                | "iter"
                | "iter_mut"
                | "into_iter"
                | "into_keys"
                | "into_values"
                | "entry"
                | "retain"
                | "extend"
                | "clear"
        ),
        Coll::BTreeSet | Coll::IndexSet => matches!(
            method,
            "insert"
                | "contains"
                | "remove"
                | "len"
                | "is_empty"
                | "iter"
                | "into_iter"
                | "retain"
                | "extend"
                | "clear"
        ),
    }
}

/// `IndexMap` / `IndexSet` methods about positions, sorting and the two kinds of removal.
pub(crate) fn index_method(method: &str) -> bool {
    matches!(
        method,
        "get_index"
            | "get_index_mut"
            | "get_index_of"
            | "get_full"
            | "insert_full"
            | "first"
            | "last"
            | "pop"
            | "remove"
            | "swap_remove"
            | "shift_remove"
            | "swap_remove_index"
            | "shift_remove_index"
            | "sort_keys"
            | "sort_unstable_keys"
            | "sort_by"
            | "sort_unstable_by"
            | "sort"
            | "sort_unstable"
            | "reverse"
            | "truncate"
    )
}

impl Checker<'_> {
    /// A B-tree or heap whose keys can't be ordered: its element type, described.
    pub(super) fn unordered_coll(&self, t: &Ty) -> Option<String> {
        let mut found = None;
        t.walk(&mut |x| {
            if found.is_some() {
                return;
            }
            let key = match x {
                Ty::Coll(Coll::BinaryHeap, v) | Ty::Coll(Coll::BTreeSet, v) => match &**v {
                    Ty::Vec(e) | Ty::Set(e) => (**e).clone(),
                    _ => return,
                },
                Ty::Coll(Coll::BTreeMap, v) => match &**v {
                    Ty::Map(k, _) => (**k).clone(),
                    _ => return,
                },
                _ => return,
            };
            if !self.implements(&key, "Ord") {
                found = Some(key.describe());
            }
        });
        found
    }

    /// `VecDeque<T>` etc. from the written (or missing) type arguments.
    pub(super) fn coll_type(&mut self, kind: Coll, mut args: Vec<Ty>) -> Ty {
        let want = if matches!(kind, Coll::BTreeMap | Coll::IndexMap) {
            2
        } else {
            1
        };
        while args.len() < want {
            args.push(self.fresh(VarKind::General));
        }
        let b = |t: &Ty| Box::new(t.clone());
        let view = match kind {
            Coll::VecDeque | Coll::BinaryHeap | Coll::FixedArray(_) => Ty::Vec(b(&args[0])),
            Coll::BTreeSet | Coll::IndexSet => Ty::Set(b(&args[0])),
            Coll::BTreeMap | Coll::IndexMap => Ty::Map(b(&args[0]), b(&args[1])),
        };
        Ty::Coll(kind, Box::new(view))
    }

    /// `VecDeque::new()`, `BTreeMap::from([..])`, ...
    pub(super) fn coll_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let [.., ty, ctor] = segs else { return None };
        let kind = Coll::from_name(ty)?;
        // (`VecDeque::<u8>::new()` names the type arguments.)
        let path_segs: Vec<ast::PathSegment> = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => {
                p.path().map(|p| p.segments().collect()).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        // (A trailing hasher argument, `IndexSet::<K, S>::default()`, is ignored.)
        use ra_ap_syntax::ast::HasGenericArgs;
        let keep = if matches!(kind, Coll::BTreeMap | Coll::IndexMap) {
            2
        } else {
            1
        };
        let trimmed: Vec<ast::PathSegment> = path_segs
            .iter()
            .map(|s| match s.generic_arg_list() {
                Some(gl)
                    if s.name_ref().is_some_and(|n| n.text() == *ty)
                        && gl.generic_args().count() > keep =>
                {
                    let kept: Vec<String> = gl
                        .generic_args()
                        .take(keep)
                        .map(|g| g.syntax().text().to_string())
                        .collect();
                    let text = format!("{}::<{}>", ty, kept.join(", "));
                    ra_ap_syntax::SourceFile::parse(
                        &format!("fn f() {{ {text}; }}"),
                        crate::macros::EDITION,
                    )
                    .tree()
                    .syntax()
                    .descendants()
                    .find_map(ast::PathSegment::cast)
                    .unwrap_or_else(|| s.clone())
                }
                _ => s.clone(),
            })
            .collect();
        let targs = self.segment_targs(&trimmed, ty);
        let t = self.coll_type(kind, targs);
        let Ty::Coll(_, view) = &t else {
            unreachable!()
        };
        match *ctor {
            "new" | "default" => self.arg_count(c.syntax(), "function", 0, args.len()),
            "with_capacity" if kind != Coll::BTreeMap && kind != Coll::BTreeSet => {
                self.arg_count(c.syntax(), "function", 1, args.len());
                if let Some(a) = args.first() {
                    self.check(a, &USIZE);
                }
            }
            "from" => {
                self.arg_count(c.syntax(), "function", 1, args.len());
                let items = match &**view {
                    Ty::Vec(e) | Ty::Set(e) => Ty::Vec(e.clone()),
                    Ty::Map(k, v) => {
                        Ty::Vec(Box::new(Ty::Tuple(vec![(**k).clone(), (**v).clone()])))
                    }
                    _ => Ty::Unknown,
                };
                if let Some(a) = args.first() {
                    self.check(a, &items);
                }
            }
            _ => {
                let msg = format!(
                    "no function or associated item named `{ctor}` found for `{}`",
                    kind.name()
                );
                self.error(c.syntax(), msg);
                for a in args {
                    self.infer(a, None);
                }
            }
        }
        Some(t)
    }

    /// `map[i]` / `set[i]` (a number that isn't a reference: indexmap's `Index<usize>`) by
    /// position, and `map[&k]` / `map[i]` as a place (`IndexMut`): a `Conv::Method` of the
    /// runtime's (`$at` / `$atMut` / `$keyMut`), with `index_places` for a place. None leaves
    /// a read by key to the maps' indexing.
    pub(super) fn index_coll_index(
        &mut self,
        ix: &ast::IndexExpr,
        kind: Coll,
        view: &Ty,
        index: &ast::Expr,
    ) -> Option<Ty> {
        use crate::adt::Conv;
        let by_ref = matches!(super::strip_parens(index), ast::Expr::RefExpr(_));
        let (key, value) = match view {
            Ty::Map(k, v) => ((**k).clone(), (**v).clone()),
            Ty::Set(e) => (USIZE, (**e).clone()),
            _ => return None,
        };
        let parent = ix.syntax().parent();
        let assigned = parent
            .clone()
            .and_then(ast::BinExpr::cast)
            .is_some_and(|b| {
                matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                    && b.lhs().is_some_and(|l| l.syntax() == ix.syntax())
            });
        // (`map[0].push_str(..)`: a string value needs the place too.)
        let str_receiver = self.shallow(&value) == Ty::Str
            && parent
                .and_then(ast::MethodCallExpr::cast)
                .is_some_and(|m| m.receiver().is_some_and(|r| r.syntax() == ix.syntax()));
        let place = (assigned || str_receiver) && kind == Coll::IndexMap;
        let positional = if kind == Coll::IndexSet || by_ref {
            kind == Coll::IndexSet
        } else {
            let it = self.infer(index, None);
            let int = matches!(self.shallow(&it), Ty::Int(_) | Ty::IntLit)
                || (matches!(self.shallow(&it), Ty::Var(_))
                    && self.var_kind(&it) == Some(VarKind::Int));
            if int {
                self.unify(&it, &USIZE);
            } else {
                self.expect(index.syntax(), &it, &key);
            }
            int
        };
        if positional && kind == Coll::IndexSet {
            self.check(index, &USIZE);
        }
        if by_ref && kind == Coll::IndexMap {
            self.check(index, &key);
        }
        if place {
            if let Some(recv) = ix.base() {
                self.check_mutable(&recv, Mutation::Borrow);
            }
            let r = self.range(ix.syntax());
            self.index_places.insert(r);
        }
        let js = match (positional, place) {
            (true, false) => "$at",
            (true, true) => "$atMut",
            (false, true) => "$keyMut",
            // (Read by key: `κσ.$mapIndex`.)
            (false, false) => return Some(value),
        };
        self.record_conv(ix.syntax(), Conv::Method { key: js.into() });
        Some(value)
    }

    /// An `IndexMap` / `IndexSet` method about positions (see `index_method`).
    fn index_coll_method(
        &mut self,
        m: &ast::MethodCallExpr,
        view: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        let opt = |t: Ty| Ty::Option(Box::new(t));
        let sig = |c: &mut Self, params: &[Ty], ret: Ty| {
            c.arg_count(m.syntax(), "method", params.len(), args.len());
            for (a, p) in args.iter().zip(params) {
                c.check(a, p);
            }
            ret
        };
        match view {
            Ty::Map(k, v) => {
                let (k, v) = ((**k).clone(), (**v).clone());
                let kv = Ty::Tuple(vec![k.clone(), v.clone()]);
                match method {
                    "get_index" => sig(self, &[USIZE], opt(kv)),
                    // (Numbers, strings and bools are copies: the value comes as a slot that
                    // `Some((k, v))` binds, so `*v += 1` reaches the map.)
                    "get_index_mut" => {
                        if let Some(recv) = m.receiver() {
                            self.check_mutable(&recv, Mutation::Borrow);
                        }
                        let resolved = self.zonk(&v);
                        if resolved.is_js_primitive() || resolved == Ty::Unknown {
                            if super::matched_position(m) {
                                let r = self.range(m.syntax());
                                self.slot_calls.insert(r);
                            } else {
                                self.error(
                                    m.syntax(),
                                    format!(
                                        "`IndexMap::get_index_mut` on a map of `{}` isn't supported here (the value would be copied, so changes wouldn't stick); use it in `if let Some((k, v)) = ..` / `match`, or index with `map[i]`",
                                        self.describe(&v)
                                    ),
                                );
                            }
                        }
                        sig(self, &[USIZE], opt(kv))
                    }
                    "get_index_of" => sig(self, &[k], opt(USIZE)),
                    "get_full" => sig(self, &[k.clone()], opt(Ty::Tuple(vec![USIZE, k, v]))),
                    "insert_full" => sig(self, &[k, v.clone()], Ty::Tuple(vec![USIZE, opt(v)])),
                    "first" | "last" | "pop" => sig(self, &[], opt(kv)),
                    "remove" | "swap_remove" | "shift_remove" => sig(self, &[k], opt(v)),
                    "swap_remove_index" | "shift_remove_index" => sig(self, &[USIZE], opt(kv)),
                    "sort_keys" | "sort_unstable_keys" => {
                        self.require_ord(m.syntax(), &k, "sort");
                        sig(self, &[], Ty::Unit)
                    }
                    "sort_by" | "sort_unstable_by" => {
                        self.arg_count(m.syntax(), "method", 1, args.len());
                        self.closure_arg(
                            args.first(),
                            vec![k.clone(), v.clone(), k, v],
                            Ty::Ordering,
                        );
                        Ty::Unit
                    }
                    "reverse" => sig(self, &[], Ty::Unit),
                    "truncate" => sig(self, &[USIZE], Ty::Unit),
                    _ => Ty::Unknown,
                }
            }
            Ty::Set(e) => {
                let e = (**e).clone();
                match method {
                    "get_index" => sig(self, &[USIZE], opt(e)),
                    "get_index_of" => sig(self, &[e], opt(USIZE)),
                    "insert_full" => sig(self, &[e], Ty::Tuple(vec![USIZE, Ty::Bool])),
                    "first" | "last" | "pop" => sig(self, &[], opt(e)),
                    "remove" | "swap_remove" | "shift_remove" => sig(self, &[e], Ty::Bool),
                    "swap_remove_index" | "shift_remove_index" => sig(self, &[USIZE], opt(e)),
                    "sort" | "sort_unstable" => {
                        self.require_ord(m.syntax(), &e, "sort");
                        sig(self, &[], Ty::Unit)
                    }
                    "sort_by" | "sort_unstable_by" => {
                        self.arg_count(m.syntax(), "method", 1, args.len());
                        self.closure_arg(args.first(), vec![e.clone(), e], Ty::Ordering);
                        Ty::Unit
                    }
                    "reverse" => sig(self, &[], Ty::Unit),
                    "truncate" => sig(self, &[USIZE], Ty::Unit),
                    _ => Ty::Unknown,
                }
            }
            _ => Ty::Unknown,
        }
    }

    /// Methods particular to a collection kind. `None` hands the call to the view's methods.
    pub(super) fn coll_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        kind: Coll,
        view: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let this = Ty::Coll(kind, Box::new(view.clone()));
        let elem = match view {
            Ty::Vec(e) | Ty::Set(e) => (**e).clone(),
            Ty::Map(k, v) => Ty::Tuple(vec![(**k).clone(), (**v).clone()]),
            _ => Ty::Unknown,
        };
        let opt = |t: Ty| Ty::Option(Box::new(t));
        let sig = |c: &mut Self, params: &[Ty], ret: Ty| {
            c.arg_count(m.syntax(), "method", params.len(), args.len());
            for (a, p) in args.iter().zip(params) {
                c.check(a, p);
            }
            ret
        };
        let mutates = matches!(
            method,
            "push_back"
                | "push_front"
                | "pop_back"
                | "pop_front"
                | "insert"
                | "remove"
                | "append"
                | "make_contiguous"
                | "push"
                | "pop"
                | "pop_first"
                | "pop_last"
                | "extend"
                | "insert_full"
                | "swap_remove"
                | "shift_remove"
                | "swap_remove_index"
                | "shift_remove_index"
                | "sort_keys"
                | "sort_unstable_keys"
                | "sort_by"
                | "sort_unstable_by"
                | "sort"
                | "sort_unstable"
                | "reverse"
                | "truncate"
        );
        if mutates
            && !(matches!(kind, Coll::BTreeMap | Coll::IndexMap | Coll::IndexSet)
                && method == "insert")
        {
            self.check_mutable(recv, Mutation::Borrow);
        }
        Some(match (kind, method) {
            (_, "clone") => sig(self, &[], this),
            (_, "append") => {
                if let Some(a) = args.first() {
                    self.check_mutable(a, Mutation::Borrow);
                }
                sig(self, &[this], Ty::Unit)
            }
            (Coll::VecDeque, "push_back" | "push_front") => sig(self, &[elem], Ty::Unit),
            (Coll::VecDeque, "pop_back" | "pop_front" | "front" | "back") => {
                sig(self, &[], opt(elem))
            }
            (Coll::VecDeque, "insert") => sig(self, &[USIZE, elem], Ty::Unit),
            (Coll::VecDeque, "remove") => sig(self, &[USIZE], opt(elem)),
            (Coll::VecDeque, "make_contiguous") => sig(self, &[], view.clone()),
            (Coll::BinaryHeap, "push") => {
                self.require_ord(m.syntax(), &elem, "sort");
                sig(self, &[elem], Ty::Unit)
            }
            (Coll::BinaryHeap, "pop" | "peek") => sig(self, &[], opt(elem)),
            (Coll::BinaryHeap, "into_sorted_vec" | "into_vec") => sig(self, &[], view.clone()),
            (Coll::BinaryHeap, "drain" | "into_iter") => sig(
                self,
                &[],
                Ty::Iter {
                    elem: Box::new(elem),
                    mutable: false,
                },
            ),
            (Coll::BinaryHeap, "extend") => {
                self.arg_count(m.syntax(), "method", 1, args.len());
                if let Some(a) = args.first() {
                    let t = self.infer(a, None);
                    match self.iter_elem(&t) {
                        Some(x) => {
                            if !self.unify(&x, &elem) {
                                let msg = format!(
                                    "mismatched types: expected an iterator over `{}`, found `{}`",
                                    self.describe(&elem),
                                    self.describe(&t)
                                );
                                self.error(a.syntax(), msg);
                            }
                        }
                        None => {
                            let msg = format!("`{}` is not an iterator", self.describe(&t));
                            self.error(a.syntax(), msg);
                        }
                    }
                }
                Ty::Unit
            }
            (Coll::BTreeSet, "first" | "last" | "pop_first" | "pop_last") => {
                sig(self, &[], opt(elem))
            }
            (Coll::BTreeSet, "is_subset" | "is_superset" | "is_disjoint") => {
                sig(self, &[this], Ty::Bool)
            }
            (Coll::BTreeSet, "union" | "intersection" | "difference" | "symmetric_difference") => {
                sig(
                    self,
                    &[this],
                    Ty::Iter {
                        elem: Box::new(elem),
                        mutable: false,
                    },
                )
            }
            (Coll::BTreeMap, "first_key_value" | "last_key_value" | "pop_first" | "pop_last") => {
                sig(self, &[], opt(elem))
            }
            // `map.range(lo..hi)`: the entries whose keys are in the range, in order.
            (Coll::BTreeMap | Coll::BTreeSet, "range") => {
                self.arg_count(m.syntax(), "method", 1, args.len());
                let key = match view {
                    Ty::Map(k, _) => (**k).clone(),
                    _ => elem.clone(),
                };
                match args.first().map(super::strip_parens) {
                    Some(ast::Expr::RangeExpr(r)) => {
                        use ra_ap_syntax::ast::RangeItem;
                        for b in [r.start(), r.end()].into_iter().flatten() {
                            self.check(&b, &key);
                        }
                        self.record(r.syntax(), Ty::Unknown);
                    }
                    Some(other) => {
                        self.error(
                            other.syntax(),
                            "`range` takes a range expression here (`a..b`, `a..=b`, `a..`, `..b`); `Bound` pairs aren't supported yet",
                        );
                        self.infer(&other, None);
                    }
                    None => {}
                }
                Ty::Iter {
                    elem: Box::new(elem),
                    mutable: false,
                }
            }
            // indexmap's positions: `get_index(i)`, `get_index_of(&k)`, `swap_remove` (what
            // `remove` is) and `shift_remove`, sorting.
            (Coll::IndexMap | Coll::IndexSet, m2) if index_method(m2) => {
                self.index_coll_method(m, view, method, args)
            }
            (_, m2) if shared(kind, m2) => return None,
            _ => {
                // `v.length()`: the JavaScript name.
                match crate::diag_help::js_alias(method, false) {
                    Some(alias) => {
                        let msg = format!(
                            "no method named `{method}` found for `{}` (`{method}` is {alias} in Rust)",
                            self.describe(&this)
                        );
                        self.error_help(m.syntax(), msg, crate::diag_help::JS_NAMES);
                    }
                    None => {
                        let msg = format!(
                            "no method named `{method}` found for `{}` (the std surface is still small; see docs/FEATURES.md)",
                            self.describe(&this)
                        );
                        self.error(m.syntax(), msg);
                    }
                }
                for a in args {
                    self.infer(a, None);
                }
                Ty::Unknown
            }
        })
    }
}
