//! The bundled crates: `rand` (and `itertools`, in `typeck_ext`'s iterator methods). Their
//! functions and methods are typed here; each call is recorded by its range with the runtime
//! helper that runs it (`κσ.$rand.gen(rng, T)`) and the types whose classes it takes.

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasGenericArgs, RangeItem},
};

use std::collections::HashMap;

use super::{Checker, Mutation, VarKind};
use crate::ty::{IntKind, Ty};

/// A call of a bundled crate's runtime helper: its name, and the types whose classes follow
/// the arguments (`gen::<u8>()` passes `u8`'s, for the range of values).
#[derive(Clone, Debug)]
pub struct CrateCall {
    pub js: String,
    pub tys: Vec<Ty>,
}

/// The `rand` functions a `use rand::prelude::*;` (or `use rand::thread_rng;`) brings in.
pub(super) const RAND_FNS: &[&str] = &[
    "thread_rng",
    "rng",
    "random",
    "random_range",
    "random_bool",
    "random_ratio",
];

/// The `Itertools` methods the bundled crate has (for the hint when it isn't imported).
pub(super) const ITERTOOLS_METHODS: &[&str] = &[
    "join",
    "format",
    "collect_vec",
    "sorted",
    "sorted_unstable",
    "sorted_by",
    "sorted_unstable_by",
    "sorted_by_key",
    "sorted_unstable_by_key",
    "sorted_by_cached_key",
    "unique",
    "unique_by",
    "dedup",
    "duplicates",
    "tuple_windows",
    "tuples",
    "circular_tuple_windows",
    "collect_tuple",
    "next_tuple",
    "combinations",
    "combinations_with_replacement",
    "permutations",
    "powerset",
    "cartesian_product",
    "zip_eq",
    "multi_cartesian_product",
    "interleave",
    "merge",
    "intersperse",
    "take_while_inclusive",
    "positions",
    "dropping",
    "k_smallest",
    "k_largest",
    "counts",
    "counts_by",
    "all_equal",
    "all_unique",
    "find_position",
    "chunk_by",
    "group_by",
    "into_group_map",
    "into_group_map_by",
];

const RAND_METHODS: &str = "`gen`, `gen_range`, `gen_bool`, `gen_ratio` (or their rand 0.9 names `random`, `random_range`, ..), `fill`, `next_u32` and `next_u64`";

