//! Generic functions: type parameters, their trait bounds, instantiation at call sites, and what
//! a body may do with a value of type `T`. Generics are erased in JS, so a `T` only allows
//! operations that behave the same for every type it could be. A child module of `typeck`.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasGenericArgs, HasGenericParams, HasName, HasTypeBounds},
};

use super::{Checker, VarKind};
use crate::ty::Ty;

/// The trait bounds a type parameter may have.
pub(super) const KNOWN_BOUNDS: &[&str] = &[
    "Copy",
    "Clone",
    "Debug",
    "Display",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
    "Add",
    "Sub",
    "Mul",
    "Div",
    "Rem",
    "Neg",
    "AddAssign",
    "SubAssign",
    "MulAssign",
    "DivAssign",
    "RemAssign",
    "BitAnd",
    "BitOr",
    "BitXor",
    "Not",
    "Shl",
    "Shr",
    "BitAndAssign",
    "BitOrAssign",
    "BitXorAssign",
    "ShlAssign",
    "ShrAssign",
    "FromStr",
    "From",
    "TryFrom",
    "Sum",
    "Product",
    // (Marker traits: nothing to check in JS.)
    "Send",
    "Sync",
    "Unpin",
    "Sized",
    "UnwindSafe",
    "RefUnwindSafe",
    "BuildHasher",
    "Allocator",
    "ToString",
    // (`T: Into<f64>`: its target is recorded; serde's bounds are checked where JSON is read.)
    "Into",
    // (`S: AsRef<str>` that `-> Wrap<S>` keeps generic: `as_ref()` converts at run time.)
    "AsRef",
    "AsMut",
    "Borrow",
    "BorrowMut",
    "Serialize",
    "Deserialize",
    "DeserializeOwned",
    // (`E: Error`: `Debug + Display`, with `source()`.)
    "Error",
    // (`C: Extend<T>` / `C: FromIterator<T>`: by the value, or by `C`'s class.)
    "Extend",
    "FromIterator",
    // (`T: Any`: `&T as &dyn Any` checks the value by its shape, as concrete ones do.)
    "Any",
    // (`C: Index<Ix, Output = E>`: `c[ix]` indexes by what the value is, at run time.)
    "Index",
    "IndexMut",
];

/// A type parameter with the names of its bounds (`T: PartialOrd + Copy` → `["PartialOrd", "Copy"]`).
pub type TypeParam = (String, Vec<String>);

/// The std traits an `impl ..` / `dyn ..` of std's may name here (`x: impl Display`, `-> impl
/// Debug`, `&dyn Display`), with marker traits beside them.
const STD_IMPL_BOUNDS: &[&str] = &[
    "Display",
    "Debug",
    "ToString",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Clone",
    "Copy",
    "Default",
    "Send",
    "Sync",
    "Unpin",
];

