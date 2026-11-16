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
            if let Some(gp) = t.generic_param_list()
                && gp
                    .generic_params()
                    .any(|p| matches!(p, ast::GenericParam::ConstParam(_)))
            {
                self.error(
                    gp.syntax(),
                    "const generic parameters on traits are not supported yet (they are on structs, functions and impls)",
                );
            }
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
                    module: None,
                    assoc_types,
                    type_params,
                    assoc_bounds,
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
            let (params, assoc) = def
                .map(|d| (d.type_params, d.assoc_types))
                .unwrap_or_default();
            let saved_assoc = std::mem::replace(
                &mut self.impl_assoc,
                assoc
                    .iter()
                    .map(|a| (a.clone(), Ty::Param(format!("{n}::{a}"))))
                    .collect(),
            );
            self.generics.clear();
            for p in &params {
                self.generics
                    .insert(p.clone(), Ty::Param(format!("{n}::{p}")));
            }
            let mut methods = Vec::new();
            let mut consts = Vec::new();
            let mut const_tys = HashMap::new();
            for it in t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                let ast::AssocItem::Fn(f) = it else {
                    // `const X: T;`: each impl gives the value.
                    if let ast::AssocItem::Const(k) = &it
                        && k.body().is_none()
                    {
                        if let Some(n) = k.name() {
                            consts.push(n.text().to_string());
                            let t = k.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                            const_tys.insert(n.text().to_string(), t);
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
                        "only methods, `type X;` and `const X: T;` declarations are supported in traits so far (default consts are planned for phase 4)",
                    );
                    continue;
                };
                let Some(fname) = f.name() else { continue };
                let sig = self.lower_method(&f);
                // (A default one without `self` is copied onto each implementing class.)
                methods.push(TraitMethod {
                    name: fname.text().to_string(),
                    sig,
                    has_default: f.body().is_some(),
                });
            }
            self.self_ty = saved;
            self.impl_assoc = saved_assoc;
            self.generics.clear();
            if let Some(def) = self.adts.traits.get_mut(&n) {
                def.methods = methods;
                def.consts = consts;
                def.const_tys = const_tys;
            }
        }
    }

    /// The type an impl's `Self` names: a struct or enum, or a built-in type's `ext_key`.
    pub(super) fn self_ty_named(&self, type_name: &str) -> Ty {
        match type_name {
            "String" => Ty::Str,
            "Vec" => Ty::Vec(Box::new(Ty::Unknown)),
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
                    .filter(|k| !given_consts.contains(k))
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
        let mut m: HashMap<String, Ty> =
            [("Self".to_string(), self.self_ty_named(type_name))].into();
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
            for (p, t) in def.type_params.iter().zip(given) {
                let t = self.lower_type(&t);
                m.insert(format!("{tr}::{p}"), t);
            }
        }
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
        if impls.iter().any(|t| t == tr) {
            let node = imp
                .trait_()
                .map(|t| t.syntax().clone())
                .unwrap_or_else(|| imp.syntax().clone());
            let msg = if def.type_params.is_empty() {
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
                let ast::AssocItem::Fn(f) = it else { continue };
                if f.body().is_none() {
                    continue;
                }
                let Some(fname) = f.name() else { continue };
                let Some(tm) = def.methods.iter().find(|m| m.name == fname.text()) else {
                    continue;
                };
                let saved = self.self_ty.replace(Ty::Param("Self".into()));
                self.param_bounds.insert("Self".into(), vec![tr.clone()]);
                let self_local = f
                    .param_list()
                    .and_then(|pl| pl.self_param())
                    .and_then(|sp| {
                        Some((
                            sp.name()?,
                            Local {
                                ty: Ty::Param("Self".into()),
                                mutable: sp.amp_token().is_none() && sp.mut_token().is_some(),
                                deferred: false,
                                reference: sp.amp_token().map(|_| sp.mut_token().is_some()),
                                decl: None,
                            },
                        ))
                    });
                self.check_body(
                    &f,
                    tm.sig.params.clone(),
                    tm.sig.ret.clone(),
                    tm.sig.is_async,
                    self_local,
                );
                self.param_bounds.clear();
                self.self_ty = saved;
            }
        }
    }

    /// Method `m` of trait `tr` or one of its supertraits.
    pub(super) fn trait_method(&self, tr: &str, m: &str) -> Option<Method> {
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
                return Some(tm.sig.clone());
            }
            todo.extend(def.supers.iter().cloned());
        }
        None
    }

    /// Whether trait `tr` is `want` or has it as a (transitive) supertrait.
    pub(super) fn trait_includes(&self, tr: &str, want: &str) -> bool {
        let mut seen = Vec::new();
        let mut todo = vec![tr.to_string()];
        while let Some(t) = todo.pop() {
            if t == want {
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

    /// Whether `ty` meets the bound `b`: a std trait, or a user trait it implements.
    fn meets_bound(&self, ty: &Ty, b: &str) -> bool {
        if self.adts.traits.contains_key(b) {
            self.implements_user_trait(ty, b)
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
        Some(given.unwrap_or_else(|| Ty::Param(format!("{p}::{assoc}"))))
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

    /// Settle generic calls' projections (`S::Unit` of a call with `S = Square` is `Square`'s
    /// `type Unit`) whose `S` is known. `last` gives up on the rest.
    pub(super) fn resolve_projections(&mut self, last: bool) {
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
            let found = key.and_then(|k| {
                self.adts
                    .impl_assoc
                    .get(&(k, tr.clone()))
                    .and_then(|m| m.get(&assoc).cloned())
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
                self.adts.traits.contains_key(&n).then_some(n)
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
    }
}