impl Checker<'_> {
    /// Whether the file uses the bundled crate `name` (`use rand::Rng;`, `rand::random()`).
    pub(super) fn uses_crate(&self, name: &str) -> bool {
        self.src.contains(&format!("{name}::")) || self.src.contains(&format!("use {name}"))
    }

    fn note_crate(&mut self, node: &SyntaxNode, js: &str, tys: Vec<Ty>) {
        let r = self.range(node);
        self.crate_calls.insert(
            r,
            CrateCall {
                js: js.to_string(),
                tys,
            },
        );
    }

    /// The type a turbofish names (`random::<f64>()`), else a fresh variable.
    fn turbofish_or_fresh(&mut self, gl: Option<ast::GenericArgList>) -> Ty {
        let given = gl
            .and_then(|l| l.generic_args().next())
            .and_then(|g| match g {
                ast::GenericArg::TypeArg(t) => t.ty(),
                _ => None,
            });
        match given {
            Some(t) => self.lower_type(&t),
            None => self.fresh(VarKind::General),
        }
    }

    /// `lo..hi` / `lo..=hi` written out (what `gen_range` takes): the element type.
    fn sample_range(&mut self, node: &SyntaxNode, what: &str, a: Option<&ast::Expr>) -> Ty {
        match a.map(super::strip_parens) {
            // (Any number bounds, floats too: it isn't iterated.)
            Some(ast::Expr::RangeExpr(r)) if r.start().is_some() && r.end().is_some() => {
                let elem = self.fresh(VarKind::General);
                for b in [r.start(), r.end()].into_iter().flatten() {
                    self.check(&b, &elem);
                }
                self.record(r.syntax(), Ty::Unknown);
                elem
            }
            other => {
                if let Some(e) = other {
                    self.infer(&e, None);
                }
                self.error(
                    node,
                    format!("`{what}` takes a range written out here (`lo..hi` or `lo..=hi`)"),
                );
                Ty::Unknown
            }
        }
    }

    /// `rand::thread_rng()`, `rand::random::<f64>()`, `StdRng::seed_from_u64(7)`.
    pub(super) fn crate_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        if !self.uses_crate("rand") {
            return None;
        }
        let tail: &[&str] = match names {
            ["rand", rest @ ..] => rest,
            all => all,
        };
        let tail: &[&str] = match tail {
            ["rngs", rest @ ..] => rest,
            t => t,
        };
        let count = |c2: &mut Self, n: usize| c2.arg_count(c.syntax(), "function", n, args.len());
        let gl = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.generic_arg_list()),
            _ => None,
        };
        let rngs = [
            "StdRng",
            "SmallRng",
            "ThreadRng",
            "ChaCha8Rng",
            "ChaCha12Rng",
            "ChaCha20Rng",
        ];
        Some(match tail {
            ["thread_rng" | "rng"] => {
                count(self, 0);
                self.note_crate(c.syntax(), "rand.thread", vec![]);
                Ty::Rng
            }
            [r, "seed_from_u64"] if rngs.contains(r) => {
                count(self, 1);
                if let Some(a) = args.first() {
                    self.check(a, &Ty::Int(IntKind::U64));
                }
                self.note_crate(c.syntax(), "rand.seeded", vec![]);
                Ty::Rng
            }
            [r, "from_entropy" | "from_os_rng"] if rngs.contains(r) => {
                count(self, 0);
                self.note_crate(c.syntax(), "rand.entropy", vec![]);
                Ty::Rng
            }
            ["random"] => {
                count(self, 0);
                let t = self.turbofish_or_fresh(gl);
                self.note_crate(c.syntax(), "rand.random", vec![t.clone()]);
                t
            }
            ["random_range"] => {
                count(self, 1);
                let t = self.sample_range(c.syntax(), "random_range", args.first());
                self.note_crate(c.syntax(), "rand.randomRange", vec![t.clone()]);
                t
            }
            ["random_bool"] => {
                count(self, 1);
                if let Some(a) = args.first() {
                    self.check(a, &Ty::Float { f32: false });
                }
                self.note_crate(c.syntax(), "rand.randomBool", vec![]);
                Ty::Bool
            }
            ["random_ratio"] => {
                count(self, 2);
                for a in args.iter().take(2) {
                    self.check(a, &Ty::Int(IntKind::U32));
                }
                self.note_crate(c.syntax(), "rand.randomRatio", vec![]);
                Ty::Bool
            }
            _ => return None,
        })
    }

    /// Methods of an RNG, and `rand`'s on slices (`shuffle`, `choose`) and iterators
    /// (`choose`).
    pub(super) fn crate_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        resolved: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let count = |c2: &mut Self, n: usize| c2.arg_count(m.syntax(), "method", n, args.len());
        let rng_arg = |c2: &mut Self, i: usize| {
            if let Some(a) = args.get(i) {
                c2.check(a, &Ty::Rng);
            }
        };
        let usize_arg = |c2: &mut Self, i: usize| {
            if let Some(a) = args.get(i) {
                c2.check(a, &Ty::Int(IntKind::Usize));
            }
        };
        if let Some(t) = self.anyhow_method(m, resolved, method, args) {
            return Some(t);
        }
        // A `dyn Error`'s `to_string()` and `source()` (the next error in its chain).
        if matches!(resolved, Ty::Dyn(tr) if tr == "Error")
            && matches!(method, "to_string" | "source")
        {
            self.arg_count(m.syntax(), "method", 0, args.len());
            if method == "source" {
                self.note_crate(m.syntax(), "err.source", vec![]);
                return Some(Ty::Option(Box::new(Ty::Dyn("Error".into()))));
            }
            self.note_crate(m.syntax(), "err.toString", vec![]);
            return Some(Ty::Str);
        }
        // `serde_json::Error`: where it failed, and what kind of failure.
        if matches!(resolved, Ty::Extern(n) if n == "serde_json::Error") {
            let (js, ret) = match method {
                "line" => ("serde_json.line", Ty::Int(IntKind::Usize)),
                "column" => ("serde_json.column", Ty::Int(IntKind::Usize)),
                "is_eof" => ("serde_json.isEof", Ty::Bool),
                "is_syntax" => ("serde_json.isSyntax", Ty::Bool),
                "is_data" => ("serde_json.isData", Ty::Bool),
                "is_io" => ("serde_json.isIo", Ty::Bool),
                "to_string" => ("serde_json.toString", Ty::Str),
                _ => return None,
            };
            self.arg_count(m.syntax(), "method", 0, args.len());
            self.note_crate(m.syntax(), js, vec![]);
            return Some(ret);
        }
        if let Ty::Iter { elem, .. } = resolved
            && self.uses_crate("itertools")
            && let Some(t) = self.itertools_method(m, elem, method, args)
        {
            return Some(t);
        }
        match resolved {
            Ty::Rng => {
                if matches!(
                    method,
                    "gen"
                        | "r#gen"
                        | "random"
                        | "gen_range"
                        | "random_range"
                        | "fill"
                        | "try_fill"
                        | "fill_bytes"
                        | "gen_bool"
                        | "random_bool"
                        | "gen_ratio"
                        | "random_ratio"
                        | "next_u32"
                        | "next_u64"
                ) {
                    self.check_mutable(recv, Mutation::Borrow);
                }
                Some(match method {
                    "gen" | "r#gen" | "random" => {
                        count(self, 0);
                        let t = self.turbofish_or_fresh(m.generic_arg_list());
                        self.note_crate(m.syntax(), "rand.gen", vec![t.clone()]);
                        t
                    }
                    // (rand 0.7's `gen_range(lo, hi)` too.)
                    "gen_range" if args.len() == 2 => {
                        let t = self.fresh(VarKind::General);
                        for a in args {
                            self.check(a, &t);
                        }
                        self.note_crate(m.syntax(), "rand.range2", vec![t.clone()]);
                        t
                    }
                    "gen_range" | "random_range" => {
                        count(self, 1);
                        let t = self.sample_range(m.syntax(), method, args.first());
                        self.note_crate(m.syntax(), "rand.range", vec![t.clone()]);
                        t
                    }
                    // `rng.fill(&mut buf)`: random values of the buffer's element type.
                    "fill" | "try_fill" | "fill_bytes" => {
                        count(self, 1);
                        let e = self.fresh(VarKind::General);
                        if let Some(a) = args.first() {
                            self.check(a, &Ty::Vec(Box::new(e.clone())));
                        }
                        self.note_crate(m.syntax(), "rand.fill", vec![e]);
                        if method != "try_fill" {
                            Ty::Unit
                        } else {
                            Ty::Result(Box::new(Ty::Unit), Box::new(Ty::Unknown))
                        }
                    }
                    "gen_bool" | "random_bool" => {
                        count(self, 1);
                        if let Some(a) = args.first() {
                            self.check(a, &Ty::Float { f32: false });
                        }
                        self.note_crate(m.syntax(), "rand.bool", vec![]);
                        Ty::Bool
                    }
                    "gen_ratio" | "random_ratio" => {
                        count(self, 2);
                        for a in args.iter().take(2) {
                            self.check(a, &Ty::Int(IntKind::U32));
                        }
                        self.note_crate(m.syntax(), "rand.ratio", vec![]);
                        Ty::Bool
                    }
                    "next_u32" => {
                        count(self, 0);
                        self.note_crate(m.syntax(), "rand.nextU32", vec![]);
                        Ty::Int(IntKind::U32)
                    }
                    "next_u64" => {
                        count(self, 0);
                        self.note_crate(m.syntax(), "rand.nextU64", vec![]);
                        Ty::Int(IntKind::U64)
                    }
                    _ => {
                        for a in args {
                            self.infer(a, None);
                        }
                        self.error(
                            m.syntax(),
                            format!(
                                "no method named `{method}` found for `{}` (the bundled `rand` has {RAND_METHODS})",
                                resolved.describe()
                            ),
                        );
                        Ty::Unknown
                    }
                })
            }
            Ty::Vec(e) if self.uses_crate("rand") => {
                let e = (**e).clone();
                Some(match method {
                    "shuffle" => {
                        count(self, 1);
                        self.check_mutable(recv, Mutation::Borrow);
                        rng_arg(self, 0);
                        self.note_crate(m.syntax(), "rand.shuffle", vec![]);
                        Ty::Unit
                    }
                    "choose" => {
                        count(self, 1);
                        rng_arg(self, 0);
                        self.note_crate(m.syntax(), "rand.choose", vec![]);
                        Ty::Option(Box::new(e))
                    }
                    "choose_multiple" | "sample" => {
                        count(self, 2);
                        rng_arg(self, 0);
                        usize_arg(self, 1);
                        self.note_crate(m.syntax(), "rand.chooseMultiple", vec![]);
                        Ty::Iter {
                            elem: Box::new(e),
                            mutable: false,
                        }
                    }
                    _ => return None,
                })
            }
            Ty::Iter { elem, .. } if self.uses_crate("rand") => {
                let e = (**elem).clone();
                Some(match method {
                    "choose" => {
                        count(self, 1);
                        rng_arg(self, 0);
                        self.note_crate(m.syntax(), "rand.iterChoose", vec![]);
                        Ty::Option(Box::new(e))
                    }
                    "choose_multiple" | "sample" => {
                        count(self, 2);
                        rng_arg(self, 0);
                        usize_arg(self, 1);
                        self.note_crate(m.syntax(), "rand.iterChooseMultiple", vec![]);
                        Ty::Vec(Box::new(e))
                    }
                    _ => return None,
                })
            }
            _ => None,
        }
    }
}