/// The bounds of an `impl Display` / `dyn Ord + Debug` when they're all std formatting,
/// comparison and marker traits (or lifetimes), without the markers: `["Ord", "Debug"]`.
pub(super) fn std_impl_bounds(t: &ast::Type) -> Option<Vec<String>> {
    let list = match t {
        ast::Type::ImplTraitType(i) => i.type_bound_list()?,
        ast::Type::DynTraitType(d) => d.type_bound_list()?,
        _ => return None,
    };
    let mut out = Vec::new();
    for b in list.bounds() {
        if b.lifetime().is_some() {
            continue;
        }
        let ast::Type::PathType(p) = b.ty()? else {
            return None;
        };
        let seg = p.path()?.segment()?;
        let n = seg.name_ref()?.text().to_string();
        if !STD_IMPL_BOUNDS.contains(&n.as_str()) || seg.generic_arg_list().is_some() {
            return None;
        }
        if !matches!(n.as_str(), "Send" | "Sync" | "Unpin") {
            out.push(n);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Whether a `Ty::Dyn` names std traits (`Display`, `Ord+Debug`: see `std_impl_bounds`).
pub(super) fn is_std_dyn(tr: &str) -> bool {
    tr.split('+').all(|b| STD_IMPL_BOUNDS.contains(&b))
}

/// The std traits one implies: `Ord` gives `PartialOrd`, `Eq` and `PartialEq`, `Display` gives
/// `ToString` ..
pub(super) fn std_implies(have: &str, want: &str) -> bool {
    matches!(
        (have, want),
        ("Ord", "PartialOrd" | "Eq" | "PartialEq")
            | ("Eq", "PartialEq")
            | ("PartialOrd", "PartialEq")
            | ("Copy", "Clone")
            | ("Display", "ToString")
    )
}

/// The `generics` key of an argument-position `impl Display` (see `impl_arg_params`).
pub(super) fn impl_arg_key(t: &ast::Type) -> String {
    format!("impl@{}", u32::from(t.syntax().text_range().start()))
}

/// `x: impl Display` / `t: &impl Ord + Debug` in `f`'s parameters: an anonymous type parameter,
/// as rustc has it, named by its bounds (`impl_Display`, then `impl_Display_2` ..), with them
/// and its `generics` key.
pub(super) fn impl_arg_params(f: &ast::Fn) -> Vec<(String, Vec<String>, String)> {
    let mut out: Vec<(String, Vec<String>, String)> = Vec::new();
    for p in f.param_list().into_iter().flat_map(|l| l.params()) {
        let Some(ty) = p.ty() else { continue };
        for t in ty.syntax().descendants().filter_map(ast::Type::cast) {
            if !matches!(t, ast::Type::ImplTraitType(_)) {
                continue;
            }
            let Some(bounds) = std_impl_bounds(&t) else {
                continue;
            };
            // (A JS name: the generated code passes a parameter's formatters as `$dbg$<name>`.)
            let base = format!("impl_{}", bounds.join("_"));
            let same = out.iter().filter(|(_, b, _)| *b == bounds).count();
            let name = if same == 0 { base } else { format!("{base}_{}", same + 1) };
            out.push((name, bounds, impl_arg_key(&t)));
        }
    }
    out
}

/// Every type parameter of `f` with its bound lists (inline and from `where`).
fn type_params(f: &ast::Fn) -> Vec<(String, Vec<ast::TypeBoundList>, SyntaxNode)> {
    let mut out: Vec<(String, Vec<ast::TypeBoundList>, SyntaxNode)> = Vec::new();
    if let Some(gp) = f.generic_param_list() {
        for p in gp.generic_params() {
            let ast::GenericParam::TypeParam(tp) = p else {
                continue;
            };
            let Some(n) = tp.name() else { continue };
            out.push((
                n.text().to_string(),
                tp.type_bound_list().into_iter().collect(),
                tp.syntax().clone(),
            ));
        }
    }
    if let Some(wc) = f.where_clause() {
        for pred in wc.predicates() {
            let name = match pred.ty() {
                Some(ast::Type::PathType(pt)) => pt
                    .path()
                    .and_then(|p| p.as_single_name_ref())
                    .map(|n| n.text().to_string()),
                _ => None,
            };
            if let (Some(n), Some(b)) = (name, pred.type_bound_list())
                && let Some(entry) = out.iter_mut().find(|(x, _, _)| *x == n)
            {
                entry.1.push(b);
            }
        }
    }
    out
}

impl Checker<'_> {
    /// The bounds of `f`'s type parameters that take type arguments and are checked at a
    /// turbofish: `R: Into<i64>`, `R: From<u8>`, `R: PartialEq<u8>`, `F: Fn(i32) -> i32`. Those
    /// whose arguments name a type parameter are left out.
    pub(super) fn arg_bounds(&mut self, f: &ast::Fn) -> Vec<(String, crate::adt::ArgBound)> {
        use crate::adt::ArgBound;
        let mut out = Vec::new();
        let tps = type_params(f);
        let names: Vec<String> = tps.iter().map(|(n, _, _)| n.clone()).collect();
        // (`R: Into<T>` with the method's `T`: not lowered here.)
        let names_param = |s: &SyntaxNode| {
            s.descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .any(|t| {
                    t.kind() == ra_ap_syntax::SyntaxKind::IDENT
                        && names.iter().any(|n| n == t.text())
                })
        };
        for (n, lists, _) in tps.iter().cloned() {
            for l in &lists {
                if names_param(l.syntax()) {
                    continue;
                }
                if let Some(c) = self.lower_fn_bounds(l)
                    && !mentions_param(&c)
                {
                    out.push((n.clone(), ArgBound::Fn(c)));
                }
                for b in l.bounds() {
                    let Some(ast::Type::PathType(p)) = b.ty() else {
                        continue;
                    };
                    let Some(seg) = p.path().and_then(|p| p.segment()) else {
                        continue;
                    };
                    let Some(tr) = seg.name_ref().map(|n| n.text().to_string()) else {
                        continue;
                    };
                    if !matches!(tr.as_str(), "Into" | "From" | "PartialEq") {
                        continue;
                    }
                    let Some(ast::GenericArg::TypeArg(ta)) = seg
                        .generic_arg_list()
                        .and_then(|l| l.generic_args().next())
                    else {
                        continue;
                    };
                    let Some(t) = ta.ty() else { continue };
                    let t = self.lower_type(&t);
                    if !mentions_param(&t) {
                        out.push((n.clone(), ArgBound::Conv(tr, t)));
                    }
                }
            }
        }
        out
    }

    /// Whether a type argument `t` (settled) meets a turbofish's `ArgBound`; `None` when it
    /// can't tell (only std's numbers, `bool`, `char`, strings and closures are judged).
    pub(super) fn arg_bound_holds(&self, t: &Ty, b: &crate::adt::ArgBound) -> Option<bool> {
        use crate::adt::ArgBound;
        let scalar = |t: &Ty| {
            matches!(
                t,
                Ty::Int(_) | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str
            )
        };
        match b {
            ArgBound::Fn(c) => match t {
                Ty::Closure { .. } => Some(self.could_be(t, c)),
                _ if scalar(t) => Some(false),
                _ => None,
            },
            ArgBound::Conv(tr, x) => {
                if !scalar(t) || !scalar(x) {
                    return None;
                }
                Some(match tr.as_str() {
                    "Into" => std_from(x, t),
                    "From" => std_from(t, x),
                    _ => t == x,
                })
            }
        }
    }
}

fn mentions_param(t: &Ty) -> bool {
    let mut ps = Vec::new();
    t.collect_params(&mut ps);
    !ps.is_empty()
}

/// Whether std has `impl From<src> for dst` (or they're the same), for numbers, `bool`, `char`
/// and strings.
fn std_from(dst: &Ty, src: &Ty) -> bool {
    use crate::ty::IntKind::*;
    if dst == src {
        return true;
    }
    match (dst, src) {
        (Ty::Str, Ty::Char) => true,
        (Ty::Int(_), Ty::Bool) => true,
        (Ty::Char, Ty::Int(U8)) => true,
        (Ty::Int(U32 | U64 | U128), Ty::Char) => true,
        (Ty::Float { f32: false }, Ty::Float { f32: true }) => true,
        (Ty::Float { f32 }, Ty::Int(k)) => {
            let max = if *f32 { 16 } else { 32 };
            !matches!(k, Isize | Usize) && k.bits() <= max
        }
        (Ty::Int(Usize), Ty::Int(k)) => matches!(k, U8 | U16),
        (Ty::Int(Isize), Ty::Int(k)) => matches!(k, I8 | I16 | U8),
        (Ty::Int(_), Ty::Int(Isize | Usize)) => false,
        (Ty::Int(d), Ty::Int(s)) => {
            if d.is_signed() == s.is_signed() {
                d.bits() > s.bits()
            } else {
                d.is_signed() && d.bits() > s.bits()
            }
        }
        _ => false,
    }
}

/// Each type parameter of `f` with the names of its trait bounds that take no type arguments
/// (`R: Default + Clone`, not `F: Fn(u8)` or `S: Into<String>`).
pub(super) fn plain_bounds(f: &ast::Fn) -> Vec<(String, Vec<String>)> {
    type_params(f)
        .into_iter()
        .map(|(n, lists, _)| {
            let plain = bound_names(&lists)
                .into_iter()
                .filter(|(_, b)| {
                    ast::TypeBound::cast(b.clone())
                        .and_then(|b| b.ty())
                        .and_then(|t| match t {
                            ast::Type::PathType(p) => p.path()?.segment(),
                            _ => None,
                        })
                        .is_some_and(|s| s.generic_arg_list().is_none())
                })
                .map(|(b, _)| b)
                .collect();
            (n, plain)
        })
        .collect()
}

/// For each type parameter, how many others its bounds wait on: 0 when they name no other
/// parameter, else one more than the deepest one they name (`F: FnOnce() -> P` waits on `P`,
/// whatever `P`'s own bounds are). Lowering in this order sees each named parameter first.
pub(super) fn mention_depths(
    params: &[(String, Vec<ast::TypeBoundList>)],
) -> HashMap<String, usize> {
    let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
    let mentions: Vec<Vec<usize>> = params
        .iter()
        .map(|(n, lists)| {
            let mut out = Vec::new();
            for l in lists {
                for t in l
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|e| e.into_token())
                {
                    if t.kind() == ra_ap_syntax::SyntaxKind::IDENT
                        && t.text() != n.as_str()
                        && let Some(i) = names.iter().position(|x| *x == t.text())
                        && !out.contains(&i)
                    {
                        out.push(i);
                    }
                }
            }
            out
        })
        .collect();
    let mut depth = vec![0usize; params.len()];
    // (A cycle can't be ordered: it stops growing after a pass per parameter.)
    for _ in 0..params.len() {
        for i in 0..params.len() {
            let d = mentions[i]
                .iter()
                .map(|&j| depth[j] + 1)
                .max()
                .unwrap_or(0)
                .min(params.len());
            depth[i] = d;
        }
    }
    params.iter().map(|(n, _)| n.clone()).zip(depth).collect()
}

/// The last path segment of each trait bound (`std::fmt::Display` → `Display`); `?Sized` and
/// lifetimes are skipped.
pub(super) fn bound_names(lists: &[ast::TypeBoundList]) -> Vec<(String, SyntaxNode)> {
    let mut out = Vec::new();
    for l in lists {
        for b in l.bounds() {
            if b.question_mark_token().is_some() {
                continue;
            }
            if let Some(ast::Type::PathType(p)) = b.ty()
                && let Some(seg) = p.path().and_then(|p| p.segment())
                && let Some(n) = seg.name_ref()
            {
                out.push((n.text().to_string(), b.syntax().clone()));
            }
        }
    }
    out
}

/// The parameter or projection a `where` predicate's subject names: `I::Item` for `I::Item`,
/// `<I as IntoIterator>::Item` and `<I::Item as IntoIterator>::Item` alike (`I::Item::Item`).
fn where_subject(t: &ast::Type) -> Option<String> {
    let ast::Type::PathType(pt) = t else {
        return None;
    };
    let p = pt.path()?;
    if let Some(n) = p.as_single_name_ref() {
        return Some(n.text().to_string());
    }
    let q = super::proj_qual(&p)?;
    Some(format!("{q}::{}", p.segment()?.name_ref()?.text()))
}

/// What iterating a std value by `into_iter()` yields: a `Vec<T>`'s `T`, a map's `(K, V)`.
fn std_into_iter_item(t: &Ty) -> Option<Ty> {
    match t {
        Ty::Vec(e) | Ty::Set(e) | Ty::Option(e) | Ty::IntoIter(e) | Ty::Iter { elem: e, .. } => {
            Some((**e).clone())
        }
        Ty::Map(k, v) => Some(Ty::Tuple(vec![(**k).clone(), (**v).clone()])),
        _ => None,
    }
}

/// Replace type parameters per `m`.
pub fn subst(t: &Ty, m: &HashMap<String, Ty>) -> Ty {
    let s = |x: &Ty| Box::new(subst(x, m));
    match t {
        // `T::Item` follows `T`: `P::Item` when `T` is now the parameter `P`, unchecked when it's
        // a concrete type or not known yet (the impl's `type Item` isn't looked up here).
        Ty::Param(n) if !m.contains_key(n) && n.contains("::") => {
            let (base, assoc) = n.split_once("::").unwrap_or_default();
            match m.get(base) {
                Some(Ty::Param(p)) => Ty::Param(format!("{p}::{assoc}")),
                // (`I::Item` with `I` an iterator of `u8`.)
                Some(Ty::Iter { elem, .. } | Ty::IntoIter(elem)) if assoc == "Item" => {
                    (**elem).clone()
                }
                // (`L::Item` / `L::IntoIter` of an `L: IntoIterator` that's a `Vec<u32>`, and
                // `H::Item::Item` of an iterator of `Vec`s.)
                Some(b) => {
                    let mut cur = b.clone();
                    for a in assoc.split("::") {
                        cur = match (a, std_into_iter_item(&cur)) {
                            ("Item", Some(item)) => item,
                            ("IntoIter", Some(item)) => Ty::Iter {
                                elem: Box::new(item),
                                mutable: false,
                            },
                            _ => return Ty::Unknown,
                        };
                    }
                    cur
                }
                None => t.clone(),
            }
        }
        Ty::Param(n) => m.get(n).cloned().unwrap_or_else(|| t.clone()),
        Ty::Vec(e) => Ty::Vec(s(e)),
        Ty::Option(e) => Ty::Option(s(e)),
        Ty::Set(e) => Ty::Set(s(e)),
        Ty::Coll(k, e) => Ty::Coll(*k, s(e)),
        Ty::Future(e) => Ty::Future(s(e)),
        Ty::Map(k, v) => Ty::Map(s(k), s(v)),
        Ty::Entry(k, v) => Ty::Entry(s(k), s(v)),
        Ty::Result(k, v) => Ty::Result(s(k), s(v)),
        Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| subst(e, m)).collect()),
        Ty::Adt(n, args) => Ty::Adt(n.clone(), args.iter().map(|a| subst(a, m)).collect()),
        Ty::Dyn(n, args) => Ty::Dyn(n.clone(), args.iter().map(|a| subst(a, m)).collect()),
        Ty::RefCell(x) => Ty::RefCell(s(x)),
        Ty::MutRef(x) => Ty::MutRef(s(x)),
        Ty::Iter { elem, mutable } => Ty::Iter {
            elem: s(elem),
            mutable: *mutable,
        },
        Ty::IntoIter(e) => Ty::IntoIter(s(e)),
        Ty::Range(e, incl) => Ty::Range(s(e), *incl),
        Ty::Closure { params, ret } => Ty::Closure {
            params: params.iter().map(|p| subst(p, m)).collect(),
            ret: s(ret),
        },
        other => other.clone(),
    }
}

/// Whether the type parameter `name` appears in `f`'s signature only as a whole parameter type
/// (`s: S`, `s: &S`, `s: &mut S`), so it can be taken as its conversion bound's target.
pub(super) fn conv_param_plain(f: &ast::Fn, name: &str) -> bool {
    let named = |t: &ast::PathType| {
        t.path()
            .is_some_and(|p| p.qualifier().is_none() && p.syntax().text() == name)
    };
    let mut types: Vec<ast::Type> = f
        .param_list()
        .into_iter()
        .flat_map(|l| l.params())
        .filter_map(|p| p.ty())
        .collect();
    types.extend(f.ret_type().and_then(|r| r.ty()));
    types.iter().all(|t| {
        t.syntax().descendants().filter_map(ast::PathType::cast).filter(named).all(|pt| {
            let top = match pt.syntax().parent().and_then(ast::RefType::cast) {
                Some(r) => r.syntax().clone(),
                None => pt.syntax().clone(),
            };
            top.parent().is_some_and(|p| ast::Param::can_cast(p.kind()))
        })
    })
}

