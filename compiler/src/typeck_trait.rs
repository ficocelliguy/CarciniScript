//! User-defined traits: declarations (with default methods and supertraits), `impl Trait for
//! Type`, trait-bounded type parameters and `dyn Trait`. Types are erased in JS, so a trait method
//! call is an ordinary JS method call and dispatch is dynamic. A child module of `typeck`.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericParams, HasName, HasTypeBounds},
};

use super::{Checker, Local, Mutation};
use crate::adt::{Method, Receiver, TraitDef, TraitMethod};
use crate::ty::Ty;

/// The traits a trait declaration names as supertraits (`trait Shape: Debug + Named`).
fn supertraits(t: &ast::Trait) -> Vec<String> {
    let Some(list) = t.type_bound_list() else {
        return vec![];
    };
    list.bounds()
        .filter_map(|b| match b.ty()? {
            ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
            _ => None,
        })
        .collect()
}

impl Checker<'_> {
    /// Declare every trait and lower its method signatures (`Self` is a type parameter bounded
    /// by the trait). Runs before `impl` blocks are read.
    pub(super) fn collect_traits(&mut self, file: &ast::SourceFile) {
        for item in crate::adt::all_items(file) {
            let ast::Item::Trait(t) = &item else { continue };
            let Some(name) = t.name() else { continue };
            let n = name.text().to_string();
            if self.adts.traits.contains_key(&n) || self.adts.defs.contains_key(&n) {
                self.error(
                    name.syntax(),
                    format!("the name `{n}` is defined multiple times"),
                );
                continue;
            }
            // `trait Convert<T>`: each impl names `T` (`impl Convert<String> for A`).
            let type_params: Vec<String> = t
                .generic_param_list()
                .into_iter()
                .flat_map(|l| l.generic_params())
                .filter_map(|p| match p {
                    ast::GenericParam::TypeParam(tp) => Some(tp.name()?.text().to_string()),
                    _ => None,
                })
                .collect();
            // `trait Buf<const N: usize>`: each impl names `N` (`impl Buf<4> for X`); the default
            // methods read it (each impl gets its own copy of them, see `Trait$defaults`).
            let const_params: Vec<String> = t
                .generic_param_list()
                .into_iter()
                .flat_map(|l| l.generic_params())
                .filter_map(|p| match p {
                    ast::GenericParam::ConstParam(cp) => Some(cp.name()?.text().to_string()),
                    _ => None,
                })
                .collect();
            let assoc_types: Vec<String> = t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
                .filter_map(|i| match i {
                    ast::AssocItem::TypeAlias(ta) => Some(ta.name()?.text().to_string()),
                    _ => None,
                })
                .collect();
            // `type Unit: Debug + Copy;`
            let assoc_bounds: HashMap<String, Vec<String>> = t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
                .filter_map(|i| match i {
                    ast::AssocItem::TypeAlias(ta) => {
                        let names = ta
                            .type_bound_list()
                            .into_iter()
                            .flat_map(|l| l.bounds())
                            .filter_map(|b| match b.ty()? {
                                ast::Type::PathType(p) => {
                                    Some(p.path()?.segment()?.name_ref()?.text().to_string())
                                }
                                _ => None,
                            })
                            .collect();
                        Some((ta.name()?.text().to_string(), names))
                    }
                    _ => None,
                })
                .collect();
            self.adts.traits.insert(
                n.clone(),
                TraitDef {
                    supers: supertraits(t),
                    methods: Vec::new(),
                    consts: Vec::new(),
                    const_tys: HashMap::new(),
                    const_defaults: Vec::new(),
                    module: None,
                    assoc_types,
                    type_params,
                    const_params,
                    assoc_bounds,
                    assoc_iters: HashMap::new(),
                },
            );
        }
        for item in crate::adt::all_items(file) {
            let ast::Item::Trait(t) = &item else { continue };
            let Some(name) = t.name() else { continue };
            let n = name.text().to_string();
            let saved = self.self_ty.replace(Ty::Param("Self".into()));
            // In the trait's signatures, its type parameters and `Self::Unit` are whatever each
            // impl says: `Ty::Param("Shape::Unit")`, which matches any type (an impl's own
            // signatures are what calls on a concrete type use) and is replaced in the default
            // methods an impl inherits.
            let def = self.adts.traits.get(&n).cloned();
            // (`trait IterExt: Iterator`: `Self::Item` is the iterator's, unchecked here.)
            let iter_ext = def.as_ref().is_some_and(|d| {
                d.supers.iter().any(|s| {
                    matches!(
                        s.as_str(),
                        "Iterator" | "DoubleEndedIterator" | "ExactSizeIterator"
                    )
                })
            });
            let (params, assoc) = def
                .map(|d| (d.type_params, d.assoc_types))
                .unwrap_or_default();
            let mut assoc_tys: HashMap<String, Ty> = assoc
                .iter()
                .map(|a| (a.clone(), Ty::Param(format!("{n}::{a}"))))
                .collect();
            if iter_ext {
                assoc_tys.entry("Item".into()).or_insert(Ty::Unknown);
            }
            let saved_assoc = std::mem::replace(&mut self.impl_assoc, assoc_tys);
            self.generics.clear();
            self.enter_trait_const_params(t);
            for p in &params {
                self.generics
                    .insert(p.clone(), Ty::Param(format!("{n}::{p}")));
            }
            let mut methods = Vec::new();
            let mut consts = Vec::new();
            let mut const_tys = HashMap::new();
            let mut const_defaults = Vec::new();
            // `type Iter: Iterator<Item = u32>;`
            let mut assoc_iters = HashMap::new();
            for ta in t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
                .filter_map(|i| match i {
                    ast::AssocItem::TypeAlias(ta) => Some(ta),
                    _ => None,
                })
            {
                if let (Some(an), Some(Some(item))) = (
                    ta.name(),
                    ta.type_bound_list()
                        .as_ref()
                        .and_then(super::bounds_iterator_item),
                ) {
                    let elem = self.lower_type(&item);
                    assoc_iters.insert(an.text().to_string(), elem);
                }
            }
            for it in t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                let ast::AssocItem::Fn(f) = it else {
                    // `const X: T;`: each impl gives the value; `const X: T = ..;` is a
                    // default that impls may override.
                    if let ast::AssocItem::Const(k) = &it {
                        if let Some(n) = k.name() {
                            consts.push(n.text().to_string());
                            let t = k.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                            const_tys.insert(n.text().to_string(), t);
                            if k.body().is_some() {
                                const_defaults.push(n.text().to_string());
                            }
                        }
                        continue;
                    }
                    // `type Unit;` (recorded above); a default (`type Unit = f64;`) is unstable.
                    if let ast::AssocItem::TypeAlias(ta) = &it {
                        if ta.ty().is_some() {
                            self.error(
                                ta.syntax(),
                                "associated type defaults are unstable (each impl says `type X = ..;`)",
                            );
                        }
                        continue;
                    }
                    self.error(
                        it.syntax(),
                        "only methods, associated types and associated consts are supported in traits so far",
                    );
                    continue;
                };
                let Some(fname) = f.name() else { continue };
                let sig = self.lower_method(&f);
                let arg_bounds = self.arg_bounds(&f);
                // (A default one without `self` is copied onto each implementing class.)
                methods.push(TraitMethod {
                    name: fname.text().to_string(),
                    sig,
                    has_default: f.body().is_some(),
                    own_bounds: super::generic::plain_bounds(&f),
                    arg_bounds,
                });
            }
            self.self_ty = saved;
            self.impl_assoc = saved_assoc;
            self.generics.clear();
            self.const_tys.clear();
            if let Some(def) = self.adts.traits.get_mut(&n) {
                def.methods = methods;
                def.consts = consts;
                def.const_tys = const_tys;
                def.const_defaults = const_defaults;
                def.assoc_iters = assoc_iters;
            }
        }
    }

    /// A trait's const generic parameters (`trait Buf<const N: usize>`), as values in its
    /// signatures and default bodies.
    fn enter_trait_const_params(&mut self, t: &ast::Trait) {
        for p in t
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
        {
            if let ast::GenericParam::ConstParam(cp) = p
                && let Some(n) = cp.name()
            {
                let n = n.text().to_string();
                let ty = self.const_param_ty(&cp, false);
                self.generics.insert(n.clone(), Ty::Param(n.clone()));
                self.const_tys.insert(n, ty);
            }
        }
    }

    /// The type an impl's `Self` names: a struct or enum, or a built-in type's `ext_key`.
    pub(super) fn self_ty_named(&self, type_name: &str) -> Ty {
        // (A blanket impl's `Self` is its `T`, which each call replaces.)
        if let Some(p) = type_name
            .strip_suffix("$all")
            .and_then(|tr| self.adts.blanket_params.get(tr))
        {
            return Ty::Param(p.clone());
        }
        match type_name {
            "String" => Ty::Str,
            "Vec" => Ty::Vec(Box::new(Ty::Unknown)),
            "VecDeque" => Ty::Coll(
                crate::ty::Coll::VecDeque,
                Box::new(Ty::Vec(Box::new(Ty::Unknown))),
            ),
            k if let Some(r) = crate::ty::RangeKind::from_name(k) => {
                Ty::Range(Box::new(Ty::Unknown), r)
            }
            "bool" => Ty::Bool,
            "char" => Ty::Char,
            "f64" => Ty::Float { f32: false },
            "f32" => Ty::Float { f32: true },
            n if self.adts.ext_types.contains(n) => Ty::from_name(n),
            n => Ty::Adt(n.to_string(), vec![]),
        }
    }

    /// `impl Trait for Type`: every required method must be there and nothing else may be;
    /// default methods the impl doesn't override become the type's methods too.
    pub(super) fn register_trait_impl(&mut self, imp: &ast::Impl, tr: &str, type_name: &str) {
        let Some(def) = self.adts.traits.get(tr).cloned() else {
            let node = imp
                .trait_()
                .map(|t| t.syntax().clone())
                .unwrap_or_else(|| imp.syntax().clone());
            self.error(
                &node,
                format!("cannot find trait `{tr}` in this scope (std traits other than `Display`, `Default`, `From`, the comparisons, `Clone` and the operators are derived, not implemented)"),
            );
            return;
        };
        let given: Vec<(String, ast::Fn)> = imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
            .filter_map(|i| match i {
                ast::AssocItem::Fn(f) => Some((f.name()?.text().to_string(), f)),
                _ => None,
            })
            .collect();
        for (n, f) in &given {
            if !def.methods.iter().any(|m| &m.name == n) {
                let node = f
                    .name()
                    .map(|x| x.syntax().clone())
                    .unwrap_or_else(|| f.syntax().clone());
                self.error(
                    &node,
                    format!("method `{n}` is not a member of trait `{tr}`"),
                );
            }
        }
        let given_consts: Vec<String> = imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
            .filter_map(|i| match i {
                ast::AssocItem::Const(k) => Some(k.name()?.text().to_string()),
                _ => None,
            })
            .collect();
        let given_types: Vec<String> = imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
            .filter_map(|i| match i {
                ast::AssocItem::TypeAlias(ta) => Some(ta.name()?.text().to_string()),
                _ => None,
            })
            .collect();
        for (n, node) in imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
            .filter_map(|i| match i {
                ast::AssocItem::TypeAlias(ta) => {
                    Some((ta.name()?.text().to_string(), ta.syntax().clone()))
                }
                _ => None,
            })
        {
            if !def.assoc_types.contains(&n) {
                self.error(&node, format!("type `{n}` is not a member of trait `{tr}`"));
            }
        }
        let missing: Vec<String> = def
            .methods
            .iter()
            .filter(|m| !m.has_default && !given.iter().any(|(n, _)| *n == m.name))
            .map(|m| format!("`{}`", m.name))
            .chain(
                def.assoc_types
                    .iter()
                    .filter(|k| !given_types.contains(k))
                    .map(|k| format!("`{k}`")),
            )
            .chain(
                def.consts
                    .iter()
                    .filter(|k| !given_consts.contains(k) && !def.const_defaults.contains(k))
                    .map(|k| format!("`{k}`")),
            )
            .collect();
        if !missing.is_empty() {
            let node = imp
                .self_ty()
                .map(|t| t.syntax().clone())
                .unwrap_or_else(|| imp.syntax().clone());
            self.error(
                &node,
                format!(
                    "not all trait items implemented, missing: {}",
                    missing.join(", ")
                ),
            );
        }
        // Defaults the impl leaves out are the type's constants too (`S::N`), read off the
        // class, where the trait's `$statics` put them.
        for k in &def.const_defaults {
            if !given_consts.contains(k) {
                let ty = def.const_tys.get(k).cloned().unwrap_or(Ty::Unknown);
                self.adts
                    .assoc_consts
                    .entry(type_name.to_string())
                    .or_default()
                    .entry(k.clone())
                    .or_insert(ty);
            }
        }
        let mut m: HashMap<String, Ty> =
            [("Self".to_string(), self.self_ty_named(type_name))].into();
        // The impl's type parameters, which its trait arguments and associated types may name
        // (`impl<T> Store<T> for Bag<T> { type Item = T; }`). (`enter_impl` sets them up again,
        // with their bounds, for the methods.)
        self.generics.clear();
        for p in imp
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
        {
            let n = match &p {
                ast::GenericParam::TypeParam(tp) => tp.name(),
                ast::GenericParam::ConstParam(cp) => cp.name(),
                ast::GenericParam::LifetimeParam(_) => None,
            };
            if let Some(n) = n {
                let n = n.text().to_string();
                self.generics.insert(n.clone(), Ty::Param(n.clone()));
                // (And their bounds' names, for `type P = T::P;` with `T: Fac`.)
                let wheres = imp
                    .where_clause()
                    .into_iter()
                    .flat_map(|w| w.predicates())
                    .filter(|pred| {
                        matches!(pred.ty(), Some(ast::Type::PathType(pt))
                            if pt.path().and_then(|p| p.as_single_name_ref()).is_some_and(|x| x.text() == n.as_str()))
                    })
                    .filter_map(|pred| pred.type_bound_list());
                let inline = match &p {
                    ast::GenericParam::TypeParam(tp) => tp.type_bound_list(),
                    _ => None,
                };
                let bounds: Vec<String> = inline
                    .into_iter()
                    .chain(wheres)
                    .flat_map(|l| l.bounds())
                    .filter_map(|b| match b.ty()? {
                        ast::Type::PathType(pt) => {
                            Some(pt.path()?.segment()?.name_ref()?.text().to_string())
                        }
                        _ => None,
                    })
                    .collect();
                self.param_bounds.insert(n, bounds);
            }
        }
        // The impl's associated types (`type Unit = f64;`) and trait arguments
        // (`impl Convert<String>`) for the defaults it inherits.
        for i in imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
        {
            if let ast::AssocItem::TypeAlias(ta) = i
                && let (Some(name), Some(t)) = (ta.name(), ta.ty())
            {
                let t = self.lower_type(&t);
                m.insert(format!("{tr}::{}", name.text()), t.clone());
                self.adts
                    .impl_assoc
                    .entry((type_name.to_string(), tr.to_string()))
                    .or_default()
                    .insert(name.text().to_string(), t);
            }
        }
        let mut head_args = Vec::new();
        if let Some(ast::Type::PathType(pt)) = imp.trait_()
            && let Some(args) = pt
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| ra_ap_syntax::ast::HasGenericArgs::generic_arg_list(&s))
        {
            let given: Vec<ast::Type> = args
                .generic_args()
                .filter_map(|g| match g {
                    ast::GenericArg::TypeArg(ta) => ta.ty(),
                    _ => None,
                })
                .collect();
            let mut trait_args = Vec::new();
            for (p, t) in def.type_params.iter().zip(given) {
                let t = self.lower_type(&t);
                m.insert(format!("{tr}::{p}"), t.clone());
                trait_args.push(t);
            }
            head_args = trait_args;
        }
        // (`enter_impl` lowers the self type again, and reports what's wrong with it.)
        let n_diags = self.diags.len();
        let self_args = match imp.self_ty().map(|t| self.lower_type(&t)) {
            Some(Ty::Adt(_, args)) => args,
            Some(t) => crate::adt::ext_args(&t),
            None => Vec::new(),
        };
        self.diags.truncate(n_diags);
        self.adts
            .impl_heads
            .entry((type_name.to_string(), tr.to_string()))
            .or_default()
            .push((self_args, head_args));
        for tm in def
            .methods
            .iter()
            .filter(|tm| tm.has_default && !given.iter().any(|(n, _)| *n == tm.name))
        {
            let sig = subst_method(&tm.sig, &m);
            self.adts
                .methods
                .entry(type_name.to_string())
                .or_default()
                .insert(tm.name.clone(), sig);
        }
        let impls = self
            .adts
            .trait_impls
            .entry(type_name.to_string())
            .or_default();
        if impls.iter().any(|t| t == tr) && def.type_params.is_empty() {
            let node = imp
                .trait_()
                .map(|t| t.syntax().clone())
                .unwrap_or_else(|| imp.syntax().clone());
            // (`impl Tr for String` next to `impl Tr for str` whose methods don't hand on to it:
            // see `adt::all_items`.)
            let owned = match imp.self_ty() {
                Some(ast::Type::PathType(p)) => p
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.name_ref())
                    .is_some_and(|n| n.text() == type_name),
                _ => false,
            };
            let msg = if owned && matches!(type_name, "String" | "Vec") {
                let (owned, borrowed, call) = if type_name == "String" {
                    ("String", "str", "self.as_str()")
                } else {
                    ("Vec<T>", "[T]", "self.as_slice()")
                };
                format!(
                    "`{tr}` is implemented for both `{borrowed}` and `{owned}`, which are one type in JS: the `{owned}` impl's methods must hand each call on to the `{borrowed}` impl's (`{call}.method(args)`)"
                )
            } else if matches!(type_name, "String" | "Vec") {
                let same = if type_name == "String" {
                    "`str`, `&str`, `String` and `Box<str>`"
                } else {
                    "`[T]`, `&[T]`, `[T; N]`, `Vec<T>` and `Box<[T]>`"
                };
                format!(
                    "conflicting implementations of trait `{tr}` for type `{type_name}` ({same} are one type in JS)"
                )
            } else if let Some(n) = crate::adt::tuple_arity(type_name) {
                format!(
                    "`{tr}` for two tuples of {n} isn't supported yet (tuples are JS arrays, which a call through a type parameter tells apart by length only)"
                )
            } else if def.type_params.is_empty() {
                format!("conflicting implementations of trait `{tr}` for type `{type_name}`")
            } else {
                format!(
                    "implementing the generic trait `{tr}` more than once for `{type_name}` isn't supported yet (one impl per type)"
                )
            };
            self.error(&node, msg);
        } else {
            impls.push(tr.to_string());
        }
    }

    /// Check default method bodies, with `self: Self` where `Self: Trait`.
    pub(super) fn check_trait_defaults(&mut self, file: &ast::SourceFile) {
        for item in crate::adt::all_items(file) {
            let ast::Item::Trait(t) = &item else { continue };
            let Some(name) = t.name() else { continue };
            let tr = name.text().to_string();
            let Some(def) = self.adts.traits.get(&tr).cloned() else {
                continue;
            };
            for it in t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                // A default const's value, with `Self: Trait`.
                if let ast::AssocItem::Const(k) = &it
                    && let (Some(n), Some(body)) = (k.name(), k.body())
                {
                    let ty = def.const_tys.get(n.text()).cloned().unwrap_or(Ty::Unknown);
                    let saved = self.self_ty.replace(Ty::Param("Self".into()));
                    self.param_bounds.insert("Self".into(), vec![tr.clone()]);
                    self.scopes = vec![HashMap::new()];
                    self.ret_ty = Ty::Unit;
                    self.in_async = false;
                    self.check(&body, &ty);
                    self.param_bounds.clear();
                    self.self_ty = saved;
                    continue;
                }
                let ast::AssocItem::Fn(f) = it else { continue };
                if f.body().is_none() {
                    continue;
                }
                let Some(fname) = f.name() else { continue };
                let Some(tm) = def.methods.iter().find(|m| m.name == fname.text()) else {
                    continue;
                };
                // The method's own type parameters and bounds (`fn pick<B: Ord>`).
                self.enter_generics(&f, false);
                let saved = self.self_ty.replace(Ty::Param("Self".into()));
                self.param_bounds.insert("Self".into(), vec![tr.clone()]);
                // (`trait Stats: Iterator`: `self` is an iterator, of items unchecked here.)
                let self_ty = if def.supers.iter().any(|s| {
                    matches!(
                        s.as_str(),
                        "Iterator" | "DoubleEndedIterator" | "ExactSizeIterator"
                    )
                }) {
                    Ty::Iter {
                        elem: Box::new(Ty::Unknown),
                        mutable: false,
                    }
                } else {
                    Ty::Param("Self".into())
                };
                let self_local = f
                    .param_list()
                    .and_then(|pl| pl.self_param())
                    .and_then(|sp| {
                        Some((
                            sp.name()?,
                            Local {
                                ty: self_ty.clone(),
                                mutable: sp.amp_token().is_none() && sp.mut_token().is_some(),
                                deferred: false,
                                reference: sp.amp_token().map(|_| sp.mut_token().is_some()),
                                owned: false,
                                js: false,
                                decl: None,
                            },
                        ))
                    });
                // (`trait Buf<const N: usize>`: `N` is a value in the body.)
                self.enter_trait_const_params(t);
                self.check_body(
                    &f,
                    tm.sig.params.clone(),
                    tm.sig.ret.clone(),
                    tm.sig.is_async,
                    self_local,
                );
                self.leave_generics();
                self.const_tys.clear();
                self.param_bounds.clear();
                self.self_ty = saved;
            }
        }
    }

    /// Method `m` of trait `tr` or one of its supertraits.
    pub(super) fn trait_method(&self, tr: &str, m: &str) -> Option<Method> {
        self.trait_method_of(tr, m).map(|(_, s)| s)
    }

    /// `x.get()` through a type parameter or `dyn` bound by `tr`, when some type has another
    /// trait's (or an inherent) `get` too: called as `Trait$get`, which every impl of the
    /// trait has (see `Adts::split_methods`).
    pub(super) fn note_split_method(&mut self, m: &ast::MethodCallExpr, tr: &str, method: &str) {
        if let Some((owner, _)) = self.trait_method_of(tr, method)
            && self
                .adts
                .split_methods()
                .contains(&(owner.clone(), method.to_string()))
        {
            let r = self.range(m.syntax());
            self.method_keys.insert(r, format!("{owner}${method}"));
        }
    }

    /// `trait_method`, with the trait that declares it.
    pub(super) fn trait_method_of(&self, tr: &str, m: &str) -> Option<(String, Method)> {
        let mut seen = Vec::new();
        let mut todo = vec![tr.to_string()];
        while let Some(t) = todo.pop() {
            if seen.contains(&t) {
                continue;
            }
            seen.push(t.clone());
            let Some(def) = self.adts.traits.get(&t) else {
                continue;
            };
            if let Some(tm) = def.methods.iter().find(|x| x.name == m) {
                return Some((t.clone(), tm.sig.clone()));
            }
            todo.extend(def.supers.iter().cloned());
        }
        None
    }

    /// Whether trait `tr` is `want` or has it as a (transitive) supertrait.
    pub(super) fn trait_includes(&self, tr: &str, want: &str) -> bool {
        let mut seen = Vec::new();
        // (`Ord+Debug`: a `dyn` of several std traits.)
        let mut todo: Vec<String> = tr.split('+').map(String::from).collect();
        while let Some(t) = todo.pop() {
            if t == want
                || super::generic::std_implies(&t, want) && !self.adts.traits.contains_key(&t)
            {
                return true;
            }
            if seen.contains(&t) {
                continue;
            }
            seen.push(t.clone());
            if let Some(def) = self.adts.traits.get(&t) {
                todo.extend(def.supers.iter().cloned());
            }
        }
        false
    }

    /// Whether a type implements a user trait.
    /// The blanket impl (key, method) a `recv.method()` call reaches: the receiver has no method
    /// of that name itself and meets the impl's bounds.
    pub(super) fn blanket_method(&self, rt: &Ty, method: &str) -> Option<(String, Method)> {
        let rt = self.zonk(rt);
        if matches!(rt, Ty::Unknown | Ty::Var(_) | Ty::Param(_) | Ty::Dyn(_)) {
            return None;
        }
        if let Ty::Adt(n, _) = &rt
            && self.adts.method(n, method).is_some()
        {
            return None;
        }
        if crate::service::std_methods(&rt).contains(&method) {
            return None;
        }
        self.adts.blankets.iter().find_map(|(tr, bounds)| {
            let key = format!("{tr}$all");
            let sig = self.adts.method(&key, method)?.clone();
            sig.receiver?;
            bounds
                .iter()
                .all(|b| self.meets_bound(&rt, b))
                .then_some((key, sig))
        })
    }

    /// A blanket impl's method or associated function for `ty`: the impl's `T` (and `Self`) is
    /// `ty`.
    pub(super) fn blanket_inst(&mut self, key: &str, sig: &Method, ty: &Ty) -> Method {
        let mut sig = self.method_inst(sig, Some(std::slice::from_ref(ty)));
        let mut map: HashMap<String, Ty> = [("Self".to_string(), ty.clone())].into();
        // (`impl<I: Iterator> Stats for I`: `I::Item` is the receiver's item.)
        if let Some(p) = key
            .strip_suffix("$all")
            .and_then(|tr| self.adts.blanket_params.get(tr))
            .cloned()
        {
            map.insert(p.clone(), ty.clone());
            // (`impl<U, T> DoubleDrop<U> for T`: `U` is whatever each call passes.)
            let tr = key.strip_suffix("$all").unwrap_or_default();
            for o in self
                .adts
                .blanket_others
                .get(tr)
                .cloned()
                .unwrap_or_default()
            {
                let v = self.fresh(super::VarKind::General);
                map.insert(o, v);
            }
            let item = match self.zonk(ty) {
                Ty::Iter { elem, .. } | Ty::Range(elem, _) => Some(*elem),
                Ty::Adt(n, _) => self.adts.iter_items.get(&n).cloned(),
                _ => None,
            };
            if let Some(item) = item {
                map.insert(format!("{p}::Item"), item);
            }
        }
        sig.params = sig
            .params
            .iter()
            .map(|t| super::generic::subst(t, &map))
            .collect();
        sig.ret = super::generic::subst(&sig.ret, &map);
        sig
    }

    /// The blanket impl (key, function) whose associated function `ty::f()` reaches
    /// (`i32::make()` with `impl<T: Default> Make for T`): `ty` has no function of that name and
    /// meets the impl's bounds.
    pub(super) fn blanket_static(&self, ty: &Ty, f: &str) -> Option<(String, Method)> {
        let ty = self.zonk(ty);
        if matches!(ty, Ty::Unknown | Ty::Var(_) | Ty::Param(_) | Ty::Dyn(_)) {
            return None;
        }
        let own = match &ty {
            Ty::Adt(n, _) => self.adts.method(n, f).is_some(),
            t => crate::adt::ext_key_of(t).is_some_and(|k| self.adts.method(k, f).is_some()),
        };
        if own {
            return None;
        }
        // (A built-in type's own associated functions come first: `String::new()`.)
        if !matches!(ty, Ty::Adt(..))
            && matches!(
                f,
                "new"
                    | "from"
                    | "default"
                    | "with_capacity"
                    | "from_iter"
                    | "from_str"
                    | "try_from"
                    | "from_utf8"
                    | "from_str_radix"
                    | "from_le_bytes"
                    | "from_be_bytes"
                    | "from_ne_bytes"
                    | "from_u32"
                    | "from_digit"
                    | "with_hasher"
                    | "with_capacity_and_hasher"
                    | "new_in"
            )
        {
            return None;
        }
        let mut found: Vec<(&String, &Vec<String>)> = self.adts.blankets.iter().collect();
        found.sort();
        found.into_iter().find_map(|(tr, bounds)| {
            let key = format!("{tr}$all");
            let sig = self.adts.method(&key, f)?.clone();
            if sig.receiver.is_some() {
                return None;
            }
            bounds
                .iter()
                .all(|b| self.meets_bound(&ty, b))
                .then_some((key, sig))
        })
    }

    /// The type `T` names in a `T::f()` call that may reach a blanket impl's associated function:
    /// a built-in type or one of the file's structs and enums (with `Vec::<u8>`'s arguments).
    pub(super) fn blanket_qualifier(&mut self, path: &ast::Path, t: &str) -> Option<Ty> {
        use ra_ap_syntax::ast::HasGenericArgs;
        if self.adts.blankets.is_empty() || self.param_bounds.contains_key(t) {
            return None;
        }
        let args: Vec<Ty> = path
            .qualifier()
            .and_then(|q| q.segment())
            .and_then(|s| s.generic_arg_list())
            .map(|l| {
                l.generic_args()
                    .filter_map(|g| match g {
                        ast::GenericArg::TypeArg(ta) => ta.ty(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .iter()
            .map(|a| self.lower_type(a))
            .collect();
        let arg = |i: usize| Box::new(args.get(i).cloned().unwrap_or(Ty::Unknown));
        Some(match t {
            "String" | "str" => Ty::Str,
            "Vec" => Ty::Vec(arg(0)),
            "Option" => Ty::Option(arg(0)),
            "HashMap" => Ty::Map(arg(0), arg(1)),
            "HashSet" => Ty::Set(arg(0)),
            "bool" => Ty::Bool,
            "char" => Ty::Char,
            n if crate::adt::ext_key(n).is_some() && Ty::from_name(n).is_numeric() => {
                Ty::from_name(n)
            }
            n if self.adts.defs.get(n).is_some_and(|d| d.module.is_none()) => {
                let n_params = self.adts.defs[n].type_params.len();
                let mut targs = args.clone();
                while targs.len() < n_params {
                    targs.push(self.fresh(super::VarKind::General));
                }
                Ty::Adt(n.to_string(), targs)
            }
            _ => return None,
        })
    }

    /// `ty::f(..)` through a blanket impl's associated function (`i32::make()`).
    pub(super) fn blanket_static_call(
        &mut self,
        c: &ast::CallExpr,
        ty: &Ty,
        f: &str,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        let (key, sig) = self.blanket_static(ty, f)?;
        let sig = self.blanket_inst(&key, &sig, ty);
        self.arg_count(c.syntax(), "function", sig.params.len(), args.len());
        for (a, p) in args.iter().zip(&sig.params) {
            self.check(a, p);
        }
        self.record_conv(
            c.syntax(),
            crate::adt::Conv::ExtDict {
                key,
                dict: ty.clone(),
            },
        );
        Some(sig.ret)
    }

    /// The traits blanket impls give a type parameter through its bounds
    /// (`impl<T: Bounded> Lower for T` makes `Lower` a trait of a `T: Bounded`).
    pub(super) fn param_blanket_traits(&self, param: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .adts
            .blankets
            .iter()
            .filter(|(tr, bounds)| {
                !bounds.contains(tr) && bounds.iter().all(|b| self.param_has(param, b))
            })
            .map(|(tr, _)| tr.clone())
            .collect();
        out.sort();
        out
    }

    /// The blanket impl whose associated function is being checked, when `param` is its `T`
    /// (`Self::make()` / `T::make()` in `impl<T: Default> Make for T`): the trait.
    pub(super) fn cur_blanket_trait(&self, param: &str) -> Option<String> {
        let cur = self.cur_fn.as_deref()?;
        let (ty, _) = cur.split_once("::")?;
        let tr = ty.strip_suffix("$all")?;
        (self.adts.blanket_params.get(tr).map(String::as_str) == Some(param))
            .then(|| tr.to_string())
    }

    /// Whether `ty` meets the bound `b`: a std trait, or a user trait it implements.
    fn meets_bound(&self, ty: &Ty, b: &str) -> bool {
        // (Marker traits every value here has: one thread, no unsized values.)
        if matches!(
            b,
            "Sized" | "Send" | "Sync" | "Unpin" | "UnwindSafe" | "RefUnwindSafe"
        ) {
            return true;
        }
        if self.adts.traits.contains_key(b) {
            self.implements_user_trait(ty, b)
        } else if b == "Iterator" {
            // (`impl<I: Iterator> Tr for &mut I`.)
            match self.zonk(ty) {
                Ty::Iter { .. } | Ty::Range(..) => true,
                Ty::Adt(n, _) => self.adts.iter_items.contains_key(&n),
                _ => false,
            }
        } else {
            self.implements(ty, b)
        }
    }

    pub(super) fn implements_user_trait(&self, ty: &Ty, tr: &str) -> bool {
        // A blanket impl: whatever meets its bounds (a bound on the trait itself can't help).
        if let Some(bounds) = self.adts.blankets.get(tr)
            && !matches!(self.zonk(ty), Ty::Param(_) | Ty::Dyn(_) | Ty::Unknown)
            && bounds.iter().all(|b| b != tr && self.meets_bound(ty, b))
        {
            return true;
        }
        // A `String` and a `Formatter` are `fmt::Write`rs (through `κσ.$fmtWriteCall`).
        if tr == "FmtWrite" && matches!(self.zonk(ty), Ty::Str | Ty::Formatter) {
            return true;
        }
        // `impl std::error::Error for MyError {}`
        if tr == "Error"
            && let Ty::Adt(n, _) = self.zonk(ty)
        {
            return self
                .adts
                .defs
                .get(&n)
                .is_some_and(|d| d.implements("Error"));
        }
        match self.zonk(ty) {
            Ty::Adt(n, _) => self
                .adts
                .trait_impls
                .get(&n)
                .is_some_and(|ts| ts.iter().any(|t| self.trait_includes(t, tr))),
            Ty::Dyn(t) => self.trait_includes(&t, tr),
            // `impl Money for f64`
            t if crate::adt::ext_key_of(&t).is_some_and(|k| self.adts.ext_types.contains(k)) => {
                let k = crate::adt::ext_key_of(&t).unwrap_or_default();
                self.adts
                    .trait_impls
                    .get(k)
                    .is_some_and(|ts| ts.iter().any(|t| self.trait_includes(t, tr)))
            }
            Ty::Param(n) => self
                .param_bounds
                .get(&n)
                .is_some_and(|bs| bs.iter().any(|b| self.trait_includes(b, tr))),
            Ty::Unknown => true,
            _ => false,
        }
    }

    /// A call of a trait method on a value whose type is `self_ty` (a `dyn Trait` or a bounded
    /// type parameter); `Self` in the signature becomes `self_ty`.
    pub(super) fn call_trait_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        sig: Method,
        self_ty: Ty,
        args: &[ast::Expr],
    ) -> Ty {
        let mut sub: HashMap<String, Ty> = [("Self".to_string(), self_ty.clone())].into();
        // Through `S: Shape` / `C: Convert<String>`: `Shape::Unit` is `S::Unit` (or the bound's
        // `Unit = f64`), `Convert::T` the bound's argument.
        if let Ty::Param(p) = &self_ty
            && p != "Self"
        {
            sub.extend(self.bound_subst(p));
        }
        let sig = subst_method(&sig, &sub);
        // The method's own type parameters (`fn fold_with<F>`): fresh at each call.
        let sig = if sig.type_params.is_empty() {
            sig
        } else {
            self.method_inst(&sig, None)
        };
        if sig.receiver == Some(Receiver::MutRef) {
            self.check_mutable(recv, Mutation::Borrow);
        }
        self.arg_count(m.syntax(), "method", sig.params.len(), args.len());
        for (i, (a, p)) in args.iter().zip(&sig.params).enumerate() {
            if sig.cell_params.get(i) == Some(&true) {
                self.cell_arg(a);
            }
            self.check(a, p);
        }
        if sig.is_async {
            Ty::Future(Box::new(sig.ret))
        } else {
            sig.ret
        }
    }

    /// For a type parameter `p`, what its bounds' trait templates stand for: `Shape::Unit` →
    /// `p::Unit` (or the bound's `Unit = X`), `Convert::T` → the bound's argument.
    pub(super) fn bound_subst(&self, p: &str) -> HashMap<String, Ty> {
        let mut sub = HashMap::new();
        for tr in self.bound_traits(p) {
            let Some(def) = self.adts.traits.get(&tr) else {
                continue;
            };
            let given = self.param_trait_args.get(p).and_then(|m| m.get(&tr));
            for a in &def.assoc_types {
                let t = given
                    .and_then(|(_, named)| named.get(a).cloned())
                    .or_else(|| self.assoc_iter_proj(p, &tr, a))
                    .unwrap_or_else(|| Ty::Param(format!("{p}::{a}")));
                sub.insert(format!("{tr}::{a}"), t);
            }
            for (i, tp) in def.type_params.iter().enumerate() {
                let t = given
                    .and_then(|(pos, _)| pos.get(i).cloned())
                    .unwrap_or(Ty::Unknown);
                sub.insert(format!("{tr}::{tp}"), t);
            }
        }
        sub
    }

    /// `p::Unit` for a type parameter `p` whose bounds declare `type Unit`: the bound's `Unit =
    /// X`, or the projection itself.
    pub(super) fn projection(&self, p: &str, assoc: &str) -> Option<Ty> {
        // `Self::Item` in `trait IterExt: Iterator { fn f(self) -> Vec<Self::Item> { .. } }`: the
        // implementing iterator's item (unchecked; each impl's own signatures name it).
        if assoc == "Item"
            && self.bound_traits(p).iter().any(|t| {
                matches!(
                    t.as_str(),
                    "Iterator" | "DoubleEndedIterator" | "ExactSizeIterator"
                )
            })
            && !self.bound_traits(p).iter().any(|t| {
                self.adts
                    .traits
                    .get(t)
                    .is_some_and(|d| d.assoc_types.iter().any(|a| a == assoc))
            })
        {
            return Some(Ty::Unknown);
        }
        let tr = self.bound_traits(p).into_iter().find(|t| {
            self.adts
                .traits
                .get(t)
                .is_some_and(|d| d.assoc_types.iter().any(|a| a == assoc))
        })?;
        let given = self
            .param_trait_args
            .get(p)
            .and_then(|m| m.get(&tr))
            .and_then(|(_, named)| named.get(assoc).cloned());
        Some(
            given
                .or_else(|| self.assoc_iter_proj(p, &tr, assoc))
                .unwrap_or_else(|| Ty::Param(format!("{p}::{assoc}"))),
        )
    }

    /// `p::Iter` where `p`'s trait `tr` declares `type Iter: Iterator<Item = X>`: the runtime's
    /// iterator of `X` (its other associated types as `p`'s projections).
    fn assoc_iter_proj(&self, p: &str, tr: &str, a: &str) -> Option<Ty> {
        let def = self.adts.traits.get(tr)?;
        let elem = def.assoc_iters.get(a)?;
        let sub: HashMap<String, Ty> = def
            .assoc_types
            .iter()
            .map(|o| (format!("{tr}::{o}"), Ty::Param(format!("{p}::{o}"))))
            .collect();
        Some(Ty::Iter {
            elem: Box::new(super::generic::subst(elem, &sub)),
            mutable: false,
        })
    }

    /// `X::Y` as its bound's binding says (`X: Tr<Y = Z>` makes it `Z`), else itself.
    pub(super) fn normalize_proj(&self, t: Ty) -> Ty {
        let mut t = t;
        // (A few steps at most: `B::Item` → `A::Item` → ..)
        for _ in 0..4 {
            let Ty::Param(n) = &t else { return t };
            let Some((x, y)) = n.split_once("::") else {
                return t;
            };
            if !self.param_bounds.contains_key(x) {
                return t;
            }
            match self.projection(x, y) {
                Some(next) if next != t => t = next,
                _ => return t,
            }
        }
        t
    }

    /// Whether `name` matches any type: a trait's own template (`Shape::Unit`), or a projection
    /// (`S::Unit`) a `where` clause constrains or whose trait has a blanket impl (which rustc
    /// can see through, as with `impl<T> Pointable for T { type Init = T; }`).
    pub(super) fn is_template(&self, name: &str) -> bool {
        let Some((t, a)) = name.split_once("::") else {
            return false;
        };
        // (`I::Item` of an `I: IntoIterator`: a type parameter of its own.)
        if self.param_bounds.contains_key(name) {
            return false;
        }
        if self.adts.traits.contains_key(t) || self.lenient_projs.contains(name) {
            return true;
        }
        // (Any blanket-implemented trait with that associated type: the call may come from where
        // the bound isn't in scope.)
        self.adts.blankets.keys().any(|tr| {
            self.adts
                .traits
                .get(tr)
                .is_some_and(|d| d.assoc_types.iter().any(|x| x == a))
        })
    }

    /// Note the projections a `where` clause constrains (`where I::Item: PartialEq<T>`).
    pub(super) fn note_where_projections(&mut self, wc: Option<ast::WhereClause>) {
        for pred in wc.into_iter().flat_map(|w| w.predicates()) {
            if let Some(ast::Type::PathType(pt)) = pred.ty()
                && let Some(path) = pt.path()
                && let Some(q) = path.qualifier()
                && let (Some(qn), Some(n)) = (
                    q.as_single_name_ref(),
                    path.segment().and_then(|s| s.name_ref()),
                )
            {
                self.lenient_projs
                    .insert(format!("{}::{}", qn.text(), n.text()));
            }
        }
    }

    /// `t`, written in the impl of `tr` for `key`, with the impl's type parameters replaced by
    /// what the value of type `z` has for them (`T` of `impl<T> Tr for Stack<T>`, for a
    /// `Stack<i32>`). Parameters it can't place stay as they are.
    pub(super) fn through_impl_head(&self, z: &Ty, key: &str, tr: &str, t: &Ty) -> Ty {
        let Some(heads) = self.adts.impl_heads.get(&(key.to_string(), tr.to_string())) else {
            return t.clone();
        };
        let Some((pattern, _)) = heads.first() else {
            return t.clone();
        };
        let actual = match z {
            Ty::Adt(_, args) => args.clone(),
            other => crate::adt::ext_args(other),
        };
        let mut m = HashMap::new();
        for (p, a) in pattern.iter().zip(&actual) {
            if let Ty::Param(n) = p {
                m.insert(n.clone(), a.clone());
            }
        }
        super::generic::subst(t, &m)
    }

    /// Settle generic calls' projections (`S::Unit` of a call with `S = Square` is `Square`'s
    /// `type Unit`) whose `S` is known. `last` gives up on the rest.
    pub(super) fn resolve_projections(&mut self, last: bool) {
        // `S: Store<T>` with `S = Bag<String>` and `impl<T> Store<T> for Bag<T>`: `T` is
        // `String`. (Only when one impl of the trait is there to pick.)
        for (s, tr, args) in std::mem::take(&mut self.pending_trait_args) {
            let key = match &self.shallow(&s) {
                Ty::Adt(n, _) => Some(n.clone()),
                Ty::Var(_) => {
                    if !last {
                        self.pending_trait_args.push((s, tr, args));
                    }
                    continue;
                }
                Ty::Param(_) | Ty::Unknown => None,
                other => crate::adt::ext_key_of(other).map(str::to_string),
            };
            let Some(key) = key else { continue };
            let heads = self
                .adts
                .impl_heads
                .get(&(key.clone(), tr.clone()))
                .cloned()
                .unwrap_or_default();
            let [(_, want)] = heads.as_slice() else {
                continue;
            };
            // (`shallow`: the value's own unsettled arguments stay variables, to settle here.)
            let sh = self.shallow(&s);
            for (a, w) in args.iter().zip(want) {
                let w = self.through_impl_head(&sh, &key, &tr, w);
                let mut placed = true;
                w.walk(&mut |x| placed &= !matches!(x, Ty::Param(_)));
                if placed {
                    self.unify(a, &w);
                }
            }
        }
        let pending = std::mem::take(&mut self.pending_projections);
        for (s, tr, assoc, x) in pending {
            let z = self.zonk(&s);
            let key = match &z {
                Ty::Adt(n, _) => Some(n.clone()),
                Ty::Param(p) => {
                    if let Some(t) = self.projection(p, &assoc) {
                        self.unify(&x, &t);
                    }
                    continue;
                }
                Ty::Var(_) | Ty::Unknown => None,
                other => crate::adt::ext_key_of(other).map(str::to_string),
            };
            // (`impl<T> Container for Stack<T> { type Item = T; }`: `T` is the value's argument.)
            let found = key.and_then(|k| {
                let t = self
                    .adts
                    .impl_assoc
                    .get(&(k.clone(), tr.clone()))
                    .and_then(|m| m.get(&assoc).cloned())?;
                let sh = self.shallow(&s);
                Some(self.through_impl_head(&sh, &k, &tr, &t))
            });
            match found {
                Some(t) => {
                    self.unify(&x, &t);
                }
                None if !last && matches!(z, Ty::Var(_)) => {
                    self.pending_projections.push((s, tr, assoc, x));
                }
                None => {}
            }
        }
    }

    /// `dyn Trait` / `impl Trait` naming a user trait.
    pub(super) fn user_trait_type(&self, t: &ast::Type) -> Option<String> {
        let list = match t {
            ast::Type::ImplTraitType(i) => i.type_bound_list()?,
            ast::Type::DynTraitType(d) => d.type_bound_list()?,
            _ => return None,
        };
        list.bounds().find_map(|b| match b.ty()? {
            ast::Type::PathType(p) => {
                let n = p.path()?.segment()?.name_ref()?.text().to_string();
                (self.adts.traits.contains_key(&n) || self.file_trait_names.contains(&n))
                    .then_some(n)
            }
            _ => None,
        })
    }

    /// Methods on a `dyn Trait` value.
    pub(super) fn dyn_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        tr: &str,
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        match self.trait_method(tr, method) {
            Some(sig) if sig.receiver.is_some() => {
                self.note_split_method(m, tr, method);
                self.call_trait_method(m, recv, sig, Ty::Dyn(tr.to_string()), args)
            }
            // `boxed.as_ref()`: the `dyn` value itself (boxes are erased).
            _ if matches!(method, "as_ref" | "as_mut" | "borrow") && args.is_empty() => {
                Ty::Dyn(tr.to_string())
            }
            _ => {
                for a in args {
                    self.infer(a, None);
                }
                self.error(
                    m.syntax(),
                    format!("no method named `{method}` found for `dyn {tr}` in the current scope"),
                );
                Ty::Unknown
            }
        }
    }
}

pub fn subst_method(m: &Method, sub: &HashMap<String, Ty>) -> Method {
    Method {
        params: m
            .params
            .iter()
            .map(|p| super::generic::subst(p, sub))
            .collect(),
        ret: super::generic::subst(&m.ret, sub),
        is_async: m.is_async,
        receiver: m.receiver,
        cell_params: m.cell_params.clone(),
        into_params: m.into_params.clone(),
        impl_args: m.impl_args.clone(),
        type_params: m.type_params.clone(),
        ref_params: m.ref_params.clone(),
        ret_owned: m.ret_owned,
        ret_mut_ref: m.ret_mut_ref,
        ret_slot: m.ret_slot,
        opt_slot_params: m.opt_slot_params.clone(),
        cell_self: m.cell_self,
    }
}