impl Checker<'_> {
    /// `itertools::repeat_n(x, n)`.
    pub(super) fn itertools_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        if !self.uses_crate("itertools") {
            return None;
        }
        let tail: &[&str] = match names {
            ["itertools", rest @ ..] => rest,
            all => all,
        };
        match tail {
            ["repeat_n"] => {
                self.arg_count(c.syntax(), "function", 2, args.len());
                let t = match args.first() {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                };
                if let Some(n) = args.get(1) {
                    self.check(n, &Ty::Int(IntKind::Usize));
                }
                self.note_crate(c.syntax(), "itertools.repeatN", vec![]);
                Some(Ty::Iter {
                    elem: Box::new(t),
                    mutable: false,
                })
            }
            _ => None,
        }
    }

    /// The `Itertools` methods on an iterator of `elem`.
    fn itertools_method(
        &mut self,
        m: &ast::MethodCallExpr,
        elem: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let e = elem.clone();
        let iter = |t: Ty| Ty::Iter {
            elem: Box::new(t),
            mutable: false,
        };
        let usize_ty = Ty::Int(IntKind::Usize);
        let (js, ret): (&str, Ty) = match method {
            "join" | "format" => {
                self.crate_args(m, args, &[Ty::Str]);
                ("join", Ty::Str)
            }
            "collect_vec" => {
                self.crate_args(m, args, &[]);
                ("collectVec", Ty::Vec(Box::new(e)))
            }
            "sorted" | "sorted_unstable" => {
                self.crate_args(m, args, &[]);
                self.require_ord(m.syntax(), &e, "sorted");
                ("sorted", iter(e))
            }
            "sorted_by" | "sorted_unstable_by" => {
                let f = Ty::Closure {
                    params: vec![e.clone(), e.clone()],
                    ret: Box::new(Ty::Ordering),
                };
                self.crate_args(m, args, &[f]);
                ("sortedBy", iter(e))
            }
            "sorted_by_key" | "sorted_unstable_by_key" | "sorted_by_cached_key" => {
                let k = self.fresh(VarKind::General);
                let f = Ty::Closure {
                    params: vec![e.clone()],
                    ret: Box::new(k),
                };
                self.crate_args(m, args, &[f]);
                let js = if method == "sorted_by_cached_key" {
                    "sortedByCachedKey"
                } else {
                    "sortedByKey"
                };
                (js, iter(e))
            }
            "unique" | "dedup" | "duplicates" => {
                self.crate_args(m, args, &[]);
                let js = match method {
                    "unique" => "unique",
                    "dedup" => "dedup",
                    _ => "duplicates",
                };
                (js, iter(e))
            }
            "unique_by" => {
                let k = self.fresh(VarKind::General);
                let f = Ty::Closure {
                    params: vec![e.clone()],
                    ret: Box::new(k),
                };
                self.crate_args(m, args, &[f]);
                ("uniqueBy", iter(e))
            }
            "tuple_windows" | "tuples" | "circular_tuple_windows" => {
                self.crate_args(m, args, &[]);
                let n = self.tuple_arity(m.syntax(), m.generic_arg_list());
                let t = Ty::Tuple(vec![e; n]);
                let js = match method {
                    "tuple_windows" => "tupleWindows",
                    "tuples" => "tuples",
                    _ => "circularTupleWindows",
                };
                self.note_crate(m.syntax(), &format!("itertools.{js}"), vec![t.clone()]);
                return Some(iter(t));
            }
            "collect_tuple" | "next_tuple" => {
                self.crate_args(m, args, &[]);
                let n = self.tuple_arity(m.syntax(), m.generic_arg_list());
                let t = Ty::Tuple(vec![e; n]);
                let js = if method == "collect_tuple" {
                    "collectTuple"
                } else {
                    "nextTuple"
                };
                self.note_crate(m.syntax(), &format!("itertools.{js}"), vec![t.clone()]);
                return Some(Ty::Option(Box::new(t)));
            }
            "combinations" | "combinations_with_replacement" | "permutations" => {
                self.crate_args(m, args, &[usize_ty]);
                let js = match method {
                    "combinations" => "combinations",
                    "permutations" => "permutations",
                    _ => "combinationsWithReplacement",
                };
                (js, iter(Ty::Vec(Box::new(e))))
            }
            "powerset" => {
                self.crate_args(m, args, &[]);
                ("powerset", iter(Ty::Vec(Box::new(e))))
            }
            "cartesian_product" | "zip_eq" => {
                let u = self.iterable_arg(m, args);
                let js = if method == "zip_eq" {
                    "zipEq"
                } else {
                    "cartesianProduct"
                };
                (js, iter(Ty::Tuple(vec![e, u])))
            }
            "multi_cartesian_product" => {
                self.crate_args(m, args, &[]);
                let u = self.iter_elem(&e).unwrap_or(Ty::Unknown);
                ("multiCartesianProduct", iter(Ty::Vec(Box::new(u))))
            }
            "interleave" | "merge" => {
                let u = self.iterable_arg(m, args);
                if !self.unify(&e, &u) {
                    let msg = format!(
                        "mismatched types: expected items of type `{}`, found `{}`",
                        self.describe(&e),
                        self.describe(&u)
                    );
                    self.error(m.syntax(), msg);
                }
                (
                    if method == "merge" {
                        "merge"
                    } else {
                        "interleave"
                    },
                    iter(e),
                )
            }
            "intersperse" => {
                self.crate_args(m, args, std::slice::from_ref(&e));
                ("intersperse", iter(e))
            }
            "take_while_inclusive" | "positions" => {
                let f = Ty::Closure {
                    params: vec![e.clone()],
                    ret: Box::new(Ty::Bool),
                };
                self.crate_args(m, args, &[f]);
                if method == "positions" {
                    ("positions", iter(usize_ty))
                } else {
                    ("takeWhileInclusive", iter(e))
                }
            }
            "dropping" => {
                self.crate_args(m, args, &[usize_ty]);
                ("dropping", iter(e))
            }
            "k_smallest" | "k_largest" => {
                self.crate_args(m, args, &[usize_ty]);
                self.require_ord(m.syntax(), &e, method);
                (
                    if method == "k_smallest" {
                        "kSmallest"
                    } else {
                        "kLargest"
                    },
                    iter(e),
                )
            }
            "counts" => {
                self.crate_args(m, args, &[]);
                ("counts", Ty::Map(Box::new(e), Box::new(usize_ty)))
            }
            "counts_by" => {
                let k = self.fresh(VarKind::General);
                let f = Ty::Closure {
                    params: vec![e],
                    ret: Box::new(k.clone()),
                };
                self.crate_args(m, args, &[f]);
                ("countsBy", Ty::Map(Box::new(k), Box::new(usize_ty)))
            }
            "all_equal" | "all_unique" => {
                self.crate_args(m, args, &[]);
                (
                    if method == "all_equal" {
                        "allEqual"
                    } else {
                        "allUnique"
                    },
                    Ty::Bool,
                )
            }
            "contains" => {
                self.crate_args(m, args, std::slice::from_ref(&e));
                ("contains", Ty::Bool)
            }
            "find_position" => {
                let f = Ty::Closure {
                    params: vec![e.clone()],
                    ret: Box::new(Ty::Bool),
                };
                self.crate_args(m, args, &[f]);
                (
                    "findPosition",
                    Ty::Option(Box::new(Ty::Tuple(vec![usize_ty, e]))),
                )
            }
            "chunk_by" | "group_by" => {
                let k = self.fresh(VarKind::General);
                let f = Ty::Closure {
                    params: vec![e.clone()],
                    ret: Box::new(k.clone()),
                };
                self.crate_args(m, args, &[f]);
                ("chunkBy", iter(Ty::Tuple(vec![k, iter(e)])))
            }
            "into_group_map" => {
                self.crate_args(m, args, &[]);
                let (k, v) = (self.fresh(VarKind::General), self.fresh(VarKind::General));
                if !self.unify(&e, &Ty::Tuple(vec![k.clone(), v.clone()])) {
                    let msg = format!(
                        "`into_group_map` needs an iterator of `(key, value)` pairs, not `{}`",
                        self.describe(&e)
                    );
                    self.error(m.syntax(), msg);
                }
                (
                    "intoGroupMap",
                    Ty::Map(Box::new(k), Box::new(Ty::Vec(Box::new(v)))),
                )
            }
            "into_group_map_by" => {
                let k = self.fresh(VarKind::General);
                let f = Ty::Closure {
                    params: vec![e.clone()],
                    ret: Box::new(k.clone()),
                };
                self.crate_args(m, args, &[f]);
                (
                    "intoGroupMapBy",
                    Ty::Map(Box::new(k), Box::new(Ty::Vec(Box::new(e)))),
                )
            }
            _ => return None,
        };
        self.note_crate(m.syntax(), &format!("itertools.{js}"), vec![]);
        Some(ret)
    }

    /// Check a bundled method's arguments against `params`.
    fn crate_args(&mut self, m: &ast::MethodCallExpr, args: &[ast::Expr], params: &[Ty]) {
        self.arg_count(m.syntax(), "method", params.len(), args.len());
        for (a, p) in args.iter().zip(params) {
            self.check(a, p);
        }
    }

    /// The one argument of `cartesian_product(other)` and friends: anything iterable; its items'
    /// type.
    fn iterable_arg(&mut self, m: &ast::MethodCallExpr, args: &[ast::Expr]) -> Ty {
        self.arg_count(m.syntax(), "method", 1, args.len());
        let Some(a) = args.first() else {
            return Ty::Unknown;
        };
        let t = self.infer(a, None);
        match self.iter_elem(&t) {
            Some(u) => u,
            None => {
                let msg = format!("`{}` is not an iterator", self.describe(&t));
                self.error(a.syntax(), msg);
                Ty::Unknown
            }
        }
    }

    /// The size of the tuples `tuple_windows()` / `collect_tuple()` make: from a turbofish
    /// (`::<(_, _, _)>`), else from where they're taken apart (`|(a, b)|` in the next
    /// adapter's closure, `for (a, b) in ..`, `let (a, b) = ...unwrap()`, `Some((a, b))`),
    /// else pairs.
    fn tuple_arity(&self, node: &SyntaxNode, gl: Option<ast::GenericArgList>) -> usize {
        if let Some(ast::GenericArg::TypeArg(t)) = gl.and_then(|l| l.generic_args().next())
            && let Some(ast::Type::TupleType(tt)) = t.ty()
        {
            return tt.fields().count();
        }
        let tuple_pat = |p: Option<ast::Pat>| -> Option<usize> {
            match p? {
                ast::Pat::TuplePat(tp) => Some(tp.fields().count()),
                ast::Pat::TupleStructPat(sp) => match sp.fields().next()? {
                    ast::Pat::TuplePat(tp) => Some(tp.fields().count()),
                    _ => None,
                },
                _ => None,
            }
        };
        let mut cur = node.clone();
        for _ in 0..4 {
            let Some(parent) = cur.parent() else { break };
            if let Some(mc) = ast::MethodCallExpr::cast(parent.clone()) {
                if let Some(ast::Expr::ClosureExpr(c)) = mc.arg_list().and_then(|l| l.args().next())
                    && let Some(n) = tuple_pat(
                        c.param_list()
                            .and_then(|pl| pl.params().next())
                            .and_then(|p| p.pat()),
                    )
                {
                    return n;
                }
                // (`.unwrap()` / `.expect(..)` between: look further up.)
            } else if let Some(f) = ast::ForExpr::cast(parent.clone()) {
                return tuple_pat(f.pat()).unwrap_or(2);
            } else if let Some(l) = ast::LetStmt::cast(parent.clone()) {
                return tuple_pat(l.pat()).unwrap_or(2);
            } else if let Some(le) = ast::LetExpr::cast(parent.clone()) {
                return tuple_pat(le.pat()).unwrap_or(2);
            } else if !ast::ArgList::can_cast(parent.kind())
                && !ast::ParenExpr::can_cast(parent.kind())
            {
                break;
            }
            cur = parent;
        }
        2
    }
}

