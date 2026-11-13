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
            if let Some(gp) = t.generic_param_list()
                && gp.generic_params().next().is_some()
            {
                self.error(
                    gp.syntax(),
                    "generic traits are not supported yet (planned for phase 4)",
                );
            }
            self.adts.traits.insert(
                n.clone(),
                TraitDef {
                    supers: supertraits(t),
                    methods: Vec::new(),
                    module: None,
                },
            );
        }
        for item in crate::adt::all_items(file) {
            let ast::Item::Trait(t) = &item else { continue };
            let Some(name) = t.name() else { continue };
            let n = name.text().to_string();
            let saved = self.self_ty.replace(Ty::Param("Self".into()));
            let mut methods = Vec::new();
            for it in t
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                let ast::AssocItem::Fn(f) = it else {
                    self.error(
                        it.syntax(),
                        "only methods are supported in traits so far (associated types and consts are planned for phase 4)",
                    );
                    continue;
                };
                let Some(fname) = f.name() else { continue };
                let sig = self.lower_method(&f);
                if sig.receiver.is_none() {
                    self.error(
                        fname.syntax(),
                        "trait functions without `self` are not supported yet (generics are erased, so `T::new()` has no type to call it on)",
                    );
                }
                methods.push(TraitMethod {
                    name: fname.text().to_string(),
                    sig,
                    has_default: f.body().is_some(),
                });
            }
            self.self_ty = saved;
            if let Some(def) = self.adts.traits.get_mut(&n) {
                def.methods = methods;
            }
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
                format!("cannot find trait `{tr}` in this scope (std traits other than `Display` and `Default` are derived, not implemented)"),
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
        let missing: Vec<String> = def
            .methods
            .iter()
            .filter(|m| !m.has_default && !given.iter().any(|(n, _)| *n == m.name))
            .map(|m| format!("`{}`", m.name))
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
        let m: HashMap<String, Ty> = [("Self".to_string(), Ty::Adt(type_name.to_string()))].into();
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
            self.error(
                &node,
                format!("conflicting implementations of trait `{tr}` for type `{type_name}`"),
            );
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
    pub(super) fn implements_user_trait(&self, ty: &Ty, tr: &str) -> bool {
        match self.zonk(ty) {
            Ty::Adt(n) => self
                .adts
                .trait_impls
                .get(&n)
                .is_some_and(|ts| ts.iter().any(|t| self.trait_includes(t, tr))),
            Ty::Dyn(t) => self.trait_includes(&t, tr),
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
        let sub: HashMap<String, Ty> = [("Self".to_string(), self_ty)].into();
        let sig = subst_method(&sig, &sub);
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
    }
}
