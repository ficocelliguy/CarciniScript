//! Reference-aware types for the borrow checker (the "RTy overlay"): the type checker erases
//! `&T` to `T`, so `RTy`s are rebuilt from the syntax of annotations, signatures and struct
//! fields, with regions attached. This module holds the file-level environment (struct fields,
//! function signatures, variance), the two conversions (syntax → `RTy`, erased `Ty` → `RTy`),
//! subtyping (which emits outlives constraints) and signature instantiation (fresh regions per
//! call, universal regions for the function being checked, rustc's elision rules).

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode, ast,
    ast::{HasGenericArgs, HasGenericParams, HasName, HasTypeBounds},
};

use crate::adt::Adts;
use crate::nll_bir::{Body, RTy, Region, Span};
use crate::ty::Ty;

/// A struct or enum as written: its lifetime and type parameters, its fields' types.
#[derive(Clone, Debug)]
pub struct AdtSyn {
    pub lifetimes: Vec<String>,
    pub type_params: Vec<String>,
    /// Variants (a struct is its one variant, named like the struct): fields (name, type).
    pub variants: Vec<(String, Vec<(String, ast::Type)>)>,
    /// Per lifetime parameter: invariant (used under `&mut` or in a cell).
    pub inv_lifetimes: Vec<bool>,
    /// Per type parameter: invariant.
    pub inv_types: Vec<bool>,
}

/// An `impl` header: `impl<'a, T> Parser<'a, T>`.
#[derive(Clone, Debug)]
pub struct ImplSyn {
    pub lifetimes: Vec<String>,
    pub type_params: Vec<String>,
    pub self_ty: Option<ast::Type>,
    /// `where 'a: 'b` and `<'a: 'b>` of the impl.
    pub outlives: Vec<(String, String)>,
}

/// A function or method as written.
#[derive(Clone, Debug)]
pub struct FnSyn {
    pub f: ast::Fn,
    pub imp: Option<ImplSyn>,
}

pub struct Env<'a> {
    pub src: &'a str,
    pub types: &'a HashMap<(usize, usize), Ty>,
    pub adts: &'a Adts,
    pub adt_syn: HashMap<String, AdtSyn>,
    /// Free functions by name (several: ambiguous, `None`).
    pub free_fns: HashMap<String, Option<FnSyn>>,
    /// Methods and associated functions by (type, name).
    pub methods: HashMap<(String, String), Option<FnSyn>>,
    /// Trait methods by name: whether any takes `&mut self` / `self` (for calls on type
    /// parameters).
    pub trait_recv: HashMap<String, Recv>,
}

/// How a method takes its receiver.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recv {
    Ref,
    MutRef,
    Value,
}

/// A signature instantiated for one call (or the function being checked).
#[derive(Clone, Debug)]
pub struct Sig {
    /// The receiver's type (`&'r Self`, `&'r mut Self`, `Self`), if a method.
    pub self_ty: Option<RTy>,
    pub params: Vec<RTy>,
    pub ret: RTy,
    /// Whether the return type is a reference itself (`-> &T`), for the escape messages.
    pub ret_direct_ref: bool,
}

/// Names (lifetime → region, type parameter → type, `Self`) a type is read under.
#[derive(Clone, Default)]
pub struct TyEnv {
    pub lifetimes: HashMap<String, Region>,
    pub params: HashMap<String, RTy>,
    pub self_ty: Option<RTy>,
}

fn path_name(p: &ast::PathType) -> Option<(String, Vec<ast::GenericArg>)> {
    let seg = p.path()?.segment()?;
    let name = seg.name_ref()?.text().to_string();
    let args = seg
        .generic_arg_list()
        .map(|l| l.generic_args().collect())
        .unwrap_or_default();
    Some((name, args))
}

fn type_args(args: &[ast::GenericArg]) -> Vec<ast::Type> {
    args.iter()
        .filter_map(|a| match a {
            ast::GenericArg::TypeArg(t) => t.ty(),
            _ => None,
        })
        .collect()
}

fn lifetime_args(args: &[ast::GenericArg]) -> Vec<String> {
    args.iter()
        .filter_map(|a| match a {
            ast::GenericArg::LifetimeArg(l) => l.lifetime().map(|l| l.text().to_string()),
            _ => None,
        })
        .collect()
}

/// The `Item = X` of an `impl Iterator<Item = X>` / `dyn Iterator<Item = X>` bound.
fn assoc_item(bounds: Option<ast::TypeBoundList>) -> Option<ast::Type> {
    for b in bounds?.bounds() {
        let Some(ast::Type::PathType(p)) = b.ty() else {
            continue;
        };
        let Some((name, args)) = path_name(&p) else {
            continue;
        };
        if !matches!(
            name.as_str(),
            "Iterator" | "IntoIterator" | "DoubleEndedIterator" | "ExactSizeIterator"
        ) {
            continue;
        }
        for a in args {
            if let ast::GenericArg::AssocTypeArg(at) = a
                && at.name_ref().is_some_and(|n| n.text() == "Item")
            {
                return at.ty();
            }
        }
    }
    None
}

/// The lifetime of an `impl Trait + 'a` / `dyn Trait + 'a` bound, and whether the bound is a
/// closure (`Fn(..)`), with its return type.
fn bound_parts(bounds: Option<ast::TypeBoundList>) -> (Option<String>, bool, Option<ast::Type>) {
    let (mut lt, mut is_fn, mut ret) = (None, false, None);
    for b in bounds.into_iter().flat_map(|l| l.bounds()) {
        if let Some(l) = b.lifetime() {
            lt = Some(l.text().to_string());
        }
        if let Some(ast::Type::PathType(p)) = b.ty()
            && let Some(seg) = p.path().and_then(|p| p.segment())
            && seg
                .name_ref()
                .is_some_and(|n| matches!(n.text().to_string().as_str(), "Fn" | "FnMut" | "FnOnce"))
        {
            is_fn = true;
            ret = seg.ret_type().and_then(|r| r.ty());
        }
    }
    (lt, is_fn, ret)
}

