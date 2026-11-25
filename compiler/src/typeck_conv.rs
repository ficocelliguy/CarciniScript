//! `From` / `Into` conversions: `impl From<X> for Y`, `Y::from(x)`, `x.into()` and the error
//! conversion `?` does. A type may implement `From` several times, so each impl's `from` is its
//! own static method (`from$0`, `from$1`, ..); the checker picks one per call by the argument's
//! type and records it in `Types::conversions` for codegen. A child module of `typeck`.

use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::ArithOp, ast::HasArgList};

use super::{Checker, VarKind};
use crate::adt::{ArgRef, Conv, FromImpl};
use crate::ty::{IntKind, Ty};

/// The `X` of `.into()`'s receiver when it's a parameter of the enclosing function declared
/// `impl Into<X>`, or `T` with a `T: Into<X>` bound (inline or in `where`).
/// `x.into()` with `x: T` where `T: Into<X>` for a number type `X`: narrower integers are JS
/// numbers, so into an `i64` they become BigInts (and a `bool` 0 or 1).
fn into_number_conv(target: &Ty) -> Conv {
    match target {
        Ty::Int(k) if k.is_big() => Conv::Wrap {
            js: "κσ.$intoBig".into(),
        },
        Ty::Float { .. } | Ty::Int(_) => Conv::Wrap {
            js: "κσ.$intoNum".into(),
        },
        _ => Conv::Identity,
    }
}

/// How the argument of a conversion call at `node` is written, for `narrow_alts`: the receiver
/// of `x.try_into()`, the argument of `T::try_from(x)`, the items of `it.collect()` and of
/// `t.extend(it)`.
fn arg_ref_hint(node: &SyntaxNode) -> Option<ArgRef> {
    if let Some(mc) = ast::MethodCallExpr::cast(node.clone()) {
        return match mc.name_ref()?.text().to_string().as_str() {
            "try_into" => expr_ref_hint(&mc.receiver()?),
            "collect" => items_ref_hint(&mc.receiver()?),
            "extend" => items_ref_hint(&mc.arg_list()?.args().next()?),
            _ => None,
        };
    }
    let c = ast::CallExpr::cast(node.clone())?;
    let ast::Expr::PathExpr(p) = c.expr()? else {
        return None;
    };
    let f = p.path()?.segment()?.name_ref()?;
    let a = c.arg_list()?.args().next()?;
    match f.text().to_string().as_str() {
        "try_from" => expr_ref_hint(&a),
        "from_iter" => items_ref_hint(&a),
        _ => None,
    }
}

/// `ArgRef` of a value by how it's written: `"x"` / `s.as_str()` / `&v[..]` are slices, `&x` a
/// reference, `s.to_string()` / `format!(..)` / `vec![..]` values. `None` when it doesn't say
/// (a variable).
pub(super) fn expr_ref_hint(e: &ast::Expr) -> Option<ArgRef> {
    match crate::typeck::strip_parens(e) {
        ast::Expr::Literal(l) => match l.kind() {
            ast::LiteralKind::String(_) | ast::LiteralKind::ByteString(_) => Some(ArgRef::Slice),
            _ => Some(ArgRef::Owned),
        },
        ast::Expr::RefExpr(r) => match r.expr().map(|x| crate::typeck::strip_parens(&x)) {
            Some(ast::Expr::IndexExpr(_)) => Some(ArgRef::Slice),
            _ => Some(ArgRef::Ref),
        },
        ast::Expr::MethodCallExpr(m) => match m.name_ref()?.text().to_string().as_str() {
            "as_str" | "as_slice" | "as_bytes" | "as_ref" | "trim" | "trim_start" | "trim_end"
            | "as_path" => Some(ArgRef::Slice),
            "to_string" | "to_owned" | "clone" | "to_vec" | "into" | "collect" | "into_bytes"
            | "into_owned" | "into_string" | "join" | "concat" | "repeat" | "to_uppercase"
            | "to_lowercase" | "replace" | "into_boxed_str" | "into_boxed_slice" => {
                Some(ArgRef::Owned)
            }
            _ => None,
        },
        ast::Expr::MacroExpr(m) => {
            let name = m.macro_call()?.path()?.segment()?.name_ref()?;
            matches!(name.text().to_string().as_str(), "format" | "vec").then_some(ArgRef::Owned)
        }
        ast::Expr::CallExpr(c) => {
            let ast::Expr::PathExpr(p) = c.expr()? else {
                return None;
            };
            let f = p.path()?.segment()?.name_ref()?;
            matches!(
                f.text().to_string().as_str(),
                "from" | "new" | "with_capacity" | "from_utf8" | "from_utf8_lossy"
            )
            .then_some(ArgRef::Owned)
        }
        ast::Expr::ArrayExpr(_) | ast::Expr::TupleExpr(_) => Some(ArgRef::Owned),
        _ => None,
    }
}

/// `ArgRef` of the items of an iterable by how it's written: `&v` / `v.iter()` / `m.keys()`
/// give references, `vec![..]` / `[..]` / `.cloned()` / `.drain(..)` values.
fn items_ref_hint(e: &ast::Expr) -> Option<ArgRef> {
    match crate::typeck::strip_parens(e) {
        ast::Expr::RefExpr(_) => Some(ArgRef::Ref),
        ast::Expr::ArrayExpr(_) => Some(ArgRef::Owned),
        ast::Expr::MacroExpr(m) => {
            let name = m.macro_call()?.path()?.segment()?.name_ref()?;
            (name.text() == "vec").then_some(ArgRef::Owned)
        }
        ast::Expr::MethodCallExpr(m) => match m.name_ref()?.text().to_string().as_str() {
            "iter" | "keys" | "values" => Some(ArgRef::Ref),
            "into_iter" => match crate::typeck::strip_parens(&m.receiver()?) {
                ast::Expr::RefExpr(_) => Some(ArgRef::Ref),
                _ => Some(ArgRef::Owned),
            },
            "cloned" | "copied" | "drain" | "into_values" | "into_keys" | "chars" | "bytes"
            | "lines" | "split_whitespace" => Some(ArgRef::Owned),
            // (Adapters that pass the items on as they are.)
            "filter" | "skip" | "take" | "rev" | "chain" | "peekable" | "skip_while"
            | "take_while" | "step_by" | "fuse" | "inspect" => items_ref_hint(&m.receiver()?),
            _ => None,
        },
        _ => None,
    }
}

fn into_bound_target(m: &ast::MethodCallExpr, tr: &str) -> Option<ast::Type> {
    use ra_ap_syntax::ast::{HasGenericParams, HasName};
    let ast::Expr::PathExpr(p) = crate::typeck::strip_parens(&m.receiver()?) else {
        return None;
    };
    let name = p.path()?.as_single_name_ref()?;
    let f = m.syntax().ancestors().find_map(ast::Fn::cast)?;
    let ty = f.param_list()?.params().find_map(|p| {
        let named = matches!(p.pat(), Some(ast::Pat::IdentPat(ip)) if ip.name().is_some_and(|n| n.text() == name.text()));
        if named { p.ty() } else { None }
    })?;
    // (`prefix: &'a S` with `S: AsRef<OsStr> + ?Sized`: the bound is on what it points to.)
    let ty = match ty {
        ast::Type::RefType(r) => r.ty()?,
        t => t,
    };
    match ty {
        ast::Type::ImplTraitType(i) => into_target(i.type_bound_list()?, tr),
        ast::Type::PathType(pt) => {
            let t = pt.path()?.as_single_name_ref()?.text().to_string();
            // (The function's own parameters, then its `impl`'s: `impl<S: Into<X>> From<S> for Y`.)
            param_into_target(&t, tr, f.generic_param_list(), f.where_clause()).or_else(|| {
                let imp = f.syntax().parent()?.parent().and_then(ast::Impl::cast)?;
                param_into_target(&t, tr, imp.generic_param_list(), imp.where_clause())
            })
        }
        _ => None,
    }
}

/// `Into<X>` (or `tr<X>`, `AsRef<X>`) among a bound list: `X`.
fn into_target(l: ast::TypeBoundList, tr: &str) -> Option<ast::Type> {
    use ra_ap_syntax::ast::HasGenericArgs;
    l.bounds().find_map(|b| {
        let ast::Type::PathType(pt) = b.ty()? else {
            return None;
        };
        let seg = pt.path()?.segment()?;
        if seg.name_ref()?.text() != tr {
            return None;
        }
        match seg.generic_arg_list()?.generic_args().next()? {
            ast::GenericArg::TypeArg(a) => a.ty(),
            _ => None,
        }
    })
}