impl Checker<'_> {
    /// A function's written return type. `-> impl Debug` (std's formatting and comparison
    /// traits) is the body's own type, as in rustc, so `{:?}` prints a tuple as one: a type
    /// variable the body binds.
    pub(super) fn lower_ret_type(&mut self, t: &ast::Type) -> Ty {
        if matches!(t, ast::Type::ImplTraitType(_)) && std_impl_bounds(t).is_some() {
            return self.fresh(VarKind::General);
        }
        self.lower_type(t)
    }

    /// The type parameters of `f`: `Fn`-bounded ones as closure types, the rest as `Ty::Param`.
    /// Sets `self.generics` and `self.param_bounds` for lowering the signature or checking the
    /// body. Unsupported bounds are reported when `report` is set (once per function).
    pub(super) fn enter_generics(&mut self, f: &ast::Fn, report: bool) -> Vec<TypeParam> {
        self.generics.clear();
        self.param_bounds.clear();
        self.param_trait_args.clear();
        self.param_trait_alts.clear();
        self.op_args.clear();
        self.cmp_args.clear();
        self.lenient_projs.clear();
        self.slice_index_params.clear();
        self.const_tys.clear();
        self.from_args.clear();
        self.try_from_args.clear();
        self.try_error_bounds.clear();
        self.borrow_args.clear();
        let mut params = Vec::new();
        // Bounds that name other parameters come after the plain ones: `Fut: Future<Output = T>`,
        // then `F: Fn() -> Fut` / `S: Into<T>`, then `I: IntoIterator<Item = P>`. (Rust takes
        // `<F: FnOnce() -> T, T>` and `where F: FnOnce() -> T` in any order.)
        let mut tps = type_params(f);
        // (First by what the bounds name: `F: FnOnce() -> P` after `P: Into<PathBuf>`.)
        let depths = mention_depths(
            &tps.iter()
                .map(|(n, l, _)| (n.clone(), l.clone()))
                .collect::<Vec<_>>(),
        );
        tps.sort_by_key(|(name, lists, _)| {
            let depth = depths.get(name).copied().unwrap_or(0);
            let kind = if lists.iter().any(|l| {
                super::bounds_into_iter_item(l).is_some()
                    || super::bounds_iterator_item(l).is_some()
            }) {
                3
            } else if lists
                .iter()
                .any(|l| super::bounds_future_output(l).is_some())
            {
                1
            } else if bound_names(lists)
                .iter()
                .any(|(b, _)| matches!(b.as_str(), "Fn" | "FnMut" | "FnOnce"))
                || lists.iter().any(|l| super::conversion_target(l).is_some())
            {
                2
            } else {
                0
            };
            (depth, kind)
        });
        for (name, lists, node) in tps {
            // `Fut: Future<Output = T>`: a future of `T`, like `impl Future<Output = T>`.
            if let Some(out) = lists.iter().find_map(super::bounds_future_output) {
                let out = match out {
                    Some(t) => self.lower_type(&t),
                    // `F: Future`: its output is a type parameter of its own, `F::Output`.
                    None => {
                        let out = format!("{name}::Output");
                        self.generics.insert(out.clone(), Ty::Param(out.clone()));
                        self.param_bounds.insert(out.clone(), Vec::new());
                        params.push((out.clone(), Vec::new()));
                        Ty::Param(out)
                    }
                };
                self.generics.insert(name, Ty::Future(Box::new(out)));
                continue;
            }
            if let Some(closure) = lists.iter().find_map(|l| self.lower_fn_bounds(l)) {
                self.generics.insert(name, closure);
                continue;
            }
            // `S: Into<String>` / `S: AsRef<str>`: the target type, like `impl Into<String>`.
            // (Unless `S` is named elsewhere in the signature, `-> Wrap<S>`, `&[S]`: then it
            // stays a parameter, the caller's own type, and `as_ref()` / `into()` convert it
            // where they're called.)
            if let Some(target) = lists.iter().find_map(super::conversion_target) {
                let t = self.lower_type(&target);
                // `T: Into<f64>` stays a parameter (any number type converts); `.into()` makes
                // the target.
                if matches!(t, Ty::Int(_) | Ty::Float { .. }) {
                    self.into_targets.insert(name.clone(), t);
                } else if conv_param_plain(f, &name) {
                    self.generics.insert(name, t);
                    continue;
                }
            }
            // `H: Hasher` in an `impl Hash`.
            if lists.iter().any(super::bounds_hasher) {
                self.generics.insert(name, Ty::Hasher);
                continue;
            }
            // `R: Rng` (the bundled `rand`).
            if lists.iter().any(super::crates::bounds_rng) {
                self.generics.insert(name, Ty::Rng);
                continue;
            }
            // `I: IntoIterator<Item = T>`: any iterable of `T`.
            if let Some(item) = lists.iter().find_map(super::bounds_into_iter_item) {
                let elem = match item {
                    Some(t) => self.lower_type(&t),
                    None => self.item_param(f, &name, report, &mut params),
                };
                // (`-> I::IntoIter`: the runtime's iterator it makes.)
                self.generics.insert(
                    format!("{name}::IntoIter"),
                    Ty::Iter {
                        elem: Box::new(elem.clone()),
                        mutable: false,
                    },
                );
                self.generics.insert(name, Ty::IntoIter(Box::new(elem)));
                continue;
            }
            // `I: Iterator<Item = T>`: the runtime's iterator, like `impl Iterator<Item = T>`.
            if let Some(item) = lists.iter().find_map(|l| {
                super::bounds_iterator_item(l).or_else(|| self.iter_trait_item(l))
            }) {
                let elem = match item {
                    Some(t) => self.lower_type(&t),
                    None => self.item_param(f, &name, report, &mut params),
                };
                let it = Ty::Iter {
                    elem: Box::new(elem),
                    mutable: false,
                };
                self.generics.insert(name, it);
                continue;
            }
            let mut bounds = Vec::new();
            for (b, bnode) in bound_names(&lists) {
                if KNOWN_BOUNDS.contains(&b.as_str()) || self.adts.traits.contains_key(&b) {
                    bounds.push(b);
                } else if report {
                    self.error(
                        &bnode,
                        format!("the trait bound `{b}` is not supported yet on type parameters (supported: the comparison, formatting, `Clone`/`Copy`/`Default`/`Hash`, arithmetic and bitwise operator, `From`/`TryFrom`/`FromStr`, `AsRef`/`AsMut`, `Sum`/`Product`, iterator and closure traits, `Send`/`Sync`/`UnwindSafe`, and traits declared in the file)"),
                    );
                }
            }
            let _ = node;
            self.generics.insert(name.clone(), Ty::Param(name.clone()));
            self.param_bounds.insert(name.clone(), bounds.clone());
            params.push((name, bounds));
        }
        // `const N: usize`: a value in the body, and a type argument (`Buf<N>`).
        let mut order: Vec<String> = Vec::new();
        for p in f
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
        {
            match p {
                ast::GenericParam::ConstParam(cp) => {
                    let Some(n) = cp.name() else { continue };
                    let n = n.text().to_string();
                    let ty = self.const_param_ty(&cp, report);
                    self.generics.insert(n.clone(), Ty::Param(n.clone()));
                    self.const_tys.insert(n.clone(), ty);
                    params.push((n.clone(), Vec::new()));
                    order.push(n);
                }
                ast::GenericParam::TypeParam(tp) => {
                    if let Some(n) = tp.name() {
                        order.push(n.text().to_string());
                    }
                }
                ast::GenericParam::LifetimeParam(_) => {}
            }
        }
        // `x: impl Display`: after the written ones (no turbofish names it).
        for (name, bounds, key) in impl_arg_params(f) {
            self.generics.insert(key, Ty::Param(name.clone()));
            self.generics.insert(name.clone(), Ty::Param(name.clone()));
            self.param_bounds.insert(name.clone(), bounds.clone());
            order.push(name.clone());
            params.push((name, bounds));
        }
        params.sort_by_key(|(n, _)| order.iter().position(|x| x == n));
        // The bounds' arguments (`C: Convert<String>`, `S: Shape<Unit = f64>`), once every
        // parameter is in scope.
        for (name, lists, _) in type_params(f) {
            self.record_from_bound(&name, &lists, false);
            self.record_op_bounds(&name, &lists);
            if self.param_bounds.contains_key(&name) {
                self.record_bound_args(&name, &lists);
            }
        }
        self.record_concrete_borrow_bounds(f.where_clause());
        self.note_where_projections(f.where_clause());
        // (`where &'a I: IntoIterator<Item = E>`: `a.into_iter()` with an `a: &I`.)
        let wcs: Vec<ast::WhereClause> = f.where_clause().into_iter().collect();
        self.note_ref_into_iter_bounds(&wcs);
        for (n, bounds) in params.iter_mut() {
            if let Some(b) = self.param_bounds.get(n.as_str())
                && b.len() > bounds.len()
            {
                *bounds = b.clone();
            }
        }
        params
    }

    /// `I: IntoIterator` / `I: Iterator` without `Item = ..`: its items are a type parameter of
    /// their own, `I::Item` (a fresh type per call), with the bounds `where I::Item: ..` gives.
    fn item_param(
        &mut self,
        f: &ast::Fn,
        name: &str,
        report: bool,
        params: &mut Vec<TypeParam>,
    ) -> Ty {
        let item = format!("{name}::Item");
        let mut lists = Vec::new();
        for pred in f.where_clause().into_iter().flat_map(|w| w.predicates()) {
            if let Some(t) = pred.ty()
                && where_subject(&t).as_deref() == Some(item.as_str())
                && let Some(l) = pred.type_bound_list()
            {
                lists.push(l);
            }
        }
        // `where I::Item: Into<f64>`.
        if let Some(target) = lists.iter().find_map(super::conversion_target) {
            let t = self.lower_type(&target);
            if matches!(t, Ty::Int(_) | Ty::Float { .. }) {
                self.into_targets.insert(item.clone(), t);
            }
        }
        let mut bounds = Vec::new();
        for (b, bnode) in bound_names(&lists) {
            if KNOWN_BOUNDS.contains(&b.as_str()) || self.adts.traits.contains_key(&b) {
                bounds.push(b);
            } else if b == "IntoIterator" {
                // (`where H::Item: IntoIterator`: see below.)
            } else if report {
                self.error(
                    &bnode,
                    format!("the trait bound `{b}` is not supported yet on type parameters"),
                );
            }
        }
        self.generics.insert(item.clone(), Ty::Param(item.clone()));
        self.param_bounds.insert(item.clone(), bounds.clone());
        // (`where I::Item: Tr<E>`: `E`, for `Tr::E` in its methods.)
        self.record_bound_args(&item, &lists);
        // (`where H::Item: IntoIterator`: `it.into_iter()` on each item.)
        let wcs: Vec<ast::WhereClause> = f.where_clause().into_iter().collect();
        self.note_into_iter_param(&item, &lists, &wcs);
        let bounds = self.param_bounds[&item].clone();
        params.push((item.clone(), bounds));
        Ty::Param(item)
    }

    /// `L: IntoIterator` on a type parameter that stays one (the impl's, bounded inline, in its
    /// `where` or in a method's; `where &'a I: IntoIterator`; `where H::Item: IntoIterator`):
    /// `x.into_iter()` makes the runtime's iterator of `L::Item` (`κσ.$intoIter`), and `L::Item`
    /// / `L::IntoIter` name its items and that iterator. `wcs` are the `where` clauses in scope,
    /// for `where L::Item: Clone`.
    pub(super) fn note_into_iter_param(
        &mut self,
        name: &str,
        lists: &[ast::TypeBoundList],
        wcs: &[ast::WhereClause],
    ) -> bool {
        let Some(item) = lists.iter().find_map(super::bounds_into_iter_item) else {
            return false;
        };
        let key = format!("{name}::Item");
        let elem = match item {
            Some(t) => self.lower_type(&t),
            None => match self.generics.get(&key) {
                Some(t) => t.clone(),
                None => {
                    let item_lists: Vec<ast::TypeBoundList> = wcs
                        .iter()
                        .flat_map(|w| w.predicates())
                        .filter(|pr| {
                            pr.ty()
                                .is_some_and(|t| where_subject(&t).as_deref() == Some(key.as_str()))
                        })
                        .filter_map(|pr| pr.type_bound_list())
                        .collect();
                    // (`where <I::Item as IntoIterator>::Item: Into<usize>`.)
                    if let Some(target) = item_lists.iter().find_map(super::conversion_target) {
                        let t = self.lower_type(&target);
                        if matches!(t, Ty::Int(_) | Ty::Float { .. }) {
                            self.into_targets.insert(key.clone(), t);
                        }
                    }
                    let bounds: Vec<String> = bound_names(&item_lists)
                        .into_iter()
                        .map(|(b, _)| b)
                        .filter(|b| {
                            KNOWN_BOUNDS.contains(&b.as_str()) || self.adts.traits.contains_key(b)
                        })
                        .collect();
                    self.param_bounds.insert(key.clone(), bounds);
                    self.record_bound_args(&key, &item_lists);
                    Ty::Param(key.clone())
                }
            },
        };
        self.generics.insert(key, elem.clone());
        self.generics.insert(
            format!("{name}::IntoIter"),
            Ty::Iter {
                elem: Box::new(elem),
                mutable: false,
            },
        );
        let b = self.param_bounds.entry(name.to_string()).or_default();
        if !b.iter().any(|x| x == "IntoIterator") {
            b.push("IntoIterator".into());
        }
        true
    }

    /// `where &'a I: IntoIterator<Item = E>` / `for<'a> &'a L: IntoIterator` (a reference to a
    /// type parameter as the subject): iterating `&I` is iterating `I` in JS, so `I` takes the
    /// bound (`note_into_iter_param`).
    pub(super) fn note_ref_into_iter_bounds(&mut self, wcs: &[ast::WhereClause]) {
        for pred in wcs.iter().flat_map(|w| w.predicates()) {
            let Some(ast::Type::RefType(r)) = pred.ty() else {
                continue;
            };
            let Some(ast::Type::PathType(pt)) = r.ty() else {
                continue;
            };
            let Some(n) = pt.path().and_then(|p| p.as_single_name_ref()) else {
                continue;
            };
            let n = n.text().to_string();
            if self.generics.get(&n) != Some(&Ty::Param(n.clone())) {
                continue;
            }
            let lists: Vec<ast::TypeBoundList> = pred.type_bound_list().into_iter().collect();
            self.note_into_iter_param(&n, &lists, wcs);
        }
    }

    /// The associated types of each type parameter's bounds, in order: (parameter, trait,
    /// associated type).
    pub(super) fn proj_names(&self, type_params: &[TypeParam]) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for (p, bounds) in type_params {
            for b in bounds {
                if let Some(d) = self.adts.traits.get(b) {
                    for a in &d.assoc_types {
                        out.push((p.clone(), b.clone(), a.clone()));
                    }
                }
            }
        }
        out
    }

    /// `I: SliceIndex<[T]>` (smallvec's, thin-vec's `impl<T, I: SliceIndex<[T]>> Index<I>`):
    /// `slice[i]` is an element or a part by what `i` is, and `I::Output` is left unchecked
    /// (`Output = [T]` when written: a part).
    pub(super) fn record_slice_index_bound(&mut self, name: &str, lists: &[ast::TypeBoundList]) {
        use ra_ap_syntax::ast::HasGenericArgs;
        if self.adts.traits.contains_key("SliceIndex") {
            return;
        }
        for b in lists.iter().flat_map(|l| l.bounds()) {
            let Some(ast::Type::PathType(pt)) = b.ty() else {
                continue;
            };
            let Some(seg) = pt.path().and_then(|p| p.segment()) else {
                continue;
            };
            if !seg.name_ref().is_some_and(|n| n.text() == "SliceIndex") {
                continue;
            }
            let mut elem = None;
            let mut out = None;
            for g in seg
                .generic_arg_list()
                .into_iter()
                .flat_map(|l| l.generic_args())
            {
                match g {
                    ast::GenericArg::TypeArg(ta) => {
                        if let Some(ast::Type::SliceType(st)) = ta.ty()
                            && let Some(t) = st.ty()
                        {
                            elem = Some(self.lower_type(&t));
                        }
                    }
                    ast::GenericArg::AssocTypeArg(a)
                        if a.name_ref().is_some_and(|n| n.text() == "Output") =>
                    {
                        out = a.ty().map(|t| self.lower_type(&t));
                    }
                    _ => {}
                }
            }
            if let Some(elem) = elem {
                self.lenient_projs.insert(format!("{name}::Output"));
                self.slice_index_params.insert(name.to_string(), (elem, out));
            }
        }
    }

    /// Whether `e` names a local of a type parameter bounded `SliceIndex<[T]>`.
    pub(super) fn slice_index_arg(&self, e: &ast::Expr) -> bool {
        let ast::Expr::PathExpr(p) = super::strip_parens(e) else {
            return false;
        };
        p.path()
            .and_then(|p| p.as_single_name_ref())
            .and_then(|n| self.lookup(&n.text().to_string()))
            .is_some_and(|l| {
                matches!(self.shallow(&l.ty), Ty::Param(p) if self.slice_index_params.contains_key(&p))
            })
    }

    /// `slice[i]` (or `Index::index(slice, i)`, at `node`) of a `bt` slice by an `it` of a type
    /// parameter bounded `SliceIndex<[T]>`: `I::Output` (or the bound's `Output = ..`), indexed
    /// at run time by what `i` is (`κσ.$sliceIndex`). None for other indexes.
    pub(super) fn slice_param_index(&mut self, node: &SyntaxNode, bt: &Ty, it: &Ty) -> Option<Ty> {
        let Ty::Param(p) = self.shallow(it) else {
            return None;
        };
        let (elem, out) = self.slice_index_params.get(&p).cloned()?;
        match self.shallow(bt) {
            Ty::Vec(e) => {
                self.unify(&e, &elem);
            }
            Ty::Unknown | Ty::Var(_) => {}
            _ => return None,
        }
        let r = self.range(node);
        self.slice_param_indexes.insert(r);
        Some(out.unwrap_or(Ty::Param(format!("{p}::Output"))))
    }

    /// `T: From<X>`: `X`, for `T::from(x)` (and `T: Borrow<X>`, for a map lookup by `X`; only that
    /// one with `borrow_only`, for a method's `where` clause on its impl's parameter).
    pub(super) fn record_from_bound(
        &mut self,
        name: &str,
        lists: &[ast::TypeBoundList],
        borrow_only: bool,
    ) {
        use ra_ap_syntax::ast::HasGenericArgs;
        if !borrow_only && matches!(self.generics.get(name), Some(Ty::Param(_))) {
            self.record_slice_index_bound(name, lists);
        }
        for l in lists {
            for b in l.bounds() {
                if let Some(ast::Type::PathType(pt)) = b.ty()
                    && let Some(seg) = pt.path().and_then(|p| p.segment())
                    && let Some(tr) = seg.name_ref().map(|n| n.text().to_string())
                    && matches!(tr.as_str(), "From" | "TryFrom" | "Borrow")
                    && (!borrow_only || tr == "Borrow")
                    && let Some(ast::GenericArg::TypeArg(ta)) =
                        seg.generic_arg_list().and_then(|l| l.generic_args().next())
                    && let Some(t) = ta.ty()
                {
                    let x = self.lower_type(&t);
                    if tr == "Borrow" {
                        self.borrow_args.push((Ty::Param(name.to_string()), x));
                    } else if tr == "From" {
                        // (`E: From<E1> + From<E4>`: a call or `?` picks by its argument.)
                        let xs = self.from_args.entry(name.to_string()).or_default();
                        if !xs.contains(&x) {
                            xs.push(x);
                        }
                    } else {
                        // (`T: TryFrom<u64> + TryFrom<i64>`: a call picks by its argument;
                        // `T::Error` is left unchecked, but for `Debug` / `Display`, which
                        // `param_has` answers from a `where T::Error: Debug` or, kept under the
                        // key `T::Error`, a binding `TryFrom<X, Error = E>`.)
                        self.try_from_args
                            .entry(name.to_string())
                            .or_default()
                            .push(x);
                        self.lenient_projs.insert(format!("{name}::Error"));
                        let bound_error = seg.generic_arg_list().into_iter().flat_map(|l| l.generic_args()).find_map(|g| match g {
                            ast::GenericArg::AssocTypeArg(a)
                                if a.name_ref().is_some_and(|n| n.text() == "Error") =>
                            {
                                a.ty()
                            }
                            _ => None,
                        });
                        if let Some(e) = bound_error {
                            let e = self.lower_type(&e);
                            self.try_from_args.insert(format!("{name}::Error"), vec![e]);
                        }
                    }
                }
            }
        }
    }

    /// `where String: Borrow<Q>` (a concrete type, not a parameter, as the subject): a map of
    /// `String` keys is looked up by a `&Q` (`self.map.get(q)`).
    pub(super) fn record_concrete_borrow_bounds(&mut self, wc: Option<ast::WhereClause>) {
        use ra_ap_syntax::ast::HasGenericArgs;
        for pred in wc.into_iter().flat_map(|w| w.predicates()) {
            let Some(subject) = pred.ty() else { continue };
            // (Your struct or enum, for the comparison and operator bounds below.)
            let own_adt = match &subject {
                ast::Type::PathType(pt) => pt
                    .path()
                    .and_then(|p| p.as_single_name_ref())
                    .is_some_and(|n| self.adts.defs.contains_key(&n.text().to_string())),
                _ => false,
            };
            if let ast::Type::PathType(pt) = &subject
                && let Some(n) = pt.path().and_then(|p| p.as_single_name_ref())
                && matches!(self.generics.get(&n.text().to_string()), Some(Ty::Param(_)))
            {
                continue;
            }
            for b in pred.type_bound_list().into_iter().flat_map(|l| l.bounds()) {
                // (`where Ident: PartialEq<I>`: `id == i` with an `id: Ident`, as for a parameter.)
                if let Some(ast::Type::PathType(pt)) = b.ty()
                    && let Some(seg) = pt.path().and_then(|p| p.segment())
                    && let Some(tr) = seg.name_ref().map(|n| n.text().to_string())
                    && matches!(tr.as_str(), "PartialEq" | "PartialOrd")
                    && own_adt
                    && let Ty::Adt(n, _) = self.lower_type(&subject)
                {
                    self.record_cmp_bound(&n, &tr, &seg);
                    continue;
                }
                // (`where TextSize: Add<A, Output = TextSize>`: `TextSize + a` with an `a: A` calls
                // the type's operator, picked at run time by the operand.)
                if let Some(ast::Type::PathType(pt)) = b.ty()
                    && let Some(seg) = pt.path().and_then(|p| p.segment())
                    && let Some(tr) = seg.name_ref().map(|n| n.text().to_string())
                    && crate::adt::op_method(&tr).is_some()
                    && own_adt
                    && let Ty::Adt(n, _) = self.lower_type(&subject)
                {
                    for g in seg.generic_arg_list().into_iter().flat_map(|l| l.generic_args()) {
                        match g {
                            ast::GenericArg::TypeArg(ta) => {
                                if let Some(t) = ta.ty() {
                                    let t = self.lower_type(&t);
                                    self.op_args.insert((n.clone(), tr.clone()), t);
                                }
                            }
                            ast::GenericArg::AssocTypeArg(a)
                                if a.name_ref().is_some_and(|x| x.text() == "Output") =>
                            {
                                if let Some(t) = a.ty() {
                                    let t = self.lower_type(&t);
                                    self.op_args.insert((n.clone(), format!("{tr}$out")), t);
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
                if let Some(ast::Type::PathType(pt)) = b.ty()
                    && let Some(seg) = pt.path().and_then(|p| p.segment())
                    && seg.name_ref().is_some_and(|n| n.text() == "Borrow")
                    && let Some(ast::GenericArg::TypeArg(ta)) =
                        seg.generic_arg_list().and_then(|l| l.generic_args().next())
                    && let Some(t) = ta.ty()
                {
                    let k = self.lower_type(&subject);
                    let q = self.lower_type(&t);
                    self.borrow_args.push((k, q));
                }
            }
        }
    }

    /// `L: Add<R>` / `L: Add<Output = X>`: the right-hand side `a + b` takes (`R`), and
    /// `L::Output` (`X`, or else `L` itself: what arithmetic on a `L` yields here, and right for
    /// numbers and for an `impl Add for V` whose output is `V`).
    pub(super) fn record_op_bounds(&mut self, name: &str, lists: &[ast::TypeBoundList]) {
        use ra_ap_syntax::ast::HasGenericArgs;
        if !matches!(self.generics.get(name), Some(Ty::Param(_))) {
            return;
        }
        self.record_slice_index_bound(name, lists);
        for b in lists.iter().flat_map(|l| l.bounds()) {
            let Some(ast::Type::PathType(pt)) = b.ty() else {
                continue;
            };
            let Some(seg) = pt.path().and_then(|p| p.segment()) else {
                continue;
            };
            let Some(tr) = seg.name_ref().map(|n| n.text().to_string()) else {
                continue;
            };
            if self.adts.traits.contains_key(&tr) {
                continue;
            }
            if matches!(tr.as_str(), "PartialEq" | "PartialOrd") {
                self.record_cmp_bound(name, &tr, &seg);
                continue;
            }
            if crate::adt::op_method(&tr).is_none() {
                continue;
            }
            let mut out = None;
            for g in seg
                .generic_arg_list()
                .into_iter()
                .flat_map(|l| l.generic_args())
            {
                match g {
                    ast::GenericArg::TypeArg(ta) => {
                        if let Some(t) = ta.ty() {
                            let t = self.lower_type(&t);
                            self.op_args.insert((name.to_string(), tr.clone()), t);
                        }
                    }
                    ast::GenericArg::AssocTypeArg(a)
                        if a.name_ref().is_some_and(|n| n.text() == "Output") =>
                    {
                        out = a.ty().map(|t| self.lower_type(&t));
                    }
                    _ => {}
                }
            }
            let key = format!("{name}::Output");
            match out {
                Some(t) => {
                    self.generics.insert(key, t);
                }
                None => {
                    self.generics
                        .entry(key)
                        .or_insert_with(|| Ty::Param(name.to_string()));
                }
            }
        }
    }

    /// `a == b` / `a < b` (`tr` is `PartialEq` / `PartialOrd`) with an `a` of a parameter bounded
    /// `PartialEq<R>` / `PartialOrd<R>`: whether `b` is that `R` (then unified). A `PartialOrd<R>`
    /// bound serves `==` too: its supertrait is `PartialEq<R>`. Not when `b` is an `a` already.
    pub(super) fn cmp_bound_fits(&mut self, lt: &Ty, rt: &Ty, tr: &str) -> bool {
        let (Ty::Param(n) | Ty::Adt(n, _)) = self.shallow(lt) else {
            return false;
        };
        // (A struct or enum by a concrete `where Ident: PartialEq<I>`.)
        if matches!(self.shallow(lt), Ty::Adt(..))
            && !self.cmp_args.keys().any(|(k, _)| *k == n)
        {
            return false;
        }
        // (Two type parameters are one type only by name; `could_be` takes any two.)
        let fits = |c: &Self, x: &Ty| match (c.shallow(x), c.shallow(rt)) {
            (Ty::Param(a), Ty::Param(b)) => a == b,
            _ => c.could_be(x, rt),
        };
        if fits(self, lt) {
            return false;
        }
        let traits: &[&str] = if tr == "PartialEq" {
            &["PartialEq", "PartialOrd"]
        } else {
            &["PartialOrd"]
        };
        for t in traits {
            if let Some(x) = self.cmp_args.get(&(n.clone(), t.to_string())).cloned() {
                let x = self.lower_type(&x);
                if fits(self, &x) {
                    self.unify(&x, rt);
                    return true;
                }
            }
        }
        false
    }

    /// `name: PartialEq<R>` (or `PartialOrd<R>`, at `seg`): `R`, for `a == b` with an `a` of
    /// `name` (`name` may be a projection, `I::Item`).
    pub(super) fn record_cmp_bound(&mut self, name: &str, tr: &str, seg: &ast::PathSegment) {
        use ra_ap_syntax::ast::HasGenericArgs;
        if let Some(ast::GenericArg::TypeArg(ta)) =
            seg.generic_arg_list().and_then(|l| l.generic_args().next())
            && let Some(t) = ta.ty()
        {
            self.cmp_args
                .insert((name.to_string(), tr.to_string()), t);
        }
    }

    /// Record `name`'s user-trait bounds with their arguments.
    pub(super) fn record_bound_args(&mut self, name: &str, lists: &[ast::TypeBoundList]) {
        use ra_ap_syntax::ast::HasGenericArgs;
        let mut per: super::BoundArgs = HashMap::new();
        for l in lists {
            for b in l.bounds() {
                let Some(ast::Type::PathType(pt)) = b.ty() else {
                    continue;
                };
                let Some(seg) = pt.path().and_then(|p| p.segment()) else {
                    continue;
                };
                let Some(tr) = seg.name_ref().map(|n| n.text().to_string()) else {
                    continue;
                };
                if !self.adts.traits.contains_key(&tr) {
                    continue;
                }
                let mut pos = Vec::new();
                let mut named = HashMap::new();
                for g in seg
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|l| l.generic_args())
                {
                    match g {
                        ast::GenericArg::TypeArg(ta) => {
                            if let Some(t) = ta.ty() {
                                pos.push(self.lower_type(&t));
                            }
                        }
                        ast::GenericArg::AssocTypeArg(a) => {
                            if let (Some(n), Some(t)) = (a.name_ref(), a.ty()) {
                                named.insert(n.text().to_string(), self.lower_type(&t));
                            }
                        }
                        _ => {}
                    }
                }
                // (`C: Conv<u8> + Conv<u16>`: each, for the calls to pick from.)
                if let Some((first, _)) = per.get(&tr) {
                    let alts = self
                        .param_trait_alts
                        .entry((name.to_string(), tr.clone()))
                        .or_insert_with(|| vec![first.clone()]);
                    alts.push(pos);
                    continue;
                }
                per.insert(tr, (pos, named));
            }
        }
        if !per.is_empty() {
            self.param_trait_args.insert(name.to_string(), per);
        }
    }

    pub(super) fn leave_generics(&mut self) {
        self.generics.clear();
        self.param_bounds.clear();
        self.param_trait_args.clear();
        self.param_trait_alts.clear();
        self.op_args.clear();
        self.cmp_args.clear();
        self.lenient_projs.clear();
        self.slice_index_params.clear();
        self.const_tys.clear();
        self.from_args.clear();
        self.try_from_args.clear();
        self.try_error_bounds.clear();
        self.borrow_args.clear();
    }

    /// Fresh type variables for a generic function's parameters, with the bounds to check once
    /// they're known.
    pub(super) fn instantiate(
        &mut self,
        node: &SyntaxNode,
        type_params: &[TypeParam],
        params: &[Ty],
        ret: &Ty,
        turbofish: &[Ty],
    ) -> (Vec<Ty>, Ty) {
        let mut m = HashMap::new();
        self.last_type_args.clear();
        for (i, (name, bounds)) in type_params.iter().enumerate() {
            let v = self.fresh(VarKind::General);
            self.last_type_args.push(v.clone());
            if let Some(t) = turbofish.get(i) {
                self.unify(&v, t);
            }
            for b in bounds {
                self.pending_bounds
                    .push((v.clone(), b.clone(), node.clone(), self.base));
            }
            m.insert(name.clone(), v);
        }
        // Each bound's associated types (`S::Unit`): variables, settled once `S` is known; they
        // follow the type arguments (for the `{:?}` formatters a function takes).
        for (s, tr, a) in self.proj_names(type_params) {
            let Some(sv) = m.get(&s).cloned() else {
                continue;
            };
            let x = self.fresh(VarKind::General);
            self.pending_projections
                .push((sv, tr, a.clone(), x.clone()));
            self.last_type_args.push(x.clone());
            m.insert(format!("{s}::{a}"), x);
        }
        // Any other `S::Unit` in the signature.
        let mut projs: Vec<String> = Vec::new();
        for t in params.iter().chain(std::iter::once(ret)) {
            t.walk(&mut |x| {
                if let Ty::Param(p) = x
                    && p.contains("::")
                    && !projs.contains(p)
                {
                    projs.push(p.clone());
                }
            });
        }
        for p in projs {
            if m.contains_key(&p) {
                continue;
            }
            let Some((s, assoc)) = p.split_once("::") else {
                continue;
            };
            let Some(sv) = m.get(s).cloned() else {
                continue;
            };
            let tr = type_params
                .iter()
                .find(|(n, _)| n == s)
                .and_then(|(_, bs)| {
                    bs.iter().find(|b| {
                        self.adts
                            .traits
                            .get(*b)
                            .is_some_and(|d| d.assoc_types.iter().any(|a| a == assoc))
                    })
                })
                .cloned();
            let Some(tr) = tr else { continue };
            let x = self.fresh(VarKind::General);
            self.pending_projections
                .push((sv, tr, assoc.to_string(), x.clone()));
            m.insert(p.clone(), x);
        }
        (
            params.iter().map(|p| subst(p, &m)).collect(),
            subst(ret, &m),
        )
    }

    /// `emit(&mut Vec::new())` with `W: io::Write`: the one `Vec` impl of the trait (`impl Write
    /// for Vec<u8>`) names the element type, as rustc infers it.
    fn settle_by_impl(&mut self, t: &Ty, b: &str) {
        // (`shallow`, not `zonk`: an unsettled element type stays a variable.)
        let s = self.shallow(t);
        if let Some(key) = crate::adt::ext_key_of(&s)
            && let Some(tr) = self.adts.traits.get(b)
            && let Some(impl_args) = tr.methods.iter().find_map(|tm| {
                self.adts
                    .method(key, &tm.name)
                    .map(|s| s.impl_args.clone())
                    .filter(|a| !a.is_empty())
            })
        {
            for (arg, want) in crate::adt::ext_args(&s).iter().zip(&impl_args) {
                if matches!(self.shallow(arg), Ty::Var(_)) && !matches!(want, Ty::Param(_)) {
                    self.unify(arg, want);
                }
            }
        }
    }

    /// Check the bounds of every generic call in the function.
    pub(super) fn resolve_bounds(&mut self) {
        // A struct's type arguments, by its parameters' bounds: only inferred from, not checked.
        for (t, b) in std::mem::take(&mut self.adt_bounds) {
            self.settle_by_impl(&t, &b);
        }
        let saved = self.base;
        for (t, b, node, base) in std::mem::take(&mut self.pending_bounds) {
            // (Calls inside macro arguments were parsed from the argument's text.)
            self.base = base;
            let z = self.zonk(&t);
            if matches!(z, Ty::Var(_) | Ty::Unknown) {
                continue;
            }
            self.settle_by_impl(&t, &b);
            let z = self.zonk(&t);
            if !self.satisfies(&z, &b) {
                // (The react prelude's bounds say what a `jsx!` child or attribute takes.)
                if let Some((at, msg)) = self.react_bound_error(&z, &b, &node) {
                    self.error(&at, msg);
                    continue;
                }
                let msg = format!(
                    "the trait bound `{}: {b}` is not satisfied",
                    self.describe(&z)
                );
                self.error(&node, msg);
            }
        }
        for (t, b, node, base) in std::mem::take(&mut self.pending_arg_bounds) {
            self.base = base;
            let z = self.zonk(&t);
            if self.arg_bound_holds(&z, &b) != Some(false) {
                continue;
            }
            let msg = match &b {
                crate::adt::ArgBound::Fn(c) => format!(
                    "expected an `{}` closure, found `{}`",
                    self.describe(c).trim_start_matches("impl "),
                    self.describe(&z)
                ),
                crate::adt::ArgBound::Conv(tr, x) if tr == "Into" => format!(
                    "the trait bound `{}: From<{}>` is not satisfied (needed for `{}: Into<{}>`)",
                    self.describe(x),
                    self.describe(&z),
                    self.describe(&z),
                    self.describe(x)
                ),
                crate::adt::ArgBound::Conv(tr, x) => format!(
                    "the trait bound `{}: {tr}<{}>` is not satisfied",
                    self.describe(&z),
                    self.describe(x)
                ),
            };
            self.error(&node, msg);
        }
        self.base = saved;
    }

    /// Whether type parameter `name` (in the function being checked) has bound `b`, directly or
    /// through a subtrait (`Ord` gives `PartialOrd`, `Copy` gives `Clone`, ...).
    pub(super) fn param_has(&self, name: &str, b: &str) -> bool {
        // `T::Error` of a `T: TryFrom<X>` printed or unwrapped: a `where T::Error: Debug`, or the
        // bound's `Error = E`. (Its other uses stay unchecked.)
        if matches!(b, "Debug" | "Display")
            && name
                .strip_suffix("::Error")
                .is_some_and(|s| self.try_from_args.contains_key(s))
        {
            if let Some(e) = self.try_from_args.get(name).and_then(|e| e.first()) {
                return !self.lacks_fmt(&self.shallow(e), b == "Debug");
            }
            // (Through a user trait's supertraits too: `where T::Error: MyError` with `trait
            // MyError: Debug`.)
            let mut todo: Vec<String> = self.try_error_bounds.get(name).cloned().unwrap_or_default();
            let mut seen: Vec<String> = Vec::new();
            while let Some(t) = todo.pop() {
                if t == b || t == "Error" {
                    return true;
                }
                if !seen.contains(&t) {
                    if let Some(d) = self.adts.traits.get(&t) {
                        todo.extend(d.supers.iter().cloned());
                    }
                    seen.push(t);
                }
            }
            return false;
        }
        // A trait's own associated type or type parameter (`Shape::Unit`): unchecked.
        if self.is_template(name) {
            return true;
        }
        // `S::Unit`: what the trait declares (`type Unit: Debug;`).
        let declared: Vec<String>;
        let bounds = match name.split_once("::") {
            // (`I::Item` of an `I: IntoIterator`: a parameter of its own.)
            Some(_) if self.param_bounds.contains_key(name) => &self.param_bounds[name],
            Some((s, a)) => {
                let mut todo: Vec<String> = self
                    .bound_traits(s)
                    .iter()
                    .filter_map(|t| self.adts.traits.get(t)?.assoc_bounds.get(a).cloned())
                    .flatten()
                    .collect();
                // (A user trait's supertraits too: `type Bound: Bound;` with
                // `trait Bound: PartialOrd`.)
                let mut all: Vec<String> = Vec::new();
                while let Some(t) = todo.pop() {
                    if all.contains(&t) {
                        continue;
                    }
                    if let Some(d) = self.adts.traits.get(&t) {
                        todo.extend(d.supers.iter().cloned());
                    }
                    all.push(t);
                }
                declared = all;
                &declared
            }
            None => match self.param_bounds.get(name) {
                Some(b) => b,
                None => return false,
            },
        };
        let meets = |x: &String| {
            x == b
                || matches!(
                    (x.as_str(), b),
                    ("Ord", "PartialOrd" | "Eq" | "PartialEq")
                        | ("Eq", "PartialEq")
                        | ("PartialOrd", "PartialEq")
                        | ("Copy", "Clone")
                        | ("Display", "ToString")
                        | ("Error", "Display" | "Debug" | "ToString")
                )
        };
        // (Else through a user trait's supertraits: `trait Float: PartialOrd + Mul<Output =
        // Self>` gives a default method's `self < o` and `self * self`.)
        bounds.iter().any(meets)
            || (!name.contains("::")
                && bounds.iter().any(|x| self.adts.traits.contains_key(x))
                && self.bound_traits(name).iter().any(meets))
    }

    /// Whether a concrete type meets a bound.
    /// `f(&mut bytes)` for an `R: Read` parameter taken by value: reading would advance the
    /// caller's `&[u8]`, but the function reads from a `Cursor` of its own (see `is_bytes`). (An
    /// `r: &mut R` reads the caller's copy in place: `slice_ref_muts`.)
    pub(super) fn check_byte_readers(&mut self, f: &str, args: &[ast::Expr], inst: &[Ty]) {
        let Some(sig) = self.fns.get(f) else { return };
        let readers: Vec<usize> = (sig.params.iter().enumerate())
            .filter(|(_, p)| match p {
                Ty::Param(n) => sig
                    .type_params
                    .iter()
                    .any(|(x, bs)| x == n && bs.iter().any(|b| b == "IoRead" || b == "IoBufRead")),
                Ty::Dyn(tr, _) => tr == "IoRead" || tr == "IoBufRead",
                _ => false,
            })
            .map(|(i, _)| i)
            .collect();
        let mut_ref_params: Vec<bool> = (sig.ref_params.iter())
            .map(|r| r.as_deref().is_some_and(|r| r.starts_with("&mut")))
            .collect();
        for i in readers {
            // (An `impl Read` parameter stays `dyn`: the argument's own type.)
            let arg_ty = |c: &Self, r: &ast::RefExpr| c.raw.get(&c.range(r.syntax())).cloned();
            if let (Some(ast::Expr::RefExpr(r)), Some(t)) = (args.get(i), inst.get(i))
                && r.mut_token().is_some()
                && (self.is_bytes(t)
                    || matches!(t, Ty::Dyn(_, _)) && arg_ty(self, r).is_some_and(|a| self.is_bytes(&a)))
                // (`r: &mut R` given `&mut s`, `s: &[u8]`: it reads `s`'s own copy in place.)
                && !(mut_ref_params.get(i) == Some(&true)
                    && self.slice_ref_muts.contains(&self.range(r.syntax())))
            {
                let node = r.syntax().clone();
                self.error(&node, "reading from a `&mut &[u8]` taken by value would advance the caller's slice, which isn't supported; take `r: &mut R`, or pass the bytes (`&data[..]`) or an `io::Cursor::new(data)`");
            }
        }
    }

    /// `Vec<u8>` / `&[u8]`.
    pub(super) fn is_bytes(&self, t: &Ty) -> bool {
        matches!(self.zonk(t), Ty::Vec(e) if matches!(self.zonk(&e), Ty::Int(crate::ty::IntKind::U8)))
    }

    pub(super) fn satisfies(&self, t: &Ty, b: &str) -> bool {
        if let Some(ok) = self.react_bound(t, b) {
            return ok;
        }
        match b {
            "Sized" | "Send" | "Sync" | "Unpin" | "UnwindSafe" | "RefUnwindSafe"
            | "BuildHasher" | "Allocator" => true,
            "FromStr" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str => true,
                Ty::Adt(n, _) => self.adts.method(n, "from_str").is_some(),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            // (Std's, unless the file declares a trait of that name.)
            "FromIterator" | "Extend" if !self.adts.traits.contains_key(b) => match t {
                Ty::Vec(_) | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..) | Ty::Str => true,
                Ty::Adt(n, _) => {
                    let m = if b == "Extend" { "extend" } else { "from_iter" };
                    self.adts.method(n, m).is_some()
                }
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            // `C: Index<Ix>` / `IndexMut<Ix>`: a Vec, deque or map, or your type with the impl.
            "Index" | "IndexMut" if !self.adts.traits.contains_key(b) => match t {
                Ty::Vec(_) | Ty::Map(..) => true,
                Ty::Coll(k, _) => matches!(
                    k,
                    crate::ty::Coll::VecDeque | crate::ty::Coll::FixedArray(_)
                ) || (b == "Index" && *k == crate::ty::Coll::BTreeMap),
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            "Error" if !self.adts.traits.contains_key(b) => match t {
                Ty::Adt(n, _) => self.adts.defs.get(n).is_some_and(|d| d.implements("Error")),
                Ty::Extern(n) => super::is_std_error(n),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            // (`H::Item: IntoIterator` of a parameter kept as one: collections, iterators and
            // your iterable types; `κσ.$intoIter` iterates whatever it is.)
            "IntoIterator" if !self.adts.traits.contains_key(b) => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str => false,
                Ty::Param(n) => self.param_has(n, b),
                _ => true,
            },
            "Sum" | "Product" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => true,
                Ty::Adt(n, _) => self.adts.method(n, &b.to_lowercase()).is_some(),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            // A trait of the file's own named like an operator trait (`trait Add { .. }`).
            tr if crate::adt::op_method(tr).is_some() && self.adts.traits.contains_key(tr) => {
                self.implements_user_trait(t, tr)
            }
            // (`From<X>`'s `X` is checked at `T::from(x)`, `Into<Y>`'s at `.into()`; JSON's
            // traits where it's read or written. A `TryFrom<X>` without a fitting impl fails
            // at run time.)
            "From" | "TryFrom" | "Into" | "Serialize" | "Deserialize" | "DeserializeOwned" => true,
            "Display" | "ToString" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str => true,
                Ty::Adt(n, _) => self.adts.defs.get(n).is_some_and(|d| d.display),
                Ty::Extern(n) => super::is_std_error(n),
                Ty::Param(n) => self.param_has(n, b),
                _ => false,
            },
            "Add" | "Sub" | "Mul" | "Div" | "Rem" | "AddAssign" | "SubAssign" | "MulAssign"
            | "DivAssign" | "RemAssign" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => true,
                Ty::Str => matches!(b, "Add" | "AddAssign"),
                Ty::Param(n) => self.param_has(n, b),
                // `impl Add for V` (with `V` on the right, as `T: Add<Output = T>` uses it).
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                _ => false,
            },
            "BitAnd" | "BitOr" | "BitXor" | "BitAndAssign" | "BitOrAssign" | "BitXorAssign"
            | "Not" => match t {
                Ty::Int(_) | Ty::IntLit | Ty::Bool => true,
                Ty::Param(n) => self.param_has(n, b),
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                _ => false,
            },
            "Shl" | "Shr" | "ShlAssign" | "ShrAssign" => match t {
                Ty::Int(_) | Ty::IntLit => true,
                Ty::Param(n) => self.param_has(n, b),
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                _ => false,
            },
            "Neg" => match t {
                Ty::Int(k) => k.is_signed(),
                Ty::IntLit | Ty::Float { .. } => true,
                Ty::Param(n) => self.param_has(n, b),
                Ty::Adt(n, _) => self.has_op_impl(n, b),
                _ => false,
            },
            // `&[u8]` reads its bytes: a generic function wraps it in a `Cursor` on entry.
            "IoRead" | "IoBufRead" if self.is_bytes(t) => true,
            tr if self.adts.traits.contains_key(tr) => self.implements_user_trait(t, tr),
            "Copy" => match t {
                // (`&str` is `Copy`, and the two string types are one here: a JS string is a
                // value either way, so `T: Copy` takes it, as `GraphMap<&str, _>`'s nodes.)
                Ty::Str => true,
                Ty::Vec(_) | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..) => false,
                other => self.implements(other, "Copy"),
            },
            other => self.implements(t, other),
        }
    }

    /// `a < b`, `a + b` on a `T`: the bound must allow it.
    pub(super) fn require_param_bound(&mut self, node: &SyntaxNode, t: &Ty, b: &str, what: &str) {
        if let Ty::Param(n) = self.shallow(t)
            && !self.param_has(&n, b)
        {
            // (An associated type's bounds are the trait's: `type Unit: Add<Output = ..>;`.)
            let msg = match n.split_once("::") {
                Some((_, a)) => format!(
                    "{what} on associated type `{n}` needs a bound: declare it in the trait (`type {a}: {b}`)"
                ),
                None => format!(
                    "{what} on type parameter `{n}` needs a bound: add `{n}: {b}` to the function's generics"
                ),
            };
            self.error(node, msg);
        }
    }

    /// Whether `adt` has an `impl {tr} for adt` (`Add`, `Neg`, `AddAssign`, ..).
    pub(super) fn has_op_impl(&self, adt: &str, tr: &str) -> bool {
        self.adts
            .op_impls
            .get(adt)
            .is_some_and(|v| v.iter().any(|i| i.trait_name == tr))
    }

    /// `a + b` (or `a += b`, `assign`) with `a: T`: needs `T: Add` (etc.); the result is `T`.
    /// The function takes `T`'s class, which says what kind of number it is (integer division
    /// truncates, results wrap) or calls a struct's own `add`: `/`, `%`, `<<` and `>>` need it;
    /// the others fall back to JS arithmetic without it.
    pub(super) fn param_arith(
        &mut self,
        b: &ast::BinExpr,
        op: ast::ArithOp,
        name: &str,
        lt: &Ty,
        rhs: &ast::Expr,
        assign: bool,
    ) -> Ty {
        let shift = matches!(op, ast::ArithOp::Shl | ast::ArithOp::Shr);
        let bound = match op {
            ast::ArithOp::Add => "Add",
            ast::ArithOp::Sub => "Sub",
            ast::ArithOp::Mul => "Mul",
            ast::ArithOp::Div => "Div",
            ast::ArithOp::Rem => "Rem",
            ast::ArithOp::BitAnd => "BitAnd",
            ast::ArithOp::BitOr => "BitOr",
            ast::ArithOp::BitXor => "BitXor",
            ast::ArithOp::Shl => "Shl",
            ast::ArithOp::Shr => "Shr",
        };
        let bound = if assign {
            format!("{bound}Assign")
        } else {
            bound.to_string()
        };
        // `L: Add<R>`: the right-hand side is an `R`.
        if let Some(want) = self
            .op_args
            .get(&(name.to_string(), bound.clone()))
            .cloned()
        {
            self.check(rhs, &want);
        } else if shift {
            // (`T: Shl<u32>`: the amount is any integer, or a `T`.)
            let rt = self.infer(rhs, None);
            if !self.is_integer(&rt) && !matches!(self.shallow(&rt), Ty::Param(_)) {
                let msg = format!(
                    "no implementation for `{} {op} {}`",
                    self.describe(lt),
                    self.describe(&rt)
                );
                self.error(rhs.syntax(), msg);
            }
        } else {
            self.check(rhs, lt);
        }
        let sym = if assign {
            format!("`{op}=`")
        } else {
            format!("`{op}`")
        };
        self.require_param_bound(b.syntax(), lt, &bound, &sym);
        // Division and shifts depend on the number kind (integer division, the width).
        let needs_kind = shift || matches!(op, ast::ArithOp::Div | ast::ArithOp::Rem);
        self.param_class(name, b.syntax(), needs_kind.then_some(sym.as_str()));
        match self.generics.get(&format!("{name}::Output")) {
            Some(out) if !assign => out.clone(),
            _ => lt.clone(),
        }
    }

    /// Whether a call of trait `b`'s method on a value of `T` (`name`) goes by `T`'s class
    /// (`κσ.$dcallT`), noting the class the function then takes.
    pub(super) fn by_class_dispatch(&mut self, name: &str, b: &str) -> bool {
        // `x.money()` with `impl Money for i32` and `impl Money for f64`: JS can't tell a whole
        // `f64` from an integer.
        let numeric_impls = self
            .adts
            .trait_impls
            .iter()
            .filter(|(ty, trs)| {
                trs.iter().any(|t| t == b)
                    && matches!(Ty::from_name(ty), Ty::Int(_) | Ty::Float { .. })
            })
            .count();
        // The same with an impl on `Option` next to others: `Some(4)` is `4` at run time, so
        // only the class tells `Option<i32>`'s impl from `i32`'s.
        let option_impl = self
            .adts
            .trait_impls
            .get("Option")
            .is_some_and(|trs| trs.iter().any(|t| t == b))
            && self
                .adts
                .trait_impls
                .iter()
                .any(|(ty, trs)| ty != "Option" && trs.iter().any(|t| t == b));
        // And with impls on a tuple and a `Vec` (slice, array): all of them are JS arrays.
        let has = |k: &str| {
            self.adts
                .trait_impls
                .get(k)
                .is_some_and(|trs| trs.iter().any(|t| t == b))
        };
        let array_impls = has("Vec") && (1..=12).filter_map(crate::adt::tuple_key).any(has);
        // And with impls on two tuples of one length (`(u8, u8)`, `(i32, i32)`): only the
        // elements' classes tell them apart.
        let tuple_alts = self.adts.traits.get(b).is_some_and(|d| {
            d.methods.iter().any(|tm| {
                (1..=12)
                    .filter_map(crate::adt::tuple_key)
                    .any(|k| self.adts.inherent_alts.contains(&format!("{k}::{}", tm.name)))
            })
        });
        // And with a generic `impl<T: Tr> Tr for Vec<T>` (or `Cell<T>` / `RefCell<T>`): its
        // methods may need `T`'s class, which only a caller that knows the value's type has
        // (`κσ.$callElem`).
        let generic_vec = ["Vec", "RefCell", "HashMap", "HashSet"].iter().any(|k| {
            has(k)
                && self.adts.traits.get(b).is_some_and(|d| {
                    d.methods.iter().any(|tm| {
                        self.adts.method(k, &tm.name).is_some_and(|s| {
                            s.impl_args.iter().any(|a| matches!(a, Ty::Param(_)))
                        })
                    })
                })
        });
        // And with impls for two instantiations of one struct (`W<u8>`, `W<u16>`): only the class
        // passed for `T` (`κσ.$altT`) tells them apart.
        let inst_alts = self.adts.traits.get(b).is_some_and(|d| {
            d.methods.iter().any(|tm| {
                self.adts.inherent_alts.iter().any(|k| {
                    k.rsplit_once("::").is_some_and(|(ty, m)| {
                        m == tm.name && (self.adts.defs.contains_key(ty) || ty == "RefCell") && crate::adt::tuple_arity(ty).is_none()
                    })
                })
            })
        });
        let arrays = array_impls || tuple_alts || generic_vec;
        // And with impls on `str` and `String` (or `[T]` and `Vec<T>`) that differ: one type in JS.
        let twins = self.twin_trait(b);
        // And with an impl for `&T` (`Box<T>`) that does more than `T`'s: a reference passed for
        // `T` reaches it by its class (`κσ.$wrapT`).
        let wraps = self.wrapper_trait(b);
        (numeric_impls >= 2 || option_impl || arrays || twins || wraps || inst_alts)
            && (self.try_note_dict(name) || (arrays || wraps) && self.elem_class(name))
    }

    /// Methods on a value of type parameter `T`.
    pub(super) fn param_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        name: &str,
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        // A method of a user trait in `T`'s bounds.
        // (`p.parse(s)` on a `T::P` whose trait declares `type P: Parser;`: those bounds.)
        let mut bounds = match self.param_bounds.get(name) {
            Some(b) => b.clone(),
            None => self.bound_traits(name),
        };
        // (And a trait a blanket impl gives `T` through those bounds.)
        for tr in self.param_blanket_traits(name) {
            if !bounds.contains(&tr) {
                bounds.push(tr);
            }
        }
        if let Some((b, sig)) = bounds
            .iter()
            .find_map(|b| self.trait_method(b, method).map(|s| (b.clone(), s)))
            && sig.receiver.is_some()
        {
            if self.by_class_dispatch(name, &b) {
                let r = self.range(m.syntax());
                self.class_dispatch.insert(r);
            }
            self.note_split_method(m, &b, method);
            return self.call_trait_method(m, recv, &b, sig, Ty::Param(name.to_string()), args);
        }
        let t = Ty::Param(name.to_string());
        let need = |c: &mut Self, b: &str| {
            if !c.param_has(name, b) {
                c.error(
                    m.syntax(),
                    format!("`{method}` on type parameter `{name}` needs a bound: add `{name}: {b}` to the function's generics"),
                );
            }
        };
        let sig = |c: &mut Self, params: &[Ty], ret: Ty| {
            c.arg_count(m.syntax(), "method", params.len(), args.len());
            for (a, p) in args.iter().zip(params) {
                c.check(a, p);
            }
            ret
        };
        match method {
            "clone" => {
                need(self, "Clone");
                sig(self, &[], t)
            }
            "to_string" => {
                need(self, "ToString");
                sig(self, &[], Ty::Str)
            }
            "eq" | "ne" => {
                need(self, "PartialEq");
                sig(self, &[t], Ty::Bool)
            }
            "cmp" => {
                need(self, "Ord");
                sig(self, &[t], Ty::Ordering)
            }
            "partial_cmp" => {
                need(self, "PartialOrd");
                sig(self, &[t], Ty::Option(Box::new(Ty::Ordering)))
            }
            "max" | "min" => {
                need(self, "Ord");
                sig(self, &[t.clone()], t)
            }
            // `self.0.fmt(f)`: `Display::fmt` / `Debug::fmt` delegation.
            "fmt" if self.param_has(name, "Display") || self.param_has(name, "Debug") => {
                sig(self, &[Ty::Formatter], super::gaps::fmt_result())
            }
            // An `E: Error`: the next error in its chain, as a `dyn Error`'s.
            "source" if self.param_has(name, "Error") => {
                self.arg_count(m.syntax(), "method", 0, args.len());
                self.note_crate(m.syntax(), "err.source", vec![]);
                Ty::Option(Box::new(Ty::Dyn("Error".into(), Vec::new())))
            }
            // A `C: Extend<T>`: by what the value is at run time (a `Vec`, map, set, string or
            // your own `impl Extend`).
            "extend" if self.param_has(name, "Extend") => {
                self.check_mutable(recv, super::Mutation::Borrow);
                self.arg_count(m.syntax(), "method", 1, args.len());
                for a in args {
                    self.infer(a, None);
                }
                Ty::Unit
            }
            // An `L: IntoIterator` kept a parameter (the impl's, or `where &'a I: IntoIterator`):
            // the runtime's iterator over whatever it is (`κσ.$intoIter`).
            "into_iter" if self.param_has(name, "IntoIterator") => {
                let key = format!("{name}::Item");
                let elem = self
                    .generics
                    .get(&key)
                    .cloned()
                    .unwrap_or(Ty::Param(key));
                sig(
                    self,
                    &[],
                    Ty::Iter {
                        elem: Box::new(elem),
                        mutable: false,
                    },
                )
            }
            // An `S: BuildHasher` (a `RandomState`, or your own): `x.hash(&mut h)` feeds the
            // hasher it builds.
            "build_hasher" if self.param_has(name, "BuildHasher") => sig(self, &[], Ty::Hasher),
            "hash_one" if self.param_has(name, "BuildHasher") => {
                self.arg_count(m.syntax(), "method", 1, args.len());
                for a in args {
                    self.infer(a, None);
                }
                Ty::Int(crate::ty::IntKind::U64)
            }
            _ => {
                for a in args {
                    self.infer(a, None);
                }
                self.error(
                    m.syntax(),
                    format!("no method named `{method}` found for type parameter `{name}` in the current scope (only methods from its bounds are available)"),
                );
                Ty::Unknown
            }
        }
    }
}