/// The crates built into the compiler (and `js`, the browser's).
pub(crate) const BUNDLED: &[&str] = &[
    "js",
    "serde",
    "serde_json",
    "rand",
    "itertools",
    "anyhow",
    "thiserror",
    "bitflags",
    "once_cell",
    "lazy_static",
    "either",
    "cfg_if",
    "hashbrown",
    "rayon",
    "regex",
];

/// `serde_json::Error`: the bundled `serde_json`'s (the runtime's `SerdeJsonError`).
pub(super) fn serde_json_error() -> Ty {
    Ty::Extern("serde_json::Error".into())
}

/// `serde_json::Value` (the prelude's).
pub(super) fn json_value() -> Ty {
    Ty::Adt("Value".into(), vec![])
}

/// `anyhow::Error`: the bundled `anyhow`'s error (the runtime's `AnyhowError`).
pub(super) fn anyhow_error() -> Ty {
    Ty::Extern("anyhow::Error".into())
}

/// What the file's `use krate::{a, b as c}` statements bring in: alias -> (crate, item), for the
/// bundled crates whose types are named that way (`use anyhow::Result;`).
pub(crate) fn crate_imports(src: &str) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    for krate in ["anyhow", "either", "once_cell", "hashbrown", "serde_json"] {
        let pat = format!("use {krate}::");
        let mut rest = src;
        while let Some(i) = rest.find(&pat) {
            let after = &rest[i + pat.len()..];
            let end = after.find(';').unwrap_or(after.len());
            let list = &after[..end];
            rest = &after[end..];
            let inner = list.trim().trim_start_matches('{').trim_end_matches('}');
            for part in inner.split(',') {
                let part = part.trim();
                // (`sync::Lazy`: the last segment.)
                let (item, alias) = match part.split_once(" as ") {
                    Some((i, a)) => (i.trim(), a.trim()),
                    None => (part, part),
                };
                let item = item.rsplit("::").next().unwrap_or(item);
                let alias = alias.rsplit("::").next().unwrap_or(alias);
                if !item.is_empty() && item != "*" && item != "self" {
                    out.insert(alias.to_string(), (krate.to_string(), item.to_string()));
                }
            }
        }
    }
    out
}