/// The `X` of a `t: Into<X>` (`tr<X>`) bound among generic parameters (inline or in the `where`
/// clause).
pub(super) fn param_into_target(
    t: &str,
    tr: &str,
    gl: Option<ast::GenericParamList>,
    wc: Option<ast::WhereClause>,
) -> Option<ast::Type> {
    use ra_ap_syntax::ast::{HasName, HasTypeBounds};
    let inline = gl
        .into_iter()
        .flat_map(|l| l.type_or_const_params())
        .filter_map(|p| match p {
            ast::TypeOrConstParam::Type(tp) => Some(tp),
            _ => None,
        })
        .find(|tp| tp.name().is_some_and(|n| n.text() == t))
        .and_then(|tp| tp.type_bound_list())
        .and_then(|l| into_target(l, tr));
    inline.or_else(|| {
        wc?.predicates().find_map(|pred| {
            if pred.ty()?.syntax().text() != t {
                return None;
            }
            into_target(pred.type_bound_list()?, tr)
        })
    })
}

/// The `std::ops` trait of an operator: `Add` for `+`, ..
fn op_trait(op: ArithOp) -> &'static str {
    match op {
        ArithOp::Add => "Add",
        ArithOp::Sub => "Sub",
        ArithOp::Mul => "Mul",
        ArithOp::Div => "Div",
        ArithOp::Rem => "Rem",
        ArithOp::BitAnd => "BitAnd",
        ArithOp::BitOr => "BitOr",
        ArithOp::BitXor => "BitXor",
        ArithOp::Shl => "Shl",
        ArithOp::Shr => "Shr",
    }
}