impl<'a> Env<'a> {
    pub fn new(
        file: &ast::SourceFile,
        src: &'a str,
        types: &'a HashMap<(usize, usize), Ty>,
        adts: &'a Adts,
    ) -> Env<'a> {
        let mut env = Env {
            src,
            types,
            adts,
            adt_syn: HashMap::new(),
            free_fns: HashMap::new(),
            methods: HashMap::new(),
            trait_recv: HashMap::new(),
        };
        for item in crate::adt::all_items(file) {
            match item {
                ast::Item::Struct(s) => {
                    let Some(name) = s.name() else { continue };
                    let (lifetimes, type_params) = generics_of(s.generic_param_list());
                    let fields = field_list(s.field_list());
                    env.adt_syn.insert(
                        name.text().to_string(),
                        AdtSyn {
                            inv_lifetimes: vec![false; lifetimes.len()],
                            inv_types: vec![false; type_params.len()],
                            lifetimes,
                            type_params,
                            variants: vec![(name.text().to_string(), fields)],
                        },
                    );
                }
                ast::Item::Enum(e) => {
                    let Some(name) = e.name() else { continue };
                    let (lifetimes, type_params) = generics_of(e.generic_param_list());
                    let variants = e
                        .variant_list()
                        .into_iter()
                        .flat_map(|l| l.variants())
                        .filter_map(|v| {
                            Some((v.name()?.text().to_string(), field_list(v.field_list())))
                        })
                        .collect();
                    env.adt_syn.insert(
                        name.text().to_string(),
                        AdtSyn {
                            inv_lifetimes: vec![false; lifetimes.len()],
                            inv_types: vec![false; type_params.len()],
                            lifetimes,
                            type_params,
                            variants,
                        },
                    );
                }
                ast::Item::Fn(f) => {
                    let Some(name) = f.name() else { continue };
                    let syn = FnSyn {
                        f: f.clone(),
                        imp: None,
                    };
                    env.free_fns
                        .entry(name.text().to_string())
                        .and_modify(|e| *e = None)
                        .or_insert(Some(syn));
                }
                ast::Item::Impl(imp) => {
                    let Some(ty) = crate::adt::impl_self_name(&imp) else {
                        continue;
                    };
                    let (lifetimes, type_params) = generics_of(imp.generic_param_list());
                    let outlives = outlives_of(imp.generic_param_list(), imp.where_clause());
                    let head = ImplSyn {
                        lifetimes,
                        type_params,
                        self_ty: imp.self_ty(),
                        outlives,
                    };
                    for f in imp
                        .assoc_item_list()
                        .into_iter()
                        .flat_map(|l| l.assoc_items())
                        .filter_map(|i| match i {
                            ast::AssocItem::Fn(f) => Some(f),
                            _ => None,
                        })
                    {
                        let Some(name) = f.name() else { continue };
                        let syn = FnSyn {
                            f: f.clone(),
                            imp: Some(head.clone()),
                        };
                        let key = (ty.clone(), name.text().to_string());
                        match env.methods.get(&key) {
                            // Two impls with the same method (a generic trait implemented
                            // twice): trusted only when neither hands out a borrow.
                            Some(Some(prev)) if !same_borrow_shape(&prev.f, &f) => {
                                env.methods.insert(key, None);
                            }
                            Some(_) => {}
                            None => {
                                env.methods.insert(key, Some(syn));
                            }
                        }
                    }
                }
                ast::Item::Trait(t) => {
                    for f in t
                        .assoc_item_list()
                        .into_iter()
                        .flat_map(|l| l.assoc_items())
                        .filter_map(|i| match i {
                            ast::AssocItem::Fn(f) => Some(f),
                            _ => None,
                        })
                    {
                        let Some(name) = f.name() else { continue };
                        let recv = match f.param_list().and_then(|l| l.self_param()) {
                            Some(sp) if sp.amp_token().is_some() && sp.mut_token().is_some() => {
                                Recv::MutRef
                            }
                            Some(sp) if sp.amp_token().is_some() => Recv::Ref,
                            Some(sp) => match sp.ty() {
                                Some(ast::Type::RefType(r)) if r.mut_token().is_some() => {
                                    Recv::MutRef
                                }
                                Some(ast::Type::RefType(_)) => Recv::Ref,
                                _ => Recv::Value,
                            },
                            None => continue,
                        };
                        env.trait_recv
                            .entry(name.text().to_string())
                            .and_modify(|r| {
                                // (Disagreeing traits: the safest choice.)
                                if *r != recv {
                                    *r = Recv::Ref;
                                }
                            })
                            .or_insert(recv);
                    }
                }
                _ => {}
            }
        }
        env.compute_variance();
        env
    }

    /// Variance of each ADT's parameters, by a fixpoint over the field types: a parameter used
    /// under `&mut` or inside a cell (or inside another ADT that is invariant in it) is
    /// invariant; otherwise covariant.
    fn compute_variance(&mut self) {
        loop {
            let mut changed = false;
            let names: Vec<String> = self.adt_syn.keys().cloned().collect();
            for name in names {
                let def = self.adt_syn[&name].clone();
                let (mut inv_l, mut inv_t) = (def.inv_lifetimes.clone(), def.inv_types.clone());
                for (_, fields) in &def.variants {
                    for (_, t) in fields {
                        self.mark_variance(t, false, &def, &mut inv_l, &mut inv_t);
                    }
                }
                if inv_l != def.inv_lifetimes || inv_t != def.inv_types {
                    changed = true;
                    let d = self.adt_syn.get_mut(&name).unwrap();
                    d.inv_lifetimes = inv_l;
                    d.inv_types = inv_t;
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn mark_variance(
        &self,
        t: &ast::Type,
        inv: bool,
        def: &AdtSyn,
        inv_l: &mut [bool],
        inv_t: &mut [bool],
    ) {
        let mark_lt = |l: &str, inv: bool, inv_l: &mut [bool]| {
            if inv && let Some(i) = def.lifetimes.iter().position(|x| x == l) {
                inv_l[i] = true;
            }
        };
        match t {
            ast::Type::RefType(r) => {
                let m = r.mut_token().is_some();
                if let Some(l) = r.lifetime() {
                    mark_lt(l.text(), inv, inv_l);
                }
                if let Some(inner) = r.ty() {
                    self.mark_variance(&inner, inv || m, def, inv_l, inv_t);
                }
            }
            ast::Type::PtrType(p) => {
                if let Some(inner) = p.ty() {
                    self.mark_variance(&inner, true, def, inv_l, inv_t);
                }
            }
            ast::Type::ParenType(p) => {
                if let Some(inner) = p.ty() {
                    self.mark_variance(&inner, inv, def, inv_l, inv_t);
                }
            }
            ast::Type::TupleType(tt) => {
                for f in tt.fields() {
                    self.mark_variance(&f, inv, def, inv_l, inv_t);
                }
            }
            ast::Type::ArrayType(a) => {
                if let Some(inner) = a.ty() {
                    self.mark_variance(&inner, inv, def, inv_l, inv_t);
                }
            }
            ast::Type::SliceType(s) => {
                if let Some(inner) = s.ty() {
                    self.mark_variance(&inner, inv, def, inv_l, inv_t);
                }
            }
            ast::Type::FnPtrType(_)
            | ast::Type::ForType(_)
            | ast::Type::ImplTraitType(_)
            | ast::Type::DynTraitType(_) => {
                // Function types are contravariant in their arguments: treat every lifetime and
                // parameter they name as invariant.
                for n in t.syntax().descendants_with_tokens() {
                    match n.kind() {
                        SyntaxKind::LIFETIME => {
                            if let Some(tok) = n.as_token() {
                                mark_lt(tok.text(), true, inv_l);
                            }
                        }
                        SyntaxKind::NAME_REF => {
                            if let Some(i) = def
                                .type_params
                                .iter()
                                .position(|p| p == n.to_string().as_str())
                            {
                                inv_t[i] = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            ast::Type::PathType(p) => {
                let Some((name, args)) = path_name(p) else {
                    return;
                };
                if args.is_empty() {
                    if inv && let Some(i) = def.type_params.iter().position(|x| *x == name) {
                        inv_t[i] = true;
                    }
                    return;
                }
                let cell = matches!(
                    name.as_str(),
                    "RefCell" | "Cell" | "Mutex" | "RwLock" | "UnsafeCell" | "OnceCell"
                );
                let lts = lifetime_args(&args);
                let tys = type_args(&args);
                let other = self.adt_syn.get(&name);
                for (i, l) in lts.iter().enumerate() {
                    let inner_inv = inv
                        || cell
                        || other.is_some_and(|o| o.inv_lifetimes.get(i).copied().unwrap_or(false));
                    mark_lt(l, inner_inv, inv_l);
                }
                for (i, ty) in tys.iter().enumerate() {
                    let inner_inv = inv
                        || cell
                        || other.is_some_and(|o| o.inv_types.get(i).copied().unwrap_or(false));
                    self.mark_variance(ty, inner_inv, def, inv_l, inv_t);
                }
            }
            _ => {}
        }
    }

    /// Whether a struct or enum of this file is `Copy`.
    pub fn is_copy_adt(&self, name: &str) -> bool {
        self.adts.defs.get(name).is_some_and(|d| d.derives("Copy"))
    }

    /// Whether values of `t` are moved by a by-value use.
    pub fn moves(&self, t: &RTy) -> bool {
        match t {
            RTy::Ref(..) | RTy::Str | RTy::Closure { .. } => false,
            RTy::String | RTy::Vec(_) | RTy::Set(_) | RTy::Map(..) | RTy::Cell(_) => true,
            RTy::Iter { .. } => true,
            RTy::Adt(n, ..) => self.adts.defs.contains_key(n) && !self.is_copy_adt(n),
            RTy::Tuple(es) => es.iter().any(|e| self.moves(e)),
            RTy::Option(e) => self.moves(e),
            RTy::Result(a, b) => self.moves(a) || self.moves(b),
            RTy::Opaque(m) => *m,
        }
    }

    /// Whether the ADT implements `Clone` (so `x.clone()` on a `&X` gives an `X`).
    pub fn is_clone_adt(&self, name: &str) -> bool {
        self.adts
            .defs
            .get(name)
            .is_some_and(|d| d.implements("Clone") || d.derives("Copy"))
    }

    /// The type of a field of a struct or variant of this file, for an instance `adt`.
    pub fn field_rty(&self, body: &mut Body, variant: &str, field: &str, adt: &RTy) -> Option<RTy> {
        let RTy::Adt(name, args, regions) = adt else {
            return None;
        };
        if let Some(def) = self.adt_syn.get(name) {
            let vname = if variant.is_empty() { name } else { variant };
            let (_, fields) = def.variants.iter().find(|(v, _)| v == vname)?;
            let (_, t) = fields.iter().find(|(f, _)| f == field)?;
            let mut tenv = TyEnv::default();
            for (i, l) in def.lifetimes.iter().enumerate() {
                if let Some(r) = regions.get(i) {
                    tenv.lifetimes.insert(l.clone(), *r);
                }
            }
            for (i, p) in def.type_params.iter().enumerate() {
                tenv.params
                    .insert(p.clone(), args.get(i).cloned().unwrap_or(RTy::opaque()));
            }
            tenv.self_ty = Some(adt.clone());
            // (An elided lifetime in a field type can't happen in accepted Rust.)
            return Some(self.rty_from_syntax(body, &tenv, t, &mut |b| b.new_region()));
        }
        // An ADT from another module: the erased field type.
        let def = self.adts.defs.get(name)?;
        let v = if variant.is_empty() {
            def.variants.first()?
        } else {
            def.variants.iter().find(|v| v.name == variant)?
        };
        let (_, f) = v.field(field)?;
        Some(self.rty_of_ty(body, &f.ty))
    }

    /// The region-free view of an erased type: references inside it are unknown, so it holds
    /// none.
    pub fn rty_of_ty(&self, body: &mut Body, t: &Ty) -> RTy {
        match t {
            Ty::Str => RTy::Str,
            Ty::Vec(e) => RTy::Vec(Box::new(self.rty_of_ty(body, e))),
            Ty::Option(e) => RTy::Option(Box::new(self.rty_of_ty(body, e))),
            Ty::Tuple(es) => RTy::Tuple(es.iter().map(|e| self.rty_of_ty(body, e)).collect()),
            Ty::Set(e) => RTy::Set(Box::new(self.rty_of_ty(body, e))),
            Ty::Map(k, v) => RTy::Map(
                Box::new(self.rty_of_ty(body, k)),
                Box::new(self.rty_of_ty(body, v)),
            ),
            Ty::Result(a, b) => RTy::Result(
                Box::new(self.rty_of_ty(body, a)),
                Box::new(self.rty_of_ty(body, b)),
            ),
            Ty::Coll(_, view) => self.rty_of_ty(body, view),
            Ty::RefCell(e) => RTy::Cell(Box::new(self.rty_of_ty(body, e))),
            Ty::MutRef(e) => {
                let r = body.new_region();
                RTy::Ref(r, true, Box::new(self.rty_of_ty(body, e)))
            }
            Ty::Iter { elem, mutable } => RTy::Iter {
                region: None,
                mutable: *mutable,
                elem: Box::new(self.rty_of_ty(body, elem)),
            },
            Ty::IntoIter(e) => RTy::Iter {
                region: None,
                mutable: false,
                elem: Box::new(self.rty_of_ty(body, e)),
            },
            Ty::Entry(_, v) => RTy::Iter {
                region: None,
                mutable: true,
                elem: Box::new(self.rty_of_ty(body, v)),
            },
            Ty::Closure { ret, .. } => RTy::Closure {
                region: None,
                ret: Box::new(self.rty_of_ty(body, ret)),
            },
            Ty::Future(e) => self.rty_of_ty(body, e),
            Ty::Adt(n, args) => {
                let regions = match self.adt_syn.get(n) {
                    Some(d) => (0..d.lifetimes.len()).map(|_| body.new_region()).collect(),
                    None => Vec::new(),
                };
                RTy::Adt(
                    n.clone(),
                    args.iter().map(|a| self.rty_of_ty(body, a)).collect(),
                    regions,
                )
            }
            _ => RTy::opaque(),
        }
    }

    /// The erased type recorded for a syntax node, if any.
    pub fn ty_at(&self, base: usize, n: &SyntaxNode) -> Option<&Ty> {
        let r = n.text_range();
        self.types
            .get(&(base + usize::from(r.start()), base + usize::from(r.end())))
    }

    /// A type as written. `elided` supplies the region of each `&T` without a lifetime (and of
    /// `'_`).
    pub fn rty_from_syntax(
        &self,
        body: &mut Body,
        tenv: &TyEnv,
        t: &ast::Type,
        elided: &mut dyn FnMut(&mut Body) -> Region,
    ) -> RTy {
        match t {
            ast::Type::RefType(r) => {
                let region = match r.lifetime().map(|l| l.text().to_string()) {
                    Some(l) if l != "'_" => self.named_region(body, tenv, &l),
                    _ => elided(body),
                };
                let inner = r
                    .ty()
                    .map(|t| self.rty_from_syntax(body, tenv, &t, elided))
                    .unwrap_or(RTy::opaque());
                RTy::Ref(region, r.mut_token().is_some(), Box::new(inner))
            }
            ast::Type::ParenType(p) => p
                .ty()
                .map(|t| self.rty_from_syntax(body, tenv, &t, elided))
                .unwrap_or(RTy::opaque()),
            ast::Type::TupleType(tt) => {
                let es: Vec<RTy> = tt
                    .fields()
                    .map(|f| self.rty_from_syntax(body, tenv, &f, elided))
                    .collect();
                if es.is_empty() { RTy::opaque() } else { RTy::Tuple(es) }
            }
            ast::Type::ArrayType(a) => RTy::Vec(Box::new(
                a.ty()
                    .map(|t| self.rty_from_syntax(body, tenv, &t, elided))
                    .unwrap_or(RTy::opaque()),
            )),
            ast::Type::SliceType(s) => RTy::Vec(Box::new(
                s.ty()
                    .map(|t| self.rty_from_syntax(body, tenv, &t, elided))
                    .unwrap_or(RTy::opaque()),
            )),
            ast::Type::ImplTraitType(it) => {
                self.bound_rty(body, tenv, it.type_bound_list(), elided)
            }
            ast::Type::DynTraitType(dt) => {
                self.bound_rty(body, tenv, dt.type_bound_list(), elided)
            }
            ast::Type::PathType(p) => self.path_rty(body, tenv, p, elided),
            _ => RTy::opaque(),
        }
    }

    fn bound_rty(
        &self,
        body: &mut Body,
        tenv: &TyEnv,
        bounds: Option<ast::TypeBoundList>,
        elided: &mut dyn FnMut(&mut Body) -> Region,
    ) -> RTy {
        let (lt, is_fn, ret) = bound_parts(bounds.clone());
        let region = match lt {
            Some(l) if l == "'static" => None,
            Some(l) if l != "'_" => Some(self.named_region(body, tenv, &l)),
            Some(_) => Some(elided(body)),
            None => None,
        };
        if is_fn {
            // (The return type has its own elision scope: a reference there is unknown.)
            let ret = match ret {
                Some(t) if !t.syntax().text().contains_char('&') => {
                    self.rty_from_syntax(body, tenv, &t, elided)
                }
                _ => RTy::opaque(),
            };
            return RTy::Closure {
                region,
                ret: Box::new(ret),
            };
        }
        if let Some(item) = assoc_item(bounds) {
            let elem = self.rty_from_syntax(body, tenv, &item, elided);
            return RTy::Iter {
                region,
                mutable: false,
                elem: Box::new(elem),
            };
        }
        match region {
            Some(r) => RTy::Iter {
                region: Some(r),
                mutable: false,
                elem: Box::new(RTy::opaque()),
            },
            None => RTy::opaque(),
        }
    }

    fn named_region(&self, body: &mut Body, tenv: &TyEnv, l: &str) -> Region {
        if l == "'static" {
            return Region(0);
        }
        match tenv.lifetimes.get(l) {
            Some(r) => *r,
            // (A lifetime not in scope: a fresh region, which constrains nothing.)
            None => body.new_region(),
        }
    }

    fn path_rty(
        &self,
        body: &mut Body,
        tenv: &TyEnv,
        p: &ast::PathType,
        elided: &mut dyn FnMut(&mut Body) -> Region,
    ) -> RTy {
        let Some((name, args)) = path_name(p) else {
            return RTy::opaque();
        };
        let tys = type_args(&args);
        let lts = lifetime_args(&args);
        let arg = |i: usize, body: &mut Body, elided: &mut dyn FnMut(&mut Body) -> Region| -> RTy {
            tys.get(i)
                .map(|t| self.rty_from_syntax(body, tenv, t, elided))
                .unwrap_or(RTy::opaque())
        };
        let region_arg = |i: usize, body: &mut Body, elided: &mut dyn FnMut(&mut Body) -> Region| {
            match lts.get(i).map(|s| s.as_str()) {
                Some("'static") => Region(0),
                Some("'_") | None => elided(body),
                Some(l) => self.named_region(body, tenv, l),
            }
        };
        // (The file's own types and type parameters first: an `enum Entry<'a>` isn't std's.)
        let own = self.adt_syn.contains_key(&name) || tenv.params.contains_key(&name);
        match if own { "" } else { name.as_str() } {
            "String" => RTy::String,
            "str" => RTy::Str,
            "Vec" | "VecDeque" | "BinaryHeap" | "LinkedList" | "SmallVec" => {
                RTy::Vec(Box::new(arg(0, body, elided)))
            }
            "HashSet" | "BTreeSet" | "IndexSet" => RTy::Set(Box::new(arg(0, body, elided))),
            "HashMap" | "BTreeMap" | "IndexMap" => {
                let k = arg(0, body, elided);
                let v = arg(1, body, elided);
                RTy::Map(Box::new(k), Box::new(v))
            }
            "Option" => RTy::Option(Box::new(arg(0, body, elided))),
            "Result" => {
                let a = arg(0, body, elided);
                let b = if tys.len() > 1 {
                    arg(1, body, elided)
                } else {
                    RTy::opaque()
                };
                RTy::Result(Box::new(a), Box::new(b))
            }
            "Box" | "Rc" | "Arc" => arg(0, body, elided),
            "RefCell" | "Cell" | "Mutex" | "RwLock" | "UnsafeCell" | "OnceCell" => {
                RTy::Cell(Box::new(arg(0, body, elided)))
            }
            "Cow" => {
                let r = region_arg(0, body, elided);
                RTy::Iter {
                    region: Some(r),
                    mutable: false,
                    elem: Box::new(arg(0, body, elided)),
                }
            }
            "Self" => tenv.self_ty.clone().unwrap_or(RTy::opaque()),
            // std's borrowing iterators and guards.
            "Iter" | "Chars" | "CharIndices" | "Bytes" | "Lines" | "Split" | "SplitWhitespace"
            | "SplitN" | "RSplit" | "Matches" | "Keys" | "Values" | "Windows" | "Chunks"
            | "ChunksExact" | "Ref" | "Entry" | "OccupiedEntry" | "VacantEntry" => {
                let r = region_arg(0, body, elided);
                let elem = match name.as_str() {
                    "Iter" | "Keys" | "Values" => RTy::Ref(r, false, Box::new(arg(0, body, elided))),
                    "Ref" => arg(0, body, elided),
                    "Windows" | "Chunks" | "ChunksExact" => {
                        RTy::Ref(r, false, Box::new(RTy::Vec(Box::new(arg(0, body, elided)))))
                    }
                    "Lines" | "Split" | "SplitWhitespace" | "SplitN" | "RSplit" | "Matches" => {
                        RTy::Ref(r, false, Box::new(RTy::Str))
                    }
                    _ => RTy::opaque(),
                };
                if name == "Ref" {
                    return RTy::Ref(r, false, Box::new(elem));
                }
                RTy::Iter {
                    region: Some(r),
                    mutable: false,
                    elem: Box::new(elem),
                }
            }
            "IterMut" | "ValuesMut" | "ChunksMut" | "Drain" | "RefMut" | "MutexGuard"
            | "RwLockWriteGuard" | "RwLockReadGuard" => {
                let r = region_arg(0, body, elided);
                let inner = arg(0, body, elided);
                if matches!(
                    name.as_str(),
                    "RefMut" | "MutexGuard" | "RwLockWriteGuard" | "RwLockReadGuard"
                ) {
                    return RTy::Ref(r, name != "RwLockReadGuard", Box::new(inner));
                }
                let elem = match name.as_str() {
                    "Drain" => inner,
                    "ChunksMut" => RTy::Ref(r, true, Box::new(RTy::Vec(Box::new(inner)))),
                    _ => RTy::Ref(r, true, Box::new(inner)),
                };
                RTy::Iter {
                    region: Some(r),
                    mutable: true,
                    elem: Box::new(elem),
                }
            }
            "Peekable" | "Rev" | "Enumerate" | "Map" | "Filter" | "Zip" | "Chain" | "Take"
            | "Skip" | "StepBy" | "Cloned" | "Copied" | "Fuse" | "FlatMap" | "Flatten"
            | "TakeWhile" | "SkipWhile" | "FilterMap" | "Inspect" | "Scan" | "Cycle" => {
                // An adapter over `I`: what `I` borrows.
                match arg(0, body, elided) {
                    RTy::Iter { region, mutable, .. } => RTy::Iter {
                        region,
                        mutable,
                        elem: Box::new(RTy::opaque()),
                    },
                    _ => RTy::Iter {
                        region: None,
                        mutable: false,
                        elem: Box::new(RTy::opaque()),
                    },
                }
            }
            "PhantomData" => RTy::opaque(),
            _ => {
                if let Some(t) = tenv.params.get(&name) {
                    return t.clone();
                }
                if let Some(def) = self.adt_syn.get(&name) {
                    let regions: Vec<Region> = (0..def.lifetimes.len())
                        .map(|i| region_arg(i, body, elided))
                        .collect();
                    let args: Vec<RTy> = (0..def.type_params.len())
                        .map(|i| arg(i, body, elided))
                        .collect();
                    return RTy::Adt(name, args, regions);
                }
                if let Some(def) = self.adts.defs.get(&name) {
                    let args: Vec<RTy> = (0..def.type_params.len())
                        .map(|i| arg(i, body, elided))
                        .collect();
                    return RTy::Adt(name, args, Vec::new());
                }
                RTy::opaque()
            }
        }
    }

    /// `sub <: sup`: the outlives constraints that make it so (with variance). Shapes that don't
    /// line up constrain nothing.
    pub fn relate(&self, body: &mut Body, sub: &RTy, sup: &RTy, span: Span) {
        match (sub, sup) {
            // `&&T` where `&T` is expected: auto-deref copies the inner reference out.
            (RTy::Ref(_, _, ia), RTy::Ref(_, _, ib)) if ia.is_ref() && !ib.is_ref() => {
                self.relate(body, ia, sup, span);
            }
            (RTy::Ref(ra, ma, ia), RTy::Ref(rb, mb, ib)) => {
                body.outlives(*ra, *rb, span);
                if *ma || *mb {
                    self.equate(body, ia, ib, span);
                } else {
                    self.relate(body, ia, ib, span);
                }
            }
            (RTy::Adt(n, aa, ra), RTy::Adt(m, ab, rb)) if n == m => {
                let def = self.adt_syn.get(n);
                for (i, (x, y)) in ra.iter().zip(rb).enumerate() {
                    if def.is_some_and(|d| d.inv_lifetimes.get(i).copied().unwrap_or(false)) {
                        body.outlives(*x, *y, span);
                        body.outlives(*y, *x, span);
                    } else {
                        body.outlives(*x, *y, span);
                    }
                }
                for (i, (x, y)) in aa.iter().zip(ab).enumerate() {
                    if def.is_some_and(|d| d.inv_types.get(i).copied().unwrap_or(false)) {
                        self.equate(body, x, y, span);
                    } else {
                        self.relate(body, x, y, span);
                    }
                }
            }
            (RTy::Tuple(a), RTy::Tuple(b)) => {
                for (x, y) in a.iter().zip(b) {
                    self.relate(body, x, y, span);
                }
            }
            (RTy::Option(a), RTy::Option(b))
            | (RTy::Vec(a), RTy::Vec(b))
            | (RTy::Set(a), RTy::Set(b)) => self.relate(body, a, b, span),
            (RTy::Cell(a), RTy::Cell(b)) => self.equate(body, a, b, span),
            (RTy::Result(a1, b1), RTy::Result(a2, b2)) | (RTy::Map(a1, b1), RTy::Map(a2, b2)) => {
                self.relate(body, a1, a2, span);
                self.relate(body, b1, b2, span);
            }
            (
                RTy::Iter {
                    region: ra,
                    mutable: ma,
                    elem: ea,
                },
                RTy::Iter {
                    region: rb,
                    mutable: mb,
                    elem: eb,
                },
            ) => {
                if let (Some(x), Some(y)) = (ra, rb) {
                    body.outlives(*x, *y, span);
                }
                if *ma || *mb {
                    self.equate(body, ea, eb, span);
                } else {
                    self.relate(body, ea, eb, span);
                }
            }
            (
                RTy::Closure {
                    region: ra,
                    ret: reta,
                },
                RTy::Closure {
                    region: rb,
                    ret: retb,
                },
            ) => {
                if let (Some(x), Some(y)) = (ra, rb) {
                    body.outlives(*x, *y, span);
                }
                self.relate(body, reta, retb, span);
            }
            _ => {}
        }
    }

    pub fn equate(&self, body: &mut Body, a: &RTy, b: &RTy, span: Span) {
        self.relate(body, a, b, span);
        self.relate(body, b, a, span);
    }

    /// Every region of `t` outlives `r` (`t` is held by something living for `r`).
    pub fn hold(&self, body: &mut Body, t: &RTy, r: Region, span: Span) {
        let mut rs = Vec::new();
        t.regions(&mut rs);
        for x in rs {
            body.outlives(x, r, span);
        }
    }

    /// The signature of a function of this file, with every lifetime a region made by `fresh`
    /// (universal regions for the function being checked, inference regions for a call).
    /// `self_adt` gives the receiver's type arguments (`Stack<i32>`) when known. `universal`:
    /// also record declared outlives (`'a: 'b`) on the regions, and name them.
    pub fn signature(
        &self,
        body: &mut Body,
        fs: &FnSyn,
        self_adt: Option<&RTy>,
        universal: bool,
        span: Span,
    ) -> Sig {
        let fresh = |body: &mut Body, name: &str| {
            if universal {
                body.new_universal(name)
            } else {
                body.new_region()
            }
        };
        let f = &fs.f;
        let mut tenv = TyEnv::default();
        let mut outlives: Vec<(String, String)> = Vec::new();
        if let Some(imp) = &fs.imp {
            for l in &imp.lifetimes {
                let r = fresh(body, l);
                tenv.lifetimes.insert(l.clone(), r);
            }
            outlives.extend(imp.outlives.iter().cloned());
        }
        let (fn_lts, fn_tps) = generics_of(f.generic_param_list());
        for l in &fn_lts {
            let r = fresh(body, l);
            tenv.lifetimes.insert(l.clone(), r);
        }
        outlives.extend(outlives_of(f.generic_param_list(), f.where_clause()));
        // Type parameters: the impl's from the receiver's arguments, the function's own opaque
        // (moved by value unless `Copy`).
        let copy_bound: Vec<String> = copy_params(f.generic_param_list(), f.where_clause());
        if let Some(imp) = &fs.imp {
            let args: &[RTy] = match self_adt {
                Some(RTy::Adt(_, args, _)) => args,
                _ => &[],
            };
            // (The impl's self type's arguments may be written in any order: match by name.)
            let order: Vec<String> = imp
                .self_ty
                .as_ref()
                .and_then(|t| match t {
                    ast::Type::PathType(p) => path_name(p).map(|(_, a)| {
                        type_args(&a)
                            .iter()
                            .map(|t| t.syntax().text().to_string())
                            .collect()
                    }),
                    _ => None,
                })
                .unwrap_or_default();
            for p in &imp.type_params {
                let at = order.iter().position(|o| o == p);
                let t = at
                    .and_then(|i| args.get(i).cloned())
                    .unwrap_or(RTy::Opaque(true));
                tenv.params.insert(p.clone(), t);
            }
        }
        for p in &fn_tps {
            tenv.params
                .insert(p.clone(), RTy::Opaque(!copy_bound.contains(p)));
        }
        // `Self`: the impl's self type, with the receiver's arguments.
        let elided_self = &mut |b: &mut Body| fresh(b, "'_");
        tenv.self_ty = match (&fs.imp, self_adt) {
            (Some(imp), _) => imp
                .self_ty
                .as_ref()
                .map(|t| self.rty_from_syntax(body, &tenv, t, elided_self)),
            _ => self_adt.cloned(),
        };
        if let (Some(RTy::Adt(_, _, regs)), Some(RTy::Adt(n, args, _))) = (&tenv.self_ty, self_adt)
        {
            // The receiver's own type arguments (from the type checker) override.
            tenv.self_ty = Some(RTy::Adt(n.clone(), args.clone(), regs.clone()));
        }
        if universal {
            for (a, b) in &outlives {
                let ra = tenv.lifetimes.get(a).copied();
                let rb = if b == "'static" {
                    Some(Region(0))
                } else {
                    tenv.lifetimes.get(b).copied()
                };
                if let (Some(ra), Some(rb)) = (ra, rb)
                    && let Some(u) = &mut body.regions[ra.idx()].universal
                {
                    u.outlives.push(rb);
                }
            }
        } else {
            for (a, b) in &outlives {
                let ra = tenv.lifetimes.get(a).copied();
                let rb = if b == "'static" {
                    Some(Region(0))
                } else {
                    tenv.lifetimes.get(b).copied()
                };
                if let (Some(ra), Some(rb)) = (ra, rb) {
                    body.outlives(ra, rb, span);
                }
            }
        }
        // Inputs: each elided lifetime its own region.
        let mut input_regions: Vec<Region> = Vec::new();
        let mut self_ty = None;
        let mut self_region: Option<Region> = None;
        let list = f.param_list();
        if let Some(sp) = list.as_ref().and_then(|l| l.self_param()) {
            let base = tenv.self_ty.clone().unwrap_or(RTy::opaque());
            self_ty = Some(match sp.ty() {
                Some(t) => {
                    let mut el = |b: &mut Body| fresh(b, "'_");
                    let t = self.rty_from_syntax(body, &tenv, &t, &mut el);
                    if let RTy::Ref(r, ..) = &t {
                        self_region = Some(*r);
                    }
                    t
                }
                None if sp.amp_token().is_some() => {
                    let r = match sp.lifetime().map(|l| l.text().to_string()) {
                        Some(l) if l != "'_" => self.named_region(body, &tenv, &l),
                        _ => fresh(body, "'_"),
                    };
                    self_region = Some(r);
                    RTy::Ref(r, sp.mut_token().is_some(), Box::new(base))
                }
                None => base,
            });
        }
        if let Some(t) = &self_ty {
            t.regions(&mut input_regions);
        }
        let mut params = Vec::new();
        for p in list.iter().flat_map(|l| l.params()) {
            let t = match p.ty() {
                Some(t) => {
                    let mut el = |b: &mut Body| fresh(b, "'_");
                    self.rty_from_syntax(body, &tenv, &t, &mut el)
                }
                None => RTy::opaque(),
            };
            t.regions(&mut input_regions);
            params.push(t);
        }
        // The output: elided lifetimes come from `&self`, else from the one input lifetime.
        let ret_syn = f.ret_type().and_then(|r| r.ty());
        let ret_direct_ref = matches!(ret_syn, Some(ast::Type::RefType(_)));
        let ret = match &ret_syn {
            None => RTy::opaque(),
            Some(t) => {
                let is_impl = matches!(t, ast::Type::ImplTraitType(_));
                let mut uniq: Vec<Region> = Vec::new();
                for r in &input_regions {
                    if !uniq.contains(r) {
                        uniq.push(*r);
                    }
                }
                let one = match self_region {
                    Some(r) => Some(r),
                    None if uniq.len() == 1 => Some(uniq[0]),
                    None => None,
                };
                if is_impl {
                    // `impl Trait` captures every input lifetime.
                    let cap = body.new_region();
                    for r in &uniq {
                        body.outlives(*r, cap, span);
                    }
                    let mut el = |_: &mut Body| cap;
                    self.rty_from_syntax(body, &tenv, t, &mut el)
                } else {
                    let mut el = |b: &mut Body| match one {
                        Some(r) => r,
                        None => fresh(b, "'_"),
                    };
                    self.rty_from_syntax(body, &tenv, t, &mut el)
                }
            }
        };
        if universal {
            // Implied bounds: `&'a T` is well-formed only if everything in `T` outlives `'a`.
            for t in params.iter().chain(self_ty.iter()) {
                implied_bounds(body, t);
            }
        }
        Sig {
            self_ty,
            params,
            ret,
            ret_direct_ref,
        }
    }
}

/// rustc's implied bounds for a parameter type with universal regions: the regions inside a
/// `&'a T` (or anything borrowing for `'a`) outlive `'a`.
pub fn implied_bounds(body: &mut Body, t: &RTy) {
    fn walk(body: &mut Body, t: &RTy, outer: Option<Region>) {
        let declare = |body: &mut Body, inner: Region, outer: Option<Region>| {
            if let Some(o) = outer
                && inner != o
                && inner != Region(0)
                && let Some(u) = &mut body.regions[inner.idx()].universal
                && !u.outlives.contains(&o)
            {
                u.outlives.push(o);
            }
        };
        match t {
            RTy::Ref(r, _, inner) => {
                declare(body, *r, outer);
                walk(body, inner, Some(*r));
            }
            RTy::Iter { region, elem, .. } => {
                if let Some(r) = region {
                    declare(body, *r, outer);
                    walk(body, elem, Some(*r));
                } else {
                    walk(body, elem, outer);
                }
            }
            RTy::Closure { region, ret } => {
                if let Some(r) = region {
                    declare(body, *r, outer);
                }
                walk(body, ret, outer);
            }
            RTy::Adt(_, args, rs) => {
                for r in rs {
                    declare(body, *r, outer);
                }
                for a in args {
                    walk(body, a, outer);
                }
            }
            RTy::Tuple(es) => {
                for e in es {
                    walk(body, e, outer);
                }
            }
            RTy::Option(e) | RTy::Vec(e) | RTy::Set(e) | RTy::Cell(e) => walk(body, e, outer),
            RTy::Result(a, b) | RTy::Map(a, b) => {
                walk(body, a, outer);
                walk(body, b, outer);
            }
            RTy::String | RTy::Str | RTy::Opaque(_) => {}
        }
    }
    walk(body, t, None);
}

/// Lifetime and type parameter names of a generic parameter list.
pub fn generics_of(g: Option<ast::GenericParamList>) -> (Vec<String>, Vec<String>) {
    let (mut lts, mut tps) = (Vec::new(), Vec::new());
    for p in g.into_iter().flat_map(|g| g.generic_params()) {
        match p {
            ast::GenericParam::LifetimeParam(l) => {
                if let Some(l) = l.lifetime() {
                    lts.push(l.text().to_string());
                }
            }
            ast::GenericParam::TypeParam(t) => {
                if let Some(n) = t.name() {
                    tps.push(n.text().to_string());
                }
            }
            ast::GenericParam::ConstParam(_) => {}
        }
    }
    (lts, tps)
}

/// `'a: 'b` relations declared in a generic parameter list and where clause.
fn outlives_of(
    g: Option<ast::GenericParamList>,
    w: Option<ast::WhereClause>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for p in g.into_iter().flat_map(|g| g.generic_params()) {
        if let ast::GenericParam::LifetimeParam(l) = p
            && let Some(name) = l.lifetime()
        {
            for b in l.type_bound_list().into_iter().flat_map(|b| b.bounds()) {
                if let Some(bl) = b.lifetime() {
                    out.push((name.text().to_string(), bl.text().to_string()));
                }
            }
        }
    }
    for pred in w.into_iter().flat_map(|w| w.predicates()) {
        let Some(l) = pred.lifetime() else { continue };
        for b in pred.type_bound_list().into_iter().flat_map(|b| b.bounds()) {
            if let Some(bl) = b.lifetime() {
                out.push((l.text().to_string(), bl.text().to_string()));
            }
        }
    }
    out
}

/// Type parameters bounded by `Copy`.
fn copy_params(g: Option<ast::GenericParamList>, w: Option<ast::WhereClause>) -> Vec<String> {
    let mut out = Vec::new();
    let is_copy = |b: ast::TypeBound| {
        b.ty()
            .is_some_and(|t| t.syntax().text().to_string().trim() == "Copy")
    };
    for p in g.into_iter().flat_map(|g| g.generic_params()) {
        if let ast::GenericParam::TypeParam(t) = p
            && let Some(n) = t.name()
            && t.type_bound_list()
                .into_iter()
                .flat_map(|b| b.bounds())
                .any(is_copy)
        {
            out.push(n.text().to_string());
        }
    }
    for pred in w.into_iter().flat_map(|w| w.predicates()) {
        let Some(ast::Type::PathType(t)) = pred.ty() else {
            continue;
        };
        let Some((name, _)) = path_name(&t) else {
            continue;
        };
        if pred
            .type_bound_list()
            .into_iter()
            .flat_map(|b| b.bounds())
            .any(is_copy)
        {
            out.push(name);
        }
    }
    out
}

fn field_list(fl: Option<ast::FieldList>) -> Vec<(String, ast::Type)> {
    match fl {
        Some(ast::FieldList::RecordFieldList(r)) => r
            .fields()
            .filter_map(|f| Some((f.name()?.text().to_string(), f.ty()?)))
            .collect(),
        Some(ast::FieldList::TupleFieldList(t)) => t
            .fields()
            .enumerate()
            .filter_map(|(i, f)| Some((i.to_string(), f.ty()?)))
            .collect(),
        None => Vec::new(),
    }
}

/// Whether two same-named methods hand out the same borrows (neither does: the common case of
/// a generic trait implemented twice).
fn same_borrow_shape(a: &ast::Fn, b: &ast::Fn) -> bool {
    let shape = |f: &ast::Fn| {
        f.ret_type()
            .map(|r| r.syntax().text().to_string())
            .unwrap_or_default()
            .replace(' ', "")
    };
    let sa = shape(a);
    let sb = shape(b);
    (!sa.contains('&') && !sa.contains('\'') && !sb.contains('&') && !sb.contains('\''))
        || sa == sb
}
