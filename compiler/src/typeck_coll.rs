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
        Coll::BTreeMap => matches!(
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
        Coll::BTreeSet => matches!(
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
        let want = if kind == Coll::BTreeMap { 2 } else { 1 };
        while args.len() < want {
            args.push(self.fresh(VarKind::General));
        }
        let b = |t: &Ty| Box::new(t.clone());
        let view = match kind {
            Coll::VecDeque | Coll::BinaryHeap => Ty::Vec(b(&args[0])),
            Coll::BTreeSet => Ty::Set(b(&args[0])),
            Coll::BTreeMap => Ty::Map(b(&args[0]), b(&args[1])),
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
        let t = self.coll_type(kind, vec![]);
        let Ty::Coll(_, view) = &t else {
            unreachable!()
        };
        match *ctor {
            "new" => self.arg_count(c.syntax(), "function", 0, args.len()),
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
        );
        if mutates && !(kind == Coll::BTreeMap && method == "insert") {
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
            (Coll::BinaryHeap, "drain") => sig(
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
            (_, m2) if shared(kind, m2) => return None,
            _ => {
                let msg = format!(
                    "no method named `{method}` found for `{}` (the std surface is still small; see docs/FEATURES.md)",
                    self.describe(&this)
                );
                self.error(m.syntax(), msg);
                for a in args {
                    self.infer(a, None);
                }
                Ty::Unknown
            }
        })
    }
}