impl Checker<'_> {
    /// `i32::ZERO` (an associated const of your trait impl for a built-in type) or `Tr::ZERO`
    /// (`Self` from the context: `let p: P = Tr::ZERO;`): read off `Self`'s class
    /// (`const_paths`).
    pub(super) fn trait_const_path(
        &mut self,
        node: &SyntaxNode,
        owner: &str,
        konst: &str,
    ) -> Option<Ty> {
        let (self_ty, t) = if let Some(key) =
            crate::adt::ext_key(owner).filter(|k| self.adts.ext_types.contains(*k))
        {
            let t = self.adts.assoc_consts.get(key)?.get(konst)?.clone();
            // (`<Vec<u8>>::SIZE` / `Vec::<u8>::SIZE`: with the element, whose class a const of
            // `impl<T> Tr for Vec<T>` reads.)
            let seg = ast::PathExpr::cast(node.clone())
                .and_then(|p| p.path())
                .and_then(|p| p.qualifier())
                .and_then(|q| q.segment());
            let written = match seg.as_ref().and_then(|s| s.type_anchor()).and_then(|a| a.ty()) {
                Some(t) => Some(self.lower_type(&t)),
                None => {
                    let arg = seg
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next())
                        .and_then(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        })
                        .map(|t| Box::new(self.lower_type(&t)));
                    match (owner, arg) {
                        ("Vec", Some(a)) => Some(Ty::Vec(a)),
                        ("Option", Some(a)) => Some(Ty::Option(a)),
                        ("RefCell", Some(a)) => Some(Ty::RefCell(a)),
                        _ => None,
                    }
                }
            };
            (written.unwrap_or_else(|| Ty::from_name(owner)), t)
        } else if let Some(d) = self.adts.traits.get(owner) {
            let t = d.const_tys.get(konst)?.clone();
            (self.fresh(VarKind::General), t)
        } else {
            return None;
        };
        let t = subst(&t, &[("Self".to_string(), self_ty.clone())].into());
        let r = self.range(node);
        self.const_paths.insert(r, (self_ty, konst.to_string()));
        Some(t)
    }
}