impl Checker<'_> {
    /// Whether `a` and `b` could be the same type once inference finishes (no side effects).
    pub(super) fn could_be(&self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        if let Ty::MutRef(x) = &a {
            return self.could_be(x, &b);
        }
        if let Ty::MutRef(y) = &b {
            return self.could_be(&a, y);
        }
        match (&a, &b) {
            (Ty::Unknown, _) | (_, Ty::Unknown) | (Ty::Never, _) | (_, Ty::Never) => true,
            (Ty::Var(_), Ty::Var(_)) => true,
            (Ty::Var(_), t) | (t, Ty::Var(_)) => {
                let v = if matches!(a, Ty::Var(_)) { &a } else { &b };
                match self.var_kind(v) {
                    Some(VarKind::Int) => matches!(t, Ty::Int(_) | Ty::IntLit),
                    Some(VarKind::Float) => matches!(t, Ty::Float { .. }),
                    _ => true,
                }
            }
            (Ty::Adt(x, xa), Ty::Adt(y, ya)) => {
                x == y
                    && xa.len() == ya.len()
                    && xa.iter().zip(ya).all(|(p, q)| self.could_be(p, q))
            }
            _ if std::mem::discriminant(&a) == std::mem::discriminant(&b) => {
                let (ia, ib) = (a.inner_types(), b.inner_types());
                match (&a, &b) {
                    (Ty::Int(x), Ty::Int(y)) => x == y,
                    (Ty::Float { f32: x }, Ty::Float { f32: y }) => x == y,
                    (Ty::Dyn(x), Ty::Dyn(y)) | (Ty::Extern(x), Ty::Extern(y)) => x == y,
                    (Ty::Range(x, k1), Ty::Range(y, k2)) => k1 == k2 && self.could_be(x, y),
                    _ => {
                        ia.len() == ib.len() && ia.iter().zip(&ib).all(|(p, q)| self.could_be(p, q))
                    }
                }
            }
            // (As `unify` takes it: any iterable of `T` but an `Option`.)
            (Ty::IntoIter(e), other) | (other, Ty::IntoIter(e))
                if !matches!(other, Ty::Option(_) | Ty::Param(_)) =>
            {
                self.iter_elem(other)
                    .is_some_and(|item| self.could_be(e, &item))
            }
            _ => false,
        }
    }

    /// Whether a `src` number converts into the number type `x` with `.into()` (for an
    /// `impl<N: Into<i64>> From<N>`: a narrower integer, a `bool`).
    fn number_into(&self, src: &Ty, x: &Ty) -> bool {
        let num = |t: &Ty| {
            matches!(t, Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool)
                || self.var_kind(t).is_some_and(|k| k != VarKind::General)
        };
        num(&self.shallow(src)) && matches!(self.shallow(x), Ty::Int(_) | Ty::Float { .. })
    }

    /// The `impl From<X> for adt` that converts a `src`, if exactly one can; unifies `X` with it.
    pub(super) fn pick_from(&mut self, adt: &str, src: &Ty) -> Option<String> {
        self.pick_from_inst(adt, src).map(|(key, _)| key)
    }

    /// `pick_from`, with the impl's result type: `impl<T> From<T> for Wrap<T>` takes any `T`
    /// (its parameters fresh), and makes a `Wrap` of it.
    fn pick_from_inst(&mut self, adt: &str, src: &Ty) -> Option<(String, Option<Ty>)> {
        let mut impls: Vec<FromImpl> = self.adts.from_impls.get(adt).cloned().unwrap_or_default();
        let mut rets: std::collections::HashMap<String, Ty> = std::collections::HashMap::new();
        for i in &mut impls {
            if let Some(sig) = self.adts.method(adt, &i.key).cloned()
                && sig.impl_args.iter().any(|a| {
                    // (`impl<T> From<Vec<T>> for Cow<'_, [T]>`: the parameter may be nested; and
                    // `impl<A: Array> From<&[A::Item]> for SmallVec<A>`: through a projection.)
                    let mut ps = Vec::new();
                    a.collect_params(&mut ps);
                    let mut srcs = Vec::new();
                    i.src.collect_params(&mut srcs);
                    ps.iter().any(|p| {
                        srcs.iter().any(|s| {
                            s == p || s.strip_prefix(p.as_str()).is_some_and(|r| r.starts_with("::"))
                        })
                    })
                })
            {
                let sig = self.method_inst(&sig, None);
                self.last_type_args.clear();
                if let Some(p) = sig.params.first() {
                    i.src = p.clone();
                }
                rets.insert(i.key.clone(), sig.ret);
            }
        }
        let fits: Vec<&FromImpl> = impls
            .iter()
            .filter(|i| {
                self.could_be(&i.src, src)
                    || (i.via_into && self.number_into(src, &i.src))
                    || (i.via_as_ref
                        && matches!(&i.src, Ty::Vec(e) if **e == Ty::Int(crate::ty::IntKind::U8))
                        && matches!(self.shallow(src), Ty::Str))
            })
            .collect();
        // (An integer literal several impls take is an `i32`, as rustc falls back to.)
        let int_lit = matches!(self.zonk(src), Ty::IntLit)
            || (matches!(self.shallow(src), Ty::Var(_))
                && self.var_kind(src) == Some(super::VarKind::Int));
        let fits = if fits.len() > 1 && int_lit {
            fits.into_iter()
                .filter(|i| i.src == Ty::Int(crate::ty::IntKind::I32))
                .collect()
        } else {
            fits
        };
        // `From<&str>` and `From<String>`: one source type here (references are erased), so
        // either impl converts it; the first serves.
        let fits = match fits.split_first() {
            Some((first, rest)) if !rest.is_empty() && rest.iter().all(|i| i.src == first.src) => {
                vec![*first]
            }
            _ => fits,
        };
        let [one] = fits.as_slice() else { return None };
        if self.could_be(&one.src, src) {
            self.unify(&one.src, src);
        }
        Some((one.key.clone(), rets.remove(&one.key)))
    }

    /// `self.0.partial_cmp(other)` on a `u16` with an `impl PartialOrd<StatusCode> for u16` in the
    /// file, `self.eq(other)` with an `impl PartialEq<str> for P` next to a derived `PartialEq`:
    /// when every impl of the method for the receiver's type compares it with another type and
    /// the argument is of the receiver's own, the call is the derived or std comparison (its key
    /// `$std`, for codegen).
    pub(super) fn std_cmp_call(
        &mut self,
        m: &ast::MethodCallExpr,
        rt: &Ty,
        method: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        if !matches!(
            method,
            "eq" | "ne" | "partial_cmp" | "lt" | "le" | "gt" | "ge"
        ) {
            return None;
        }
        let [a] = args else { return None };
        let t = self.shallow(rt);
        let key = match &t {
            Ty::Adt(n, _) => n.clone(),
            t => crate::adt::ext_key_of(t)?.to_string(),
        };
        let keys = self
            .adts
            .method_alts
            .get(&format!("{key}::{method}"))
            .cloned()
            .unwrap_or_else(|| vec![method.to_string()]);
        if !keys
            .iter()
            .all(|k| self.adts.foreign_cmp.contains(&format!("{key}::{k}")))
        {
            return None;
        }
        let at = self.infer(a, None);
        let foreign = keys.iter().any(|k| {
            self.adts
                .method(&key, k)
                .and_then(|s| s.params.first())
                .is_some_and(|p| self.could_be(p, &at))
        });
        if foreign || !self.could_be(&t, &at) {
            return None;
        }
        self.unify(&t, &at);
        if matches!(t, Ty::Adt(..)) {
            let tr = if method == "eq" || method == "ne" {
                "PartialEq"
            } else {
                "PartialOrd"
            };
            self.require_trait(m.syntax(), &t, tr);
        }
        let r = self.range(m.syntax());
        self.method_keys.insert(r, "$std".into());
        Some(if method == "partial_cmp" {
            Ty::Option(Box::new(Ty::Ordering))
        } else {
            Ty::Bool
        })
    }

    /// `v.into_s()` where a generic trait is implemented more than once for `ty`
    /// (`impl IntoS<u8> for Vec<u8>`, `impl IntoS<u16> for Vec<u8>`): the key of the impl whose
    /// signature fits the arguments and the expected type (the first that fits when several do),
    /// recorded for codegen.
    pub(super) fn pick_method_alt(
        &mut self,
        m: &ast::MethodCallExpr,
        ty: &str,
        targs: &[Ty],
        method: &str,
        args: &[ast::Expr],
        expected: Option<&Ty>,
    ) -> Option<String> {
        let mut keys = self
            .adts
            .method_alts
            .get(&format!("{ty}::{method}"))?
            .clone();
        // Inherent impls for other instantiations: the one for the receiver's type arguments.
        if self.adts.inherent_alts.contains(&format!("{ty}::{method}")) {
            keys = self.alts_for_args(ty, keys, targs);
            // (`W(2).get()` with `impl W<i32>` and `impl W<u8>`: the literal's type isn't
            // settled yet, so rustc can't choose.)
            let mut open_lit = false;
            for t in targs {
                self.resolve_vars(t)
                    .walk(&mut |x| open_lit |= *x == Ty::IntLit);
            }
            // (Impls of a trait for several tuples, `(u8, u8)` and `(i32, i32)`: rustc's
            // integer fallback, `(1, 2).k()` is the `i32` one's.)
            if keys.len() > 1 && open_lit && crate::adt::tuple_arity(ty).is_some() {
                let defaulted: Vec<Ty> = targs
                    .iter()
                    .map(|t| match self.resolve_vars(t) {
                        Ty::IntLit => Ty::Int(IntKind::I32),
                        t => t,
                    })
                    .collect();
                keys = self.alts_for_args(ty, keys, &defaulted);
            } else if keys.len() > 1 && open_lit {
                self.error(
                    m.syntax(),
                    format!(
                        "multiple applicable items in scope: `{method}` is defined in `impl`s for several instantiations of `{ty}`, and the receiver's type isn't settled yet (give the number a suffix or a type)"
                    ),
                );
            }
        }
        // (The arguments' types decide between `Scale<u32>` and `Scale<&str>`; a closure is left
        // to the impl's own check.)
        let arg_tys: Vec<Ty> = args
            .iter()
            .map(|a| match crate::typeck::strip_parens(a) {
                ast::Expr::ClosureExpr(_) => Ty::Unknown,
                _ => self.infer(a, None),
            })
            .collect();
        let fits = |c: &Self, k: &String| {
            c.adts.method(ty, k).is_some_and(|sig| {
                sig.params.len() == args.len()
                    && sig
                        .params
                        .iter()
                        .zip(&arg_tys)
                        .all(|(p, a)| matches!(a, Ty::Unknown) || c.could_be(p, a))
                    && expected.is_none_or(|e| c.could_be(&sig.ret, e))
            })
        };
        let fitting: Vec<String> = keys.iter().filter(|k| fits(self, k)).cloned().collect();
        let key = self
            .narrow_alts(ty, method, fitting, m.syntax())
            .unwrap_or_else(|| keys[0].clone());
        let r = self.range(m.syntax());
        self.method_keys.insert(r, key.clone());
        Some(key)
    }

    /// Of `keys` (one method's keys in inherent impls for several instantiations of `ty`), those
    /// whose impl could be for type arguments `targs` (all of them when none could).
    pub(super) fn alts_for_args(&self, ty: &str, keys: Vec<String>, targs: &[Ty]) -> Vec<String> {
        let targs: Vec<Ty> = targs.iter().map(|t| self.resolve_vars(t)).collect();
        let fits: Vec<String> = keys
            .iter()
            .filter(|k| {
                self.adts.method(ty, k).is_some_and(|sig| {
                    sig.impl_args.len() != targs.len()
                        || !sig
                            .impl_args
                            .iter()
                            .zip(&targs)
                            .any(|(i, t)| crate::adt::disjoint_args(i, t))
                })
            })
            .cloned()
            .collect();
        if fits.is_empty() { keys } else { fits }
    }

    /// `t` with its settled variables filled in; an integer literal's still open variable is
    /// `IntLit`, any other open one stays.
    fn resolve_vars(&self, t: &Ty) -> Ty {
        let r = |x: &Ty| Box::new(self.resolve_vars(x));
        match self.shallow(t) {
            v @ Ty::Var(_) => match self.var_kind(&v) {
                Some(VarKind::Int) => Ty::IntLit,
                // (`(1.5, 2.0).kind()` with impls for `(f64, f64)` and `(u8, u8)`.)
                Some(VarKind::Float) => Ty::Float { f32: false },
                _ => v,
            },
            Ty::Adt(n, args) => Ty::Adt(n, args.iter().map(|a| self.resolve_vars(a)).collect()),
            Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| self.resolve_vars(e)).collect()),
            Ty::Vec(e) => Ty::Vec(r(&e)),
            Ty::Option(e) => Ty::Option(r(&e)),
            Ty::Set(e) => Ty::Set(r(&e)),
            Ty::Map(k, v) => Ty::Map(r(&k), r(&v)),
            Ty::Result(k, v) => Ty::Result(r(&k), r(&v)),
            other => other,
        }
    }

    /// `method` of `ty` among several impls of one generic trait, by its argument `idx`
    /// (`impl TryFrom<&str>` and `impl TryFrom<Vec<u8>>`) or, for an `IntoIterator<Item = X>`
    /// parameter, by the items (`impl FromIterator<Tok>`, `impl Extend<Stream>`): the first
    /// impl that takes `arg` (see `narrow_alts`), recorded for codegen at `node`. The method's
    /// own name when the type has one impl.
    pub(super) fn pick_alt_by_item(
        &mut self,
        ty: &str,
        method: &str,
        idx: usize,
        arg: &Ty,
        node: &SyntaxNode,
    ) -> String {
        let Some(keys) = self
            .adts
            .method_alts
            .get(&format!("{ty}::{method}"))
            .cloned()
        else {
            return method.to_string();
        };
        let fits = |c: &Self, k: &String| {
            c.adts
                .method(ty, k)
                .and_then(|sig| sig.params.get(idx))
                .is_some_and(|p| match c.shallow(p) {
                    Ty::IntoIter(x) if !matches!(c.shallow(arg), Ty::IntoIter(_)) => {
                        c.could_be(&x, arg)
                    }
                    p => c.could_be(&p, arg),
                })
        };
        let fitting: Vec<String> = keys.iter().filter(|k| fits(self, k)).cloned().collect();
        let key = self
            .narrow_alts(ty, method, fitting, node)
            .unwrap_or_else(|| keys[0].clone());
        let r = self.range(node);
        self.method_keys.insert(r, key.clone());
        key
    }

    /// The impl to call among those whose argument types fit (`fitting`, in order). Impls whose
    /// argument types differ only by references (`TryFrom<&str>` and `TryFrom<String>`,
    /// `Extend<&T>` and `Extend<T>`) take the same JS value, so how the call's argument is
    /// written decides (`s.as_str()`, `s.to_string()`, `&s`, `v.iter()`; see `arg_ref_hint`).
    /// Failing that, never the impl the call is in (`Self::try_from(s.as_str())` in the
    /// `String` impl hands on to another), then the first.
    fn narrow_alts(
        &self,
        ty: &str,
        method: &str,
        fitting: Vec<String>,
        node: &SyntaxNode,
    ) -> Option<String> {
        let mut fitting = fitting;
        let written = |k: &String| self.adts.alt_arg_refs.get(&format!("{ty}::{k}")).copied();
        if fitting.len() > 1
            && let Some(h) = arg_ref_hint(node)
        {
            let mut same: Vec<String> = fitting
                .iter()
                .filter(|k| written(k) == Some(h))
                .cloned()
                .collect();
            // (`&s` of a `String` into `TryFrom<&str>` when there's no `TryFrom<&String>`.)
            if same.is_empty() && h == ArgRef::Ref {
                same = fitting
                    .iter()
                    .filter(|k| written(k) == Some(ArgRef::Slice))
                    .cloned()
                    .collect();
            }
            if !same.is_empty() {
                fitting = same;
            }
        }
        if fitting.len() > 1
            && let Some(f) = node.ancestors().find_map(ast::Fn::cast)
            && let Some(n) = ast::HasName::name(&f)
            && n.text() == method
        {
            let at = self.range(n.syntax()).0;
            let own = self
                .adts
                .alt_keys_at
                .get(&at)
                .cloned()
                .unwrap_or_else(|| method.to_string());
            fitting.retain(|k| *k != own);
        }
        fitting.into_iter().next()
    }

    /// A call of a user trait method on a built-in type, for codegen.
    pub(super) fn record_ext(&mut self, node: &SyntaxNode, key: &str) {
        self.record_conv(
            node,
            Conv::Ext {
                key: key.to_string(),
            },
        );
    }

    pub(super) fn record_conv(&mut self, node: &SyntaxNode, conv: Conv) {
        let r = self.range(node);
        self.conversions.insert(r, conv);
    }

    fn conv_impl_list(&self, adt: &str) -> String {
        let impls = self.adts.from_impls.get(adt).cloned().unwrap_or_default();
        impls
            .iter()
            .map(|i| format!("`From<{}>`", self.describe(&i.src)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// `Y::from(x)` for a struct or enum with `impl From<X> for Y`.
    pub(super) fn conv_from_call(
        &mut self,
        c: &ast::CallExpr,
        adt: &str,
        args: &[ast::Expr],
    ) -> Ty {
        self.arg_count(c.syntax(), "function", 1, args.len());
        let Some(a) = args.first() else {
            return Ty::Unknown;
        };
        let at = self.infer(a, None);
        match self.pick_from_inst(adt, &at) {
            Some((key, ret)) => {
                self.record_conv(
                    c.syntax(),
                    Conv::From {
                        adt: adt.to_string(),
                        key,
                    },
                );
                if let Some(ret) = ret {
                    return ret;
                }
            }
            None => {
                let msg = format!(
                    "the trait bound `{adt}: From<{}>` is not satisfied (`{adt}` implements {})",
                    self.describe(&at),
                    self.conv_impl_list(adt)
                );
                self.error(c.syntax(), msg);
            }
        }
        let targs = self.fresh_args(adt);
        Ty::Adt(adt.to_string(), targs)
    }

    /// `b.as_ref()` with `b: B` where `B: AsRef<X>` (also `impl AsRef<X>`, `as_mut`, `borrow`,
    /// `borrow_mut`): the parameter is an `X` here already, so the call is the value itself
    /// (not, say, the path prelude's `as_ref` on a string).
    pub(super) fn conv_as_ref_call(&mut self, m: &ast::MethodCallExpr, method: &str) -> Option<Ty> {
        let tr = match method {
            "as_ref" => "AsRef",
            "as_mut" => "AsMut",
            "borrow" => "Borrow",
            "borrow_mut" => "BorrowMut",
            _ => return None,
        };
        if m.arg_list().is_some_and(|l| l.args().next().is_some()) {
            return None;
        }
        let x = into_bound_target(m, tr)?;
        let t = self.lower_type(&x);
        // (Bytes: callers convert a string where they pass it, but a value of an impl's
        // `H: AsRef<[u8]>` handed on as it is may still be one.)
        let bytes = matches!(&t, Ty::Vec(e) if **e == Ty::Int(IntKind::U8));
        self.record_conv(
            m.syntax(),
            if bytes {
                Conv::Wrap { js: "κσ.$asRefBytes".into() }
            } else {
                Conv::Identity
            },
        );
        Some(t)
    }

    /// Whether the type parameter `p` (of the function or `impl` around `at`) has an `AsRef<X>`,
    /// `Into<X>` or `Borrow<X>` bound whose `X` could be `target`.
    pub(super) fn param_converts_to(&mut self, p: &str, at: &SyntaxNode, target: &Ty) -> bool {
        use ra_ap_syntax::ast::HasGenericParams;
        let f = at.ancestors().find_map(ast::Fn::cast);
        let imp = at.ancestors().find_map(ast::Impl::cast);
        for tr in ["AsRef", "Into", "Borrow"] {
            let x = f
                .as_ref()
                .and_then(|f| param_into_target(p, tr, f.generic_param_list(), f.where_clause()))
                .or_else(|| {
                    let imp = imp.as_ref()?;
                    param_into_target(p, tr, imp.generic_param_list(), imp.where_clause())
                });
            if let Some(x) = x {
                let x = self.lower_type(&x);
                if self.could_be(&x, target) {
                    return true;
                }
            }
        }
        false
    }

    /// `s.as_ref()` on a string where a `&[u8]`, `&str` or `&OsStr` is expected: its bytes or
    /// itself (not the path prelude's `AsRef<Path>`, which a `use std::path` brings in).
    pub(super) fn conv_str_as_ref_call(
        &mut self,
        m: &ast::MethodCallExpr,
        method: &str,
        recv: &Ty,
        expected: Option<&Ty>,
    ) -> Option<Ty> {
        if method != "as_ref"
            || m.arg_list().is_some_and(|l| l.args().next().is_some())
            || !matches!(self.shallow(recv), Ty::Str)
        {
            return None;
        }
        let e = self.shallow(expected?);
        match &e {
            Ty::Str => self.record_conv(m.syntax(), Conv::Identity),
            Ty::Vec(x) if self.shallow(x) == Ty::Int(IntKind::U8) => self.record_conv(
                m.syntax(),
                Conv::Wrap {
                    js: "κσ.$utf8".into(),
                },
            ),
            _ => return None,
        }
        Some(e)
    }

    /// `self.key.as_ref()` (also `as_mut`, `borrow`, `borrow_mut`) where `key: T` and the
    /// impl's `T: AsRef<X>` (or a `where` clause says so). A function's own parameters are `X`
    /// already (converted where they're passed), but a value of the impl's type is whatever it
    /// was built from: a string for an `AsRef<[u8]>` converts to its bytes here, at run time.
    pub(super) fn conv_param_as_ref_call(
        &mut self,
        m: &ast::MethodCallExpr,
        method: &str,
        recv: &Ty,
    ) -> Option<Ty> {
        use ra_ap_syntax::ast::HasGenericParams;
        let tr = match method {
            "as_ref" => "AsRef",
            "as_mut" => "AsMut",
            "borrow" => "Borrow",
            "borrow_mut" => "BorrowMut",
            _ => return None,
        };
        if m.arg_list().is_some_and(|l| l.args().next().is_some()) {
            return None;
        }
        let Ty::Param(p) = self.shallow(recv) else {
            return None;
        };
        let f = m.syntax().ancestors().find_map(ast::Fn::cast);
        let x = f
            .as_ref()
            .and_then(|f| param_into_target(&p, tr, f.generic_param_list(), f.where_clause()))
            .or_else(|| {
                let imp = m.syntax().ancestors().find_map(ast::Impl::cast)?;
                param_into_target(&p, tr, imp.generic_param_list(), imp.where_clause())
            })?;
        let t = self.lower_type(&x);
        let bytes = matches!(&t, Ty::Vec(e) if **e == Ty::Int(IntKind::U8));
        self.record_conv(
            m.syntax(),
            if bytes {
                Conv::Wrap { js: "κσ.$asRefBytes".into() }
            } else {
                Conv::Identity
            },
        );
        Some(t)
    }

    /// `x.into()`: the target type usually comes from the context, so it is settled at the end
    /// of the function.
    pub(super) fn conv_into_call(&mut self, m: &ast::MethodCallExpr, recv: Ty) -> Ty {
        // `h.into()` with `h: impl Into<Host>` / `h: H` where `H: Into<Host>`: the parameter is a
        // `Host` here already, and the bound says that's what it converts to.
        if let Some(x) = into_bound_target(m, "Into") {
            let t = self.lower_type(&x);
            // (`P: Into<i64>` stays a parameter, so it may be any narrower number or a `bool`.)
            self.record_conv(m.syntax(), into_number_conv(&t));
            return t;
        }
        let target = self.fresh(VarKind::General);
        self.pending_intos
            .push((recv, target.clone(), m.syntax().clone(), self.base));
        target
    }

    /// `x.try_into()`: `Result<T, E>`, with `T` from the context.
    pub(super) fn conv_try_into_call(&mut self, m: &ast::MethodCallExpr, recv: Ty) -> Ty {
        let target = self.fresh(VarKind::General);
        let err = self.fresh(VarKind::General);
        self.pending_try_intos.push((
            recv,
            target.clone(),
            err.clone(),
            m.syntax().clone(),
            self.base,
        ));
        Ty::Result(Box::new(target), Box::new(err))
    }

    /// Settle each `x.try_into()`: an integer conversion (range-checked), a user `TryFrom`
    /// impl, or a slice to an array.
    pub(super) fn resolve_try_intos(&mut self) {
        let saved = self.base;
        for (src, target, err, node, base) in std::mem::take(&mut self.pending_try_intos) {
            self.base = base;
            let s = if self.var_kind(&src) == Some(VarKind::Int) {
                Ty::IntLit
            } else {
                self.shallow(&src)
            };
            let t = self.shallow(&target);
            let conv = match (&s, &t) {
                (_, Ty::Var(_)) if self.var_kind(&t) != Some(VarKind::Int) => {
                    // (Often a call nothing typed, already an error.)
                    if !self.diags.iter().any(|d| d.severity == "error") {
                        self.error(&node, "type annotations needed: say what `.try_into()` converts to (e.g. `let y: u8 = x.try_into().unwrap();`)");
                    }
                    continue;
                }
                (_, Ty::Param(p)) => {
                    let msg =
                        format!("`.try_into()` to a type parameter (`{p}`) isn't supported yet");
                    self.error(&node, msg);
                    continue;
                }
                (Ty::Int(_) | Ty::IntLit | Ty::Unknown | Ty::Var(_), Ty::Char) => {
                    self.unify(&err, &Ty::Extern("CharTryFromError".into()));
                    Some(Conv::TryChar)
                }
                (Ty::Int(_) | Ty::IntLit | Ty::Unknown | Ty::Var(_), Ty::Int(_) | Ty::Var(_)) => {
                    let k = match t {
                        Ty::Int(k) => k,
                        _ => {
                            self.unify(&target, &Ty::Int(IntKind::I32));
                            IntKind::I32
                        }
                    };
                    self.unify(&err, &Ty::Extern("TryFromIntError".into()));
                    Some(Conv::TryInt {
                        bits: k.bits(),
                        signed: k.is_signed(),
                        big: k.is_big(),
                    })
                }
                (_, Ty::Adt(adt, args)) if self.adts.method(adt, "try_from").is_some() => {
                    let (adt, args) = (adt.clone(), args.clone());
                    // (Several `impl TryFrom<X>`: the one for this source.)
                    let key = self.pick_alt_by_item(&adt, "try_from", 0, &src, &node);
                    let m = self.adts.method(&adt, &key).cloned().unwrap();
                    let m = self.method_inst(&m, Some(&args));
                    let ok = m.params.first().is_some_and(|p| self.unify(p, &src))
                        && self.unify(
                            &m.ret,
                            &Ty::Result(Box::new(target.clone()), Box::new(err.clone())),
                        );
                    ok.then_some(Conv::TryFrom { adt, key })
                }
                (Ty::Vec(x), Ty::Vec(y)) => {
                    let (x, y) = (x.clone(), y.clone());
                    let m = ast::MethodCallExpr::cast(node.clone());
                    // An owned `Vec` (`v.try_into()` of a `Vec` local, `v.clone()`) is the
                    // `Err` itself; a slice's is a `TryFromSliceError`.
                    let owned = m
                        .as_ref()
                        .and_then(|m| m.receiver())
                        .is_some_and(|r| self.certainly_owned(&r));
                    let len = m.as_ref().and_then(super::arrays::try_into_array_len);
                    if owned {
                        self.unify(&err, &Ty::Vec(x.clone()));
                    } else {
                        self.unify(&err, &Ty::Extern("TryFromSliceError".into()));
                    }
                    self.unify(&x, &y).then_some(Conv::TryVec { len, owned })
                }
                _ => None,
            };
            match conv {
                Some(c) => self.record_conv(&node, c),
                None => {
                    let msg = format!(
                        "the trait bound `{}: TryFrom<{}>` is not satisfied",
                        self.describe(&t),
                        self.describe(&s)
                    );
                    self.error(&node, msg);
                }
            }
        }
        self.base = saved;
    }

    /// An argument for an `impl Into<X>` parameter: an `X`, or what an `impl From<..> for X`
    /// converts (the conversion is settled with the other `.into()`s).
    pub(super) fn check_into_arg(&mut self, a: &ast::Expr, p: &Ty) -> Ty {
        let target = self.shallow(p);
        let from_target =
            matches!(&target, Ty::Adt(tn, _) if self.adts.from_impls.contains_key(tn));
        // (A suffixed number literal has a type of its own: `4u8` into an `impl Into<i64>`.)
        let suffixed = matches!(target, Ty::Int(_) | Ty::Float { .. })
            && matches!(crate::typeck::strip_parens(a), ast::Expr::Literal(l)
                if matches!(l.kind(), ast::LiteralKind::IntNumber(n) if n.suffix().is_some())
                    || matches!(l.kind(), ast::LiteralKind::FloatNumber(n) if n.suffix().is_some()));
        // (A string literal for an `impl AsRef<[u8]>` converts to its bytes.)
        let str_bytes = matches!(target, Ty::Vec(_))
            && matches!(crate::typeck::strip_parens(a), ast::Expr::Literal(l)
                if matches!(l.kind(), ast::LiteralKind::String(_)));
        // (Literals, closures and the like take the parameter's type as written.)
        if !from_target
            && !suffixed
            && !str_bytes
            && !matches!(
                crate::typeck::strip_parens(a),
                ast::Expr::PathExpr(_)
                    | ast::Expr::CallExpr(_)
                    | ast::Expr::MethodCallExpr(_)
                    | ast::Expr::FieldExpr(_)
                    | ast::Expr::RefExpr(_)
                    | ast::Expr::IndexExpr(_)
                    | ast::Expr::MacroExpr(_)
            )
        {
            return self.check(a, p);
        }
        // (`repeat(0..)` into an `impl Into<Count>` with `impl From<RangeFrom<usize>> for
        // Count`: the range is a value, not an iterator.)
        let range_src = matches!(crate::typeck::strip_parens(a), ast::Expr::RangeExpr(_))
            && matches!(&target, Ty::Adt(tn, _) if self.adts.from_impls.get(tn)
                .is_some_and(|fs| fs.iter().any(|f| matches!(f.src, Ty::Range(..)))));
        let hint = range_src.then(|| Ty::Range(Box::new(Ty::Unknown), crate::ty::RangeKind::Full));
        let t = self.infer(a, hint.as_ref());
        if self.could_be(&t, p) {
            self.unify(&t, p);
            return p.clone();
        }
        // `Input::new(haystack)` with the impl's `H: AsRef<[u8]>`: an `H` is a `[u8]` already
        // (its `as_ref()` makes the bytes where they're read).
        if let Ty::Param(name) = self.shallow(&t)
            && self.param_converts_to(&name, a.syntax(), p)
        {
            return p.clone();
        }
        let r = self.range(a.syntax());
        // `impl AsRef<str>` given your type with `impl AsRef<str>`: its `as_ref()`.
        if let Ty::Adt(n, _) = self.shallow(&t)
            && !matches!(&target, Ty::Adt(tn, _) if self.adts.from_impls.contains_key(tn))
        {
            for m in ["as_ref", "borrow"] {
                if let Some(sig) = self.adts.method(&n, m)
                    && self.could_be(&sig.ret.clone(), p)
                {
                    self.into_args.insert(r);
                    self.record_conv(a.syntax(), Conv::Method { key: m.to_string() });
                    return p.clone();
                }
            }
        }
        self.into_args.insert(r);
        self.pending_intos
            .push((t, p.clone(), a.syntax().clone(), self.base));
        p.clone()
    }

    pub(super) fn resolve_intos(&mut self) {
        let saved = self.base;
        for (src, target, node, base) in std::mem::take(&mut self.pending_intos) {
            // Calls inside macro arguments were parsed from the argument's text.
            self.base = base;
            let (s, t) = (self.shallow(&src), self.shallow(&target));
            // `x.into()` with `x: T` where `T: Into<f64>`: the number as it is.
            if let Ty::Param(p) = &s
                && let Some(into) = self.into_targets.get(p).cloned()
            {
                self.unify(&t, &into);
                self.record_conv(&node, into_number_conv(&into));
                continue;
            }
            let conv = match (&s, &t) {
                (_, Ty::Var(_)) if self.var_kind(&t) == Some(VarKind::General) => {
                    self.error(&node, "type annotations needed: say what `.into()` converts to (e.g. `let y: Y = x.into();`)");
                    continue;
                }
                // `sq.into()` as a `Box<dyn Shape>` through your `impl From<Sq> for Box<dyn Shape>`.
                (_, Ty::Dyn(tr)) if self.adts.from_impls.contains_key(&format!("dyn${tr}")) => {
                    let adt = format!("dyn${tr}");
                    self.pick_from(&adt, &s).map(|key| Conv::From { adt, key })
                }
                (_, Ty::Adt(adt, _))
                    if self.adts.from_impls.contains_key(adt) && !self.could_be(&s, &t) =>
                {
                    let adt = adt.clone();
                    self.pick_from_inst(&adt, &s).map(|(key, ret)| {
                        if let Some(ret) = ret {
                            self.unify(&ret, &t);
                        }
                        Conv::From { adt, key }
                    })
                }
                // `id.into()` as a `String` (or a number) through your `impl From<Id> for String`.
                (Ty::Adt(..), _)
                    if crate::adt::ext_key_of(&t)
                        .is_some_and(|k| self.adts.from_impls.contains_key(k)) =>
                {
                    let adt = crate::adt::ext_key_of(&t).unwrap_or_default().to_string();
                    self.pick_from(&adt, &s).map(|key| Conv::From { adt, key })
                }
                // `"x".into()` / `format!(..).into()` as a `Box<dyn Error>`: the string itself (its
                // `{}` is the text and its `{:?}` the quoted text, like std's string error's).
                (Ty::Str, Ty::Dyn(tr))
                    if tr == "Error" && !self.adts.traits.contains_key("Error") =>
                {
                    Some(Conv::Identity)
                }
                // `MyError.into()` as a `Box<dyn Error>`, with `impl Error for MyError`: unchanged,
                // as `?` converts it.
                (_, Ty::Dyn(tr))
                    if tr == "Error"
                        && !self.adts.traits.contains_key("Error")
                        && self.boxes_as_dyn_error(&s) =>
                {
                    Some(Conv::Identity)
                }
                // `arg.into()` of a bindings enum (`ScriptArg`) as a `JsValue`: the value itself.
                (Ty::Extern(src), Ty::Extern(n))
                    if n == "JsValue" && self.bindings.is_enum(src) =>
                {
                    Some(Conv::Identity)
                }
                // `"home".into()` / `1.0.into()` / `vec![].into()` as a bindings enum (`FlagValue`):
                // a variant's payload is the value itself.
                (src, Ty::Extern(n))
                    if self.bindings.is_enum(n) && !matches!(src, Ty::Extern(_)) =>
                {
                    let b = self.bindings.clone();
                    let variants = b.enum_of(n).map(|et| et.variants.as_slice()).unwrap_or_default();
                    let float = variants.iter().any(|v| matches!(v.payload, Ty::Float { .. }));
                    if matches!(src, Ty::Var(_))
                        && matches!(self.var_kind(&s), Some(VarKind::Float | VarKind::Int))
                    {
                        float.then(|| {
                            self.unify(&s, &Ty::Float { f32: false });
                            Conv::Identity
                        })
                    } else {
                        variants.iter().find(|v| self.could_be(&s, &v.payload)).map(|fit| {
                            self.unify(&s, &fit.payload);
                            Conv::Identity
                        })
                    }
                }
                // `"x".into()` / `2.5.into()` / `true.into()` as a `JsValue`: the JS value itself.
                (
                    Ty::Str
                    | Ty::Int(_)
                    | Ty::IntLit
                    | Ty::Float { .. }
                    | Ty::Bool
                    | Ty::Char
                    | Ty::Var(_),
                    Ty::Extern(n),
                ) if n == "JsValue" => {
                    if matches!(s, Ty::Var(_)) && self.var_kind(&s) == Some(VarKind::Float) {
                        self.unify(&s, &Ty::Float { f32: false });
                    } else if matches!(s, Ty::Var(_)) && self.var_kind(&s) == Some(VarKind::Int) {
                        self.unify(&s, &Ty::Int(IntKind::I32));
                    }
                    Some(Conv::Identity)
                }
                // `s.into()` as a `Vec<u8>` (also a string for an `impl AsRef<[u8]>`): its UTF-8
                // bytes.
                (Ty::Str, Ty::Vec(e)) if self.could_be(e, &Ty::Int(IntKind::U8)) => {
                    self.unify(e, &Ty::Int(IntKind::U8));
                    Some(Conv::Wrap { js: "κσ.$utf8".into() })
                }
                // `vec.into()` as a `VecDeque` (or another collection made from a `Vec`), and
                // back.
                (Ty::Vec(e), Ty::Coll(kind, view)) => {
                    use crate::ty::Coll;
                    let items = match &**view {
                        Ty::Map(k, v) => Ty::Tuple(vec![(**k).clone(), (**v).clone()]),
                        Ty::Vec(x) | Ty::Set(x) => (**x).clone(),
                        _ => Ty::Unknown,
                    };
                    self.unify(e, &items);
                    let js = match kind {
                        Coll::VecDeque => "κσ.Deque.from",
                        Coll::BinaryHeap => "κσ.$heapFrom",
                        Coll::BTreeSet => "κσ.BTreeSet.from",
                        Coll::BTreeMap => "κσ.BTreeMap.from",
                        Coll::IndexSet => "κσ.IndexSet.from",
                        Coll::IndexMap => "κσ.IndexMap.from",
                        Coll::FixedArray(_) => "Array.from",
                    };
                    Some(Conv::Wrap { js: js.into() })
                }
                (
                    Ty::Coll(crate::ty::Coll::VecDeque | crate::ty::Coll::BinaryHeap, view),
                    Ty::Vec(e),
                ) => {
                    if let Ty::Vec(x) = &**view {
                        self.unify(x, e);
                    }
                    Some(Conv::Wrap {
                        js: "Array.from".into(),
                    })
                }
                // `let one: i16 = true.into()`: 1 or 0.
                (Ty::Bool, Ty::Int(k)) => Some(Conv::Wrap {
                    js: if k.is_big() { "BigInt" } else { "Number" }.into(),
                }),
                (Ty::Bool, Ty::Float { .. }) => Some(Conv::Wrap { js: "Number".into() }),
                _ if self.lossless(&s, &t) => Some(Conv::Identity),
                _ if self.could_be(&s, &t) => {
                    self.unify(&s, &t);
                    Some(Conv::Identity)
                }
                _ => None,
            };
            match conv {
                Some(c) => self.record_conv(&node, c),
                None => {
                    let msg = format!(
                        "the trait bound `{}: From<{}>` is not satisfied",
                        self.describe(&t),
                        self.describe(&s)
                    );
                    self.error(&node, msg);
                }
            }
        }
        self.base = saved;
    }

    /// Std conversions that are the identity in JS: `&str` → `String`, and lossless numeric
    /// widenings (`i32` → `i64`, `u8` → `u32`, `i32` → `f64`, ..).
    fn lossless(&mut self, s: &Ty, t: &Ty) -> bool {
        use IntKind::*;
        let bits = |k: IntKind| match k {
            I8 | U8 => 8,
            I16 | U16 => 16,
            I32 | U32 => 32,
            I64 | U64 | Isize | Usize => 64,
            I128 | U128 => 128,
        };
        let s = if self.var_kind(s) == Some(VarKind::Int) {
            Ty::IntLit
        } else {
            self.shallow(s)
        };
        match (&s, &self.shallow(t)) {
            (Ty::Str, Ty::Str) => true,
            (Ty::IntLit, Ty::Int(_) | Ty::Float { .. }) => true,
            (Ty::Int(a), Ty::Int(b)) => {
                *a == *b
                    || (a.is_signed() == b.is_signed()
                        && bits(*a) < bits(*b)
                        && !matches!(b, Isize | Usize))
                    || (!a.is_signed()
                        && b.is_signed()
                        && bits(*a) < bits(*b)
                        && !matches!(b, Isize))
                    || matches!((a, b), (U8 | U16, Usize) | (I8 | I16, Isize) | (U8, Isize))
            }
            (Ty::Int(a), Ty::Float { f32 }) => bits(*a) <= if *f32 { 16 } else { 32 },
            (Ty::Float { f32: true }, Ty::Float { .. }) => true,
            _ => false,
        }
    }

    /// The number type whose `impl Mul<V> for f64` (or `Add`, ..) applies to `lt op rt`, with `rt`
    /// your type: `lt`'s own, or for a literal (`2.0 * v`) the first number type with one.
    pub(super) fn prim_op_key(
        &mut self,
        op: ArithOp,
        assign: bool,
        lt: &Ty,
        rt: &Ty,
    ) -> Option<String> {
        if !matches!(self.shallow(rt), Ty::Adt(..)) {
            return None;
        }
        let tr = format!("{}{}", op_trait(op), if assign { "Assign" } else { "" });
        let has = |c: &Self, k: &str| {
            c.adts
                .op_impls
                .get(k)
                .is_some_and(|v| v.iter().any(|i| i.trait_name == tr))
        };
        if let Some(k) = crate::adt::ext_key_of(&self.shallow(lt)) {
            return has(self, k).then(|| k.to_string());
        }
        let candidates: &[&str] = match self.var_kind(lt) {
            Some(VarKind::Float) => &["f64", "f32"],
            Some(VarKind::Int) => &[
                "i32", "i64", "u32", "u64", "usize", "isize", "u8", "i8", "u16", "i16", "f64",
                "f32",
            ],
            _ => &[],
        };
        let k = candidates.iter().find(|k| has(self, k))?.to_string();
        self.unify(lt, &Ty::from_name(&k));
        Some(k)
    }

    /// `a + b` (or `a += b`) where `a` is a struct or enum: its `impl Add<R>` for `b`'s type.
    /// (`pre`: `b`'s type, already inferred.)
    #[allow(clippy::too_many_arguments)]
    pub(super) fn op_overload(
        &mut self,
        node: &SyntaxNode,
        op: ArithOp,
        assign: bool,
        adt: &str,
        lt: &Ty,
        rhs: &ast::Expr,
        pre: Option<Ty>,
    ) -> Ty {
        let base = op_trait(op);
        let tr = if assign {
            format!("{base}Assign")
        } else {
            base.to_string()
        };
        let rt = match pre {
            Some(t) => t,
            None => self.infer(rhs, None),
        };
        let mut impls: Vec<crate::adt::OpImpl> = self
            .adts
            .op_impls
            .get(adt)
            .map(|v| v.iter().filter(|i| i.trait_name == tr).cloned().collect())
            .unwrap_or_default();
        // `impl<Tz> Sub<DateTime<Tz>> for DateTime<Tz>`: the impl's parameters are the left
        // operand's type arguments, in the right operand's type and the result's.
        let mut rets: std::collections::HashMap<String, Ty> = std::collections::HashMap::new();
        if let Ty::Adt(_, targs) = self.shallow(lt) {
            for i in &mut impls {
                if let Some(sig) = self.adts.method(adt, &i.key).cloned()
                    && sig.impl_args.iter().any(|a| matches!(a, Ty::Param(_)))
                {
                    // (`impl<L, R> Add<Complex<R>> for Complex<L>`: `R`, which only the right
                    // operand names, is a fresh variable.)
                    let mut own = std::collections::HashMap::new();
                    for t in sig.params.iter().take(1).chain([&sig.ret]) {
                        t.walk(&mut |x| {
                            if let Ty::Param(p) = x
                                && !sig.impl_args.iter().any(|a| a.mentions_param(p))
                            {
                                own.entry(p.clone()).or_insert(Ty::Unknown);
                            }
                        });
                    }
                    let mut sig = sig;
                    if !own.is_empty() {
                        for v in own.values_mut() {
                            *v = self.fresh(super::VarKind::General);
                        }
                        sig.params = sig
                            .params
                            .iter()
                            .map(|p| super::generic::subst(p, &own))
                            .collect();
                        sig.ret = super::generic::subst(&sig.ret, &own);
                    }
                    let sig = self.method_inst(&sig, Some(&targs));
                    self.last_type_args.clear();
                    if let Some(p) = sig.params.first() {
                        i.rhs = p.clone();
                    }
                    rets.insert(i.key.clone(), sig.ret);
                }
            }
        }
        // `impl Mul for Wrapping<u32>` next to `impl Mul for Wrapping<u64>`: only the impls for the
        // left operand's own type arguments apply.
        if let Ty::Adt(_, targs) = self.shallow(lt) {
            let own: Vec<crate::adt::OpImpl> = impls
                .iter()
                .filter(|i| {
                    self.adts.method(adt, &i.key).is_none_or(|sig| {
                        sig.impl_args.len() != targs.len()
                            || sig
                                .impl_args
                                .iter()
                                .zip(&targs)
                                .all(|(a, t)| matches!(a, Ty::Param(_)) || self.could_be(a, t))
                    })
                })
                .cloned()
                .collect();
            if !own.is_empty() {
                impls = own;
            }
        }
        let mut fits: Vec<&crate::adt::OpImpl> = impls
            .iter()
            .filter(|i| self.could_be(&i.rhs, &rt))
            .collect();
        // (`loot += cache_result` with `impl AddAssign<CacheReward>`: an NS interface it extends.)
        if fits.is_empty() {
            fits = impls
                .iter()
                .filter(|i| self.iface_coerces(&rt, &i.rhs))
                .collect();
        }
        // References are erased, so `impl Add for V` and `impl Add<&V> for &V` (or `Add<&V>
        // for V`) are the same operation here: the first one serves.
        if fits.len() > 1 {
            let first = self.describe(&fits[0].rhs);
            if fits.iter().all(|i| self.describe(&i.rhs) == first) {
                fits.truncate(1);
            }
        }
        if let [one] = fits.as_slice() {
            self.unify(&one.rhs, &rt);
            let key = one.key.clone();
            self.record_conv(node, Conv::Method { key: key.clone() });
            if assign {
                return Ty::Unit;
            }
            if let Some(r) = rets.remove(&key) {
                return r;
            }
            return self
                .adts
                .method(adt, &key)
                .map(|m| m.ret.clone())
                .unwrap_or(Ty::Unknown);
        }
        let (l, r) = (self.describe(lt), self.describe(&rt));
        let sym = format!("{op}{}", if assign { "=" } else { "" });
        let msg = if impls.is_empty() {
            format!(
                "cannot apply `{sym}` to `{l}` and `{r}` (add `impl {tr}<{r}> for {adt}` from `std::ops`)"
            )
        } else {
            format!(
                "no implementation for `{l} {sym} {r}` (`{adt}` implements {})",
                impls
                    .iter()
                    .map(|i| format!("`{tr}<{}>`", self.describe(&i.rhs)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        self.error(node, msg);
        Ty::Unknown
    }

    /// `grid[i]` where `grid` is a struct or enum: its `impl Index<I>` for `i`'s type. Reading
    /// works (also a field or method of the result); assigning to the element itself would need
    /// `IndexMut` to hand out a place, which JS can't do for numbers and strings.
    pub(super) fn index_overload(
        &mut self,
        ix: &ast::IndexExpr,
        adt: &str,
        bt: &Ty,
        index: &ast::Expr,
    ) -> Ty {
        let it = self.infer(index, None);
        let impls: Vec<crate::adt::OpImpl> = self
            .adts
            .op_impls
            .get(adt)
            .map(|v| {
                v.iter()
                    .filter(|i| i.trait_name == "Index")
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        // (`impl<N> Index<(N, N)> for GraphMap<N>`: the index type in the base's type arguments.)
        let mut impls = impls;
        if let Ty::Adt(_, targs) = self.shallow(bt) {
            for i in &mut impls {
                if let Some(sig) = self.adts.method(adt, &i.key).cloned()
                    && sig.impl_args.iter().any(|a| matches!(a, Ty::Param(_)))
                {
                    let sig = self.method_inst(&sig, Some(&targs));
                    self.last_type_args.clear();
                    if let Some(p) = sig.params.first() {
                        i.rhs = p.clone();
                    }
                }
            }
        }
        let fits: Vec<&crate::adt::OpImpl> = impls
            .iter()
            .filter(|i| self.could_be(&i.rhs, &it))
            .collect();
        let [one] = fits.as_slice() else {
            let msg = format!(
                "the type `{}` cannot be indexed by `{}` (add `impl Index<{}> for {adt}` from `std::ops`)",
                self.describe(bt),
                self.describe(&it),
                self.describe(&it)
            );
            self.error(ix.syntax(), msg);
            return Ty::Unknown;
        };
        self.unify(&one.rhs, &it);
        let key = one.key.clone();
        // (`graph[n]` with `impl<N, E> Index<NodeIndex> for Graph<N, E>`: the element type in the
        // base's type arguments.)
        let ret = match self.adts.method(adt, &key).cloned() {
            Some(sig) => match self.shallow(bt) {
                Ty::Adt(_, targs) if sig.impl_args.iter().any(|a| matches!(a, Ty::Param(_))) => {
                    let sig = self.method_inst(&sig, Some(&targs));
                    self.last_type_args.clear();
                    sig.ret
                }
                _ => sig.ret,
            },
            None => Ty::Unknown,
        };
        // (Also `v["a"]["b"] = x`: Rust takes every index of an assigned place with `IndexMut`.)
        let mut place = ix.syntax().clone();
        while let Some(outer) = place.parent().and_then(ast::IndexExpr::cast)
            && outer.base().is_some_and(|b| b.syntax() == &place)
        {
            place = outer.syntax().clone();
        }
        let assigned = place
            .parent()
            .and_then(ast::BinExpr::cast)
            .is_some_and(|b| {
                matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                    && b.lhs().is_some_and(|l| l.syntax() == &place)
            });
        // `names[i].push('!')`: a string element is a JS value, so a method on it needs the place.
        let elem_is_str = self.shallow(&ret) == Ty::Str;
        let receiver = elem_is_str
            && ix
                .syntax()
                .parent()
                .and_then(ast::MethodCallExpr::cast)
                .is_some_and(|m| m.receiver().is_some_and(|r| r.syntax() == ix.syntax()));
        let has_index_mut = self
            .adts
            .op_impls
            .get(adt)
            .is_some_and(|v| v.iter().any(|i| i.trait_name == "IndexMut"));
        let assigned = assigned || (receiver && has_index_mut);
        if assigned {
            // `grid[i] = v`: `impl IndexMut<I>` for the same index type hands out the place.
            // (The `IndexMut` beside the `Index` picked: its index type, as instantiated.)
            let index_rhs = one.rhs.clone();
            let index_mut = self.adts.op_impls.get(adt).and_then(|v| {
                v.iter()
                    .find(|i| {
                        i.trait_name == "IndexMut"
                            && (self.could_be(&i.rhs, &it)
                                || self.describe(&i.rhs) == self.describe(&index_rhs)
                                || matches!(&i.rhs, Ty::Param(_))
                                || i.rhs
                                    .inner_types()
                                    .iter()
                                    .any(|t| matches!(t, Ty::Param(_))))
                    })
                    .cloned()
            });
            match index_mut {
                Some(_) if self.opaque_index_mut.contains(adt) => {
                    let msg = format!(
                        "assigning through `{adt}`'s `index_mut` needs it to end in `&mut <place>` (like `&mut self.cells[i]`), so the write reaches the place"
                    );
                    self.error(ix.syntax(), msg);
                }
                Some(im) => {
                    let r = self.range(ix.syntax());
                    self.index_places.insert(r);
                    self.record_conv(
                        ix.syntax(),
                        Conv::Method {
                            key: im.key.clone(),
                        },
                    );
                    return ret;
                }
                None => {
                    let msg = format!(
                        "cannot assign to data in an index of `{}` (add `impl IndexMut<{}> for {adt}` from `std::ops`)",
                        self.describe(bt),
                        self.describe(&it)
                    );
                    self.error(ix.syntax(), msg);
                }
            }
        }
        self.record_conv(ix.syntax(), Conv::Method { key: key.clone() });
        ret
    }

    /// `-a` / `!a` where `a` is a struct or enum: its `impl Neg` / `impl Not`.
    pub(super) fn unary_overload(&mut self, node: &SyntaxNode, neg: bool, adt: &str, t: &Ty) -> Ty {
        let tr = if neg { "Neg" } else { "Not" };
        // (`impl Neg for Wrapping<i32>` next to `impl Neg for Wrapping<i8>`: the operand's own.)
        let targs = match self.shallow(t) {
            Ty::Adt(_, a) => a,
            _ => Vec::new(),
        };
        let impls: Vec<crate::adt::OpImpl> = self
            .adts
            .op_impls
            .get(adt)
            .map(|v| v.iter().filter(|i| i.trait_name == tr).cloned().collect())
            .unwrap_or_default();
        let found = impls
            .iter()
            .find(|i| {
                self.adts.method(adt, &i.key).is_some_and(|sig| {
                    sig.impl_args.len() == targs.len()
                        && sig
                            .impl_args
                            .iter()
                            .zip(&targs)
                            .all(|(a, t)| matches!(a, Ty::Param(_)) || self.could_be(a, t))
                })
            })
            .or_else(|| impls.first())
            .map(|i| i.key.clone());
        match found {
            Some(key) => {
                self.record_conv(node, Conv::Method { key: key.clone() });
                let Some(sig) = self.adts.method(adt, &key).cloned() else {
                    return Ty::Unknown;
                };
                // (`impl<T: Neg> Neg for Wrap<T>`: its result in the operand's type arguments.)
                match self.shallow(t) {
                    Ty::Adt(_, targs)
                        if sig.impl_args.iter().any(|a| matches!(a, Ty::Param(_))) =>
                    {
                        let sig = self.method_inst(&sig, Some(&targs));
                        self.last_type_args.clear();
                        sig.ret
                    }
                    _ => sig.ret,
                }
            }
            None => {
                let msg = format!(
                    "cannot apply unary operator `{}` to type `{}` (add `impl {tr} for {adt}` from `std::ops`)",
                    if neg { "-" } else { "!" },
                    self.describe(t)
                );
                self.error(node, msg);
                Ty::Unknown
            }
        }
    }

    /// `?` on a `Result<_, E1>` in a function returning `Result<_, E2>`: when `E2: From<E1>`,
    /// the error is converted. Returns whether a conversion was recorded.
    /// Whether a `Box<dyn Error>` holds `e` as it is: a std error, your `impl Error` type, or
    /// another `dyn Error`.
    pub(super) fn boxes_as_dyn_error(&mut self, e: &Ty) -> bool {
        match self.shallow(e) {
            Ty::Extern(n) => super::res::is_std_error(&n),
            Ty::Adt(n, _) => self
                .adts
                .defs
                .get(&n)
                .is_some_and(|d| d.implements("Error")),
            Ty::Dyn(tr) => tr == "Error",
            _ => false,
        }
    }

    pub(super) fn try_convert(&mut self, t: &ast::TryExpr, e: &Ty, re: &Ty) -> bool {
        // Into `anyhow::Error`: any error, wrapped (`{:?}` shows it as the cause chain).
        if self.shallow(re) == super::crates::anyhow_error() {
            let ok = match self.shallow(e) {
                Ty::Extern(n) => n == "anyhow::Error" || super::res::is_std_error(&n),
                Ty::Adt(n, _) => self
                    .adts
                    .defs
                    .get(&n)
                    .is_some_and(|d| d.implements("Error")),
                Ty::Dyn(tr) => tr == "Error",
                _ => false,
            };
            if ok && self.shallow(e) != super::crates::anyhow_error() {
                self.record_conv(
                    t.syntax(),
                    Conv::Wrap {
                        js: "κσ.$anyhow.from".into(),
                    },
                );
            }
            if ok {
                return true;
            }
        }
        // Into `Box<dyn Error>`: std errors and your `impl Error` types, unchanged.
        if self.shallow(re) == Ty::Dyn("Error".into()) && self.boxes_as_dyn_error(e) {
            return true;
        }
        // `Err("usage")?` / `Err(format!(..))?` into `Box<dyn Error>`: the string itself, as
        // `"x".into()`.
        if self.shallow(re) == Ty::Dyn("Error".into())
            && self.shallow(e) == Ty::Str
            && !self.adts.traits.contains_key("Error")
        {
            return true;
        }
        // (`impl From<MyErr> for String`: a std type's `From` impl.)
        let adt = match self.shallow(re) {
            Ty::Adt(adt, _) => adt,
            t => match crate::adt::ext_key_of(&t) {
                Some(k) => k.to_string(),
                None => return false,
            },
        };
        if self.could_be(e, re) || !self.adts.from_impls.contains_key(&adt) {
            return false;
        }
        match self.pick_from(&adt, e) {
            Some(key) => {
                self.record_conv(t.syntax(), Conv::From { adt, key });
                true
            }
            None => false,
        }
    }
}
