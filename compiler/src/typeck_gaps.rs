//! Std gaps: more `Vec` / slice / `char` / integer / `str` / `OsStr` methods, `NonZero*`,
//! `Ref::map`, and `Display::fmt` / `Debug::fmt` delegation (`self.0.fmt(f)`,
//! `fmt::Display::fmt(&self.0, f)`). A child module of `typeck`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasName},
};

use super::{Checker, Mutation, USIZE, VarKind};
use crate::ty::{IntKind, Ty};

/// `fmt::Result`.
pub(super) fn fmt_result() -> Ty {
    Ty::Result(
        Box::new(Ty::Unit),
        Box::new(Ty::Extern("fmt::Error".into())),
    )
}

/// The unsigned kind of the same width.
fn unsigned(k: IntKind) -> IntKind {
    match k {
        IntKind::I8 => IntKind::U8,
        IntKind::I16 => IntKind::U16,
        IntKind::I32 => IntKind::U32,
        IntKind::I64 => IntKind::U64,
        IntKind::I128 => IntKind::U128,
        IntKind::Isize => IntKind::Usize,
        other => other,
    }
}

/// `Pin`'s methods that hand back the pinned value itself (`Pin` is erased).
pub fn pin_identity(method: &str) -> bool {
    matches!(
        method,
        "as_mut" | "get_mut" | "as_ref" | "get_ref" | "get_unchecked_mut" | "into_ref"
    )
}

/// Whether `node` is in an `impl Debug for ..`.
fn in_debug_impl(node: &ra_ap_syntax::SyntaxNode) -> bool {
    node.ancestors()
        .find_map(ast::Impl::cast)
        .and_then(|i| i.trait_())
        .is_some_and(|t| {
            let s = t.syntax().text().to_string();
            s == "Debug" || s.ends_with("::Debug")
        })
}

/// Which of `fmt::Display` / `fmt::Debug` the file's `use`s bring into scope (so `x.fmt(f)`
/// can name its method).
fn fmt_traits_in_scope(node: &ra_ap_syntax::SyntaxNode) -> (bool, bool) {
    let mut found = (false, false);
    let Some(root) = node.ancestors().last() else {
        return found;
    };
    for u in root.descendants().filter_map(ast::Use::cast) {
        for (path, _, star) in super::use_leaves(&u) {
            match path
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
            {
                [.., "fmt"] if star => found = (true, true),
                [.., "fmt", "Display"] => found.0 = true,
                [.., "fmt", "Debug"] => found.1 = true,
                _ => {}
            }
        }
    }
    found
}

/// Which `fmt` `x.fmt(f)` at `node` calls, for a type with a `Display` / `Debug` impl or not:
/// `Ok(true)` for `Debug::fmt`; `Err((true, true))` when both apply, `Err((false, false))` when
/// neither trait is in scope.
pub fn fmt_choice(
    node: &ra_ap_syntax::SyntaxNode,
    has_display: bool,
    has_debug: bool,
) -> Result<bool, (bool, bool)> {
    let (display, debug) = fmt_traits_in_scope(node);
    match (display && has_display, debug && has_debug) {
        (true, false) => Ok(false),
        (false, true) => Ok(true),
        both => Err(both),
    }
}

/// Whether `x.fmt(f)` at `node` is `Debug::fmt`: by `fmt_choice`, else by the impl it's in.
pub fn delegated_fmt_is_debug(
    node: &ra_ap_syntax::SyntaxNode,
    has_display: bool,
    has_debug: bool,
) -> bool {
    fmt_choice(node, has_display, has_debug).unwrap_or_else(|_| in_debug_impl(node))
}