impl Checker<'_> {
    /// Whether a type path names `krate::item` (`anyhow::Result`, or `Result` after
    /// `use anyhow::Result;`).
    pub(super) fn names_crate_item(&self, p: &ast::Path, krate: &str, item: &str) -> bool {
        let text = p.syntax().text().to_string();
        let base = text.split('<').next().unwrap_or("").trim();
        if base == format!("{krate}::{item}") || base == format!("::{krate}::{item}") {
            return true;
        }
        self.crate_items
            .get(base)
            .is_some_and(|(k, i)| k == krate && i == item)
    }

    /// `anyhow::Error::msg(..)`, `Error::new(e)`, `Error::from(e)`, and what the `anyhow!` macro
    /// expands to.
    pub(super) fn anyhow_call(
        &mut self,
        c: &ast::CallExpr,
        names: &[&str],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        if !self.uses_crate("anyhow") {
            return None;
        }
        let tail: &[&str] = match names {
            ["anyhow", rest @ ..] => rest,
            all => all,
        };
        let is_error = |n: &str| {
            n == "Error" && names.first() == Some(&"anyhow")
                || self
                    .crate_items
                    .get(n)
                    .is_some_and(|(k, i)| k == "anyhow" && i == "Error")
        };
        let js = match tail {
            [e, "msg"] if is_error(e) => "anyhow.msg",
            [e, "new" | "from"] if is_error(e) => "anyhow.from",
            // (What `anyhow!(e)` expands to.)
            ["__wrap"] => "anyhow.wrap",
            _ => return None,
        };
        self.arg_count(c.syntax(), "function", 1, args.len());
        for a in args {
            self.infer(a, None);
        }
        self.note_crate(c.syntax(), js, vec![]);
        Some(anyhow_error())
    }

    /// `.context(..)` / `.with_context(|| ..)` on a `Result` or `Option`, and the methods of an
    /// `anyhow::Error`.
    pub(super) fn anyhow_method(
        &mut self,
        m: &ast::MethodCallExpr,
        resolved: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        if !self.uses_crate("anyhow") {
            return None;
        }
        let ctx_arg = |c2: &mut Self, lazy: bool| {
            c2.arg_count(m.syntax(), "method", 1, args.len());
            if let Some(a) = args.first() {
                if lazy {
                    let ret = c2.fresh(VarKind::General);
                    c2.check(
                        a,
                        &Ty::Closure {
                            params: vec![],
                            ret: Box::new(ret),
                        },
                    );
                } else {
                    c2.infer(a, None);
                }
            }
        };
        let err = anyhow_error();
        match (resolved, method) {
            (Ty::Result(t, _), "context" | "with_context") => {
                let lazy = method == "with_context";
                ctx_arg(self, lazy);
                let js = if lazy {
                    "anyhow.withContext"
                } else {
                    "anyhow.context"
                };
                self.note_crate(m.syntax(), js, vec![]);
                Some(Ty::Result(t.clone(), Box::new(err)))
            }
            (Ty::Option(t), "context" | "with_context") => {
                let lazy = method == "with_context";
                ctx_arg(self, lazy);
                let js = if lazy {
                    "anyhow.optWithContext"
                } else {
                    "anyhow.optContext"
                };
                self.note_crate(m.syntax(), js, vec![]);
                Some(Ty::Result(t.clone(), Box::new(err)))
            }
            (Ty::Extern(n), _) if n == "anyhow::Error" => {
                let (js, ret) = match method {
                    "context" => {
                        ctx_arg(self, false);
                        ("anyhow.errContext", err)
                    }
                    "chain" => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        (
                            "anyhow.chain",
                            Ty::Iter {
                                elem: Box::new(Ty::Dyn("Error".into())),
                                mutable: false,
                            },
                        )
                    }
                    "root_cause" => {
                        self.arg_count(m.syntax(), "method", 0, args.len());
                        ("anyhow.rootCause", Ty::Dyn("Error".into()))
                    }
                    _ => return None,
                };
                self.note_crate(m.syntax(), js, vec![]);
                Some(ret)
            }
            _ => None,
        }
    }
}

/// `R: Rng`, `impl Rng`, `&mut dyn RngCore`: a bound naming one of `rand`'s RNG traits.
pub(super) fn bounds_rng(list: &ast::TypeBoundList) -> bool {
    list.bounds().any(|b| {
        matches!(b.ty(), Some(ast::Type::PathType(p))
            if p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref()).is_some_and(|n| matches!(n.text().to_string().as_str(), "Rng" | "RngCore" | "CryptoRng")))
    })
}

/// `rand`'s RNG types (`ThreadRng`, `rngs::StdRng`).
pub(super) fn is_rng_type(name: &str) -> bool {
    matches!(
        name,
        "ThreadRng" | "StdRng" | "SmallRng" | "ChaCha8Rng" | "ChaCha12Rng" | "ChaCha20Rng"
    )
}