impl Checker<'_> {
    /// Methods from this module; `None` when `method` isn't one of them.
    pub(super) fn gaps_method(
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
        let u8s = || Ty::Vec(Box::new(Ty::Int(IntKind::U8)));
        let user_hasher_arg = method == "hash" && args.len() == 1 && {
            let t = self.infer(&args[0], None);
            matches!(self.shallow(&t), Ty::Adt(n, _) if self.is_user_hasher(&n))
        };
        Some(match (self.shallow(rt), method) {
            // ---- Futures: `Pin` is erased, so `pinned.as_mut()` / `.get_mut()` are the future
            // itself; `fut.poll(cx)` on an async fn's or block's future (a promise) or one
            // passed as an `F: Future`.
            (t @ Ty::Future(_), m2) | (t @ Ty::Adt(..), m2)
                if pin_identity(m2)
                    && args.is_empty()
                    && match &t {
                        Ty::Adt(n, _) => self.adts.future_outputs.contains_key(n),
                        _ => true,
                    } =>
            {
                t
            }
            (Ty::Future(t), "poll") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                Ty::Adt("TaskPoll".into(), vec![*t])
            }
            // ---- `self.0.fmt(f)`: the value formatted with `f`'s spec.
            (t, "fmt") if args.len() == 1 && !matches!(t, Ty::Var(_) | Ty::Formatter) => {
                if let Some(a) = a0 {
                    self.check(a, &Ty::Formatter);
                }
                let (has_display, has_debug) = match &t {
                    Ty::Adt(n, _) => (
                        self.adts.defs.get(n).is_some_and(|d| d.display),
                        self.implements(&t, "Debug"),
                    ),
                    _ => (true, true),
                };
                // (Neither imported: the impl it's in decides, as rustc accepts it.)
                if let Err((true, true)) = fmt_choice(m.syntax(), has_display, has_debug) {
                    self.error(
                        m.syntax(),
                        "multiple applicable items in scope: `fmt` is `Display::fmt` and `Debug::fmt` here (write `fmt::Display::fmt(&x, f)`)",
                    );
                }
                fmt_result()
            }
            // ---- `OsStr` / `OsString` (strings here), and `str::get(range)`
            (Ty::Str, "to_str") => {
                count(self, 0);
                opt(Ty::Str)
            }
            (
                Ty::Str,
                "to_string_lossy"
                | "to_os_string"
                | "as_os_str"
                | "into_os_string"
                | "into_boxed_str"
                | "as_encoded_bytes_str",
            ) => {
                count(self, 0);
                Ty::Str
            }
            (Ty::Str, "into_string") => {
                count(self, 0);
                Ty::Result(Box::new(Ty::Str), Box::new(Ty::Str))
            }
            (Ty::Str, "get") if matches!(a0, Some(ast::Expr::RangeExpr(_))) => {
                count(self, 1);
                self.range_bounds(a0);
                opt(Ty::Str)
            }
            // ---- Vec and slices
            (Ty::Vec(e), "into_boxed_slice" | "into_vec") => {
                count(self, 0);
                Ty::Vec(e)
            }
            (Ty::Vec(_), "capacity") => {
                count(self, 0);
                USIZE
            }
            // (Its length in bytes: JS strings have no spare room to report.)
            (Ty::Str, "capacity") => {
                count(self, 0);
                USIZE
            }
            (Ty::Vec(_), "reserve" | "reserve_exact" | "shrink_to") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                Ty::Unit
            }
            (Ty::Vec(_), "shrink_to_fit") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 0);
                Ty::Unit
            }
            (Ty::Vec(e), "resize") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 2);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                if let Some(a) = a1 {
                    self.check(a, &e);
                }
                Ty::Unit
            }
            (Ty::Vec(e), "resize_with") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 2);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                self.closure_arg(a1, vec![], *e);
                Ty::Unit
            }
            (Ty::Vec(e), "chunks_mut" | "chunks_exact_mut" | "rchunks_mut") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                Ty::Iter {
                    elem: Box::new(Ty::Vec(e)),
                    mutable: true,
                }
            }
            (Ty::Vec(_), "copy_within") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 2);
                self.range_bounds(a0);
                if let Some(a) = a1 {
                    self.check(a, &USIZE);
                }
                Ty::Unit
            }
            (Ty::Vec(e), "retain_mut") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 1);
                self.closure_arg(a0, vec![*e], Ty::Bool);
                Ty::Unit
            }
            (Ty::Vec(e), "splice") => {
                self.check_mutable(recv, Mutation::Borrow);
                count(self, 2);
                self.range_bounds(a0);
                self.extend_arg(a1, &e);
                Ty::Iter {
                    elem: e,
                    mutable: false,
                }
            }
            // `[T; N]::map`.
            (Ty::Vec(e), "map") => {
                count(self, 1);
                let r = self.fresh(VarKind::General);
                let r = self.closure_arg(a0, vec![*e], r);
                Ty::Vec(Box::new(r))
            }
            (Ty::Vec(e), "strip_prefix" | "strip_suffix") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Vec(e.clone()));
                }
                opt(Ty::Vec(e))
            }
            (Ty::Vec(e), "is_ascii") if matches!(self.shallow(&e), Ty::Int(IntKind::U8)) => {
                count(self, 0);
                Ty::Bool
            }
            (Ty::Vec(e), "eq_ignore_ascii_case")
                if matches!(self.shallow(&e), Ty::Int(IntKind::U8)) =>
            {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Vec(e));
                }
                Ty::Bool
            }
            // ---- char
            (Ty::Char, "len_utf8" | "len_utf16") => {
                count(self, 0);
                USIZE
            }
            (Ty::Char, "encode_utf8") => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &u8s());
                }
                Ty::Str
            }
            (Ty::Char, "escape_debug" | "escape_default" | "escape_unicode") => {
                count(self, 0);
                Ty::Str
            }
            // ---- integers
            (Ty::Int(k), "to_le_bytes" | "to_be_bytes" | "to_ne_bytes") => {
                count(self, 0);
                let bytes = match k {
                    IntKind::I8 | IntKind::U8 => 1,
                    IntKind::I16 | IntKind::U16 => 2,
                    IntKind::I32 | IntKind::U32 => 4,
                    IntKind::I128 | IntKind::U128 => 16,
                    _ => 8,
                };
                self.std_array_dims.insert(m.syntax().text_range(), vec![bytes]);
                u8s()
            }
            (Ty::Int(k), "unsigned_abs") => {
                count(self, 0);
                Ty::Int(unsigned(k))
            }
            (Ty::Int(k), "next_power_of_two") if !k.is_signed() => {
                count(self, 0);
                Ty::Int(k)
            }
            (Ty::Int(k), "checked_next_power_of_two") if !k.is_signed() => {
                count(self, 0);
                opt(Ty::Int(k))
            }
            // Your own `Hasher`'s provided methods, and `x.hash(&mut it)`.
            (Ty::Adt(n, _), m2) if self.is_user_hasher(&n) && m2.starts_with("write_") => {
                count(self, 1);
                let want = Ty::Int(IntKind::from_name(&m2["write_".len()..])?);
                if let Some(a) = a0 {
                    self.check(a, &want);
                }
                Ty::Unit
            }
            (_, "hash") if user_hasher_arg => Ty::Unit,
            (Ty::Adt(n, _), "hash_one") if self.adts.method(&n, "build_hasher").is_some() => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                Ty::Int(IntKind::U64)
            }
            // `NonZeroU32::get` (a `NonZero*` is its integer here).
            (Ty::Int(k), "get") if args.is_empty() => Ty::Int(k),
            _ => return None,
        })
    }

    /// `Self::Item` outside the `Iterator` impl (in `impl DoubleEndedIterator for T`): the
    /// `Iterator` impl's `type Item`, which may come later in the file.
    pub(super) fn gaps_self_item(&mut self, p: &ast::PathType) -> Option<Ty> {
        let q = p.path()?.qualifier()?;
        if q.syntax().text() != "Self" {
            return None;
        }
        let Some(Ty::Adt(n, _)) = self.self_ty.clone() else {
            return None;
        };
        if let Some(t) = self.adts.iter_items.get(&n) {
            return Some(t.clone());
        }
        let root = p.syntax().ancestors().last()?;
        let item = root
            .descendants()
            .filter_map(ast::Impl::cast)
            .find_map(|imp| {
                let tr = imp.trait_()?.syntax().text().to_string();
                let ty = imp.self_ty()?.syntax().text().to_string();
                let base = ty.split('<').next().unwrap_or_default().trim().to_string();
                if !(tr == "Iterator" || tr.ends_with("::Iterator")) || base != n {
                    return None;
                }
                imp.assoc_item_list()?.assoc_items().find_map(|a| match a {
                    ast::AssocItem::TypeAlias(t)
                        if t.name().is_some_and(|x| x.text() == "Item") =>
                    {
                        t.ty()
                    }
                    _ => None,
                })
            })?;
        Some(self.lower_type(&item))
    }

    /// Whether `n` is a struct you wrote `impl Hasher for` (it has `write` and `finish`).
    /// The `Output` of a hand-written future (`impl Future for X`), with `X`'s type arguments.
    pub(super) fn adt_future_output(&self, t: &Ty) -> Option<Ty> {
        let Ty::Adt(n, targs) = self.shallow(t) else {
            return None;
        };
        let out = self.adts.future_outputs.get(&n)?;
        let mut map = self.adt_subst(&n, &targs);
        // (`impl<F: Future> Future for Counted<F> { type Output = F::Output; .. }`.)
        for (p, a) in map.clone() {
            let inner = match self.shallow(&a) {
                Ty::Future(t) => Some(*t),
                a @ Ty::Adt(..) => self.adt_future_output(&a),
                _ => None,
            };
            if let Some(t) = inner {
                map.insert(format!("{p}::Output"), t);
            }
        }
        Some(super::generic::subst(out, &map))
    }

    pub(super) fn is_user_hasher(&self, n: &str) -> bool {
        self.adts.method(n, "write").is_some() && self.adts.method(n, "finish").is_some()
    }

    /// The bounds of a range argument (`1..3`, `..`) as `usize`s.
    fn range_bounds(&mut self, a: Option<&ast::Expr>) {
        use ra_ap_syntax::ast::RangeItem;
        match a {
            Some(ast::Expr::RangeExpr(r)) => {
                for b in [r.start(), r.end()].into_iter().flatten() {
                    self.check(&b, &USIZE);
                }
            }
            Some(a) => {
                self.error(a.syntax(), "a range is expected here: `1..3`, `..n`, `..`");
            }
            None => {}
        }
    }

    /// Associated functions from this module: `u32::from_le_bytes`, `NonZeroU32::new` (written
    /// `u32::__nz_new` by the pre-pass), `fmt::Display::fmt(&x, f)`, `Ref::map(r, f)`.
    pub(super) fn gaps_call(
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
            [int, "from_le_bytes" | "from_be_bytes" | "from_ne_bytes"]
                if IntKind::from_name(int).is_some() =>
            {
                count(self, 1);
                if let Some(a) = a0 {
                    self.check(a, &Ty::Vec(Box::new(Ty::Int(IntKind::U8))));
                }
                Ty::Int(IntKind::from_name(int)?)
            }
            [int, "__nz_new"] if IntKind::from_name(int).is_some() => {
                count(self, 1);
                let k = Ty::Int(IntKind::from_name(int)?);
                if let Some(a) = a0 {
                    self.check(a, &k);
                }
                Ty::Option(Box::new(k))
            }
            [int, "__nz_new_unchecked"] if IntKind::from_name(int).is_some() => {
                count(self, 1);
                let k = Ty::Int(IntKind::from_name(int)?);
                if let Some(a) = a0 {
                    self.check(a, &k);
                }
                k
            }
            [.., "fmt", "Display" | "Debug", "fmt"] | ["Display" | "Debug", "fmt"] => {
                count(self, 2);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                if let Some(a) = a1 {
                    self.check(a, &Ty::Formatter);
                }
                fmt_result()
            }
            [.., "Ref" | "RefMut", "map"] => {
                count(self, 2);
                let t = a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown);
                let r = self.fresh(VarKind::General);
                self.closure_arg(a1, vec![t], r)
            }
            // An allocator (`A: Allocator`, `Global`) isn't used: `Vec::new_in(alloc)` is a
            // `Vec::new()`, `Box::new_in(x, alloc)` is `x`.
            [.., "Vec", "new_in"] => {
                count(self, 1);
                if let Some(a) = a0 {
                    self.infer(a, None);
                }
                Ty::Vec(Box::new(self.fresh(VarKind::General)))
            }
            [.., "Vec", "with_capacity_in"] => {
                count(self, 2);
                if let Some(a) = a0 {
                    self.check(a, &USIZE);
                }
                if let Some(a) = a1 {
                    self.infer(a, None);
                }
                Ty::Vec(Box::new(self.fresh(VarKind::General)))
            }
            [.., "Box", "new_in" | "pin_in"] => {
                count(self, 2);
                if let Some(a) = a1 {
                    self.infer(a, None);
                }
                match a0 {
                    Some(a) => self.infer(a, None),
                    None => Ty::Unknown,
                }
            }
            // A map with your own `BuildHasher`: the hasher isn't used (keys are compared by value).
            [
                ..,
                "HashMap",
                "with_hasher" | "with_capacity_and_hasher" | "default",
            ] => {
                // (The hasher, `Default::default()` or `FxBuildHasher`, is never used: only a
                // capacity is typed.)
                if names.last() == Some(&"with_capacity_and_hasher")
                    && let Some(a) = a0
                {
                    self.check(a, &Ty::Int(crate::ty::IntKind::Usize));
                }
                let (k, v) = (self.fresh(VarKind::General), self.fresh(VarKind::General));
                Ty::Map(Box::new(k), Box::new(v))
            }
            [
                ..,
                "HashSet",
                "with_hasher" | "with_capacity_and_hasher" | "default",
            ] => {
                // (The hasher, `Default::default()` or `FxBuildHasher`, is never used: only a
                // capacity is typed.)
                if names.last() == Some(&"with_capacity_and_hasher")
                    && let Some(a) = a0
                {
                    self.check(a, &Ty::Int(crate::ty::IntKind::Usize));
                }
                Ty::Set(Box::new(self.fresh(VarKind::General)))
            }
            // `RandomState::new()`: a map ignores its hasher, so it's nothing at run time.
            [.., "RandomState" | "BuildHasherDefault", "new" | "default"] => {
                count(self, 0);
                Ty::Unit
            }
            [.., "Ref" | "RefMut", "clone"] => {
                count(self, 1);
                a0.map(|a| self.infer(a, None)).unwrap_or(Ty::Unknown)
            }
            _ => return None,
        })
    }
}
