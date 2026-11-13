//! Type checking for structs, enums and `impl` blocks. A child module of `typeck`, so it works
//! directly on the checker's state.

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasGenericParams, HasName},
};

use super::{Ap, Checker, Local, Mutation};
use crate::adt::{
    AdtDef, Field, KNOWN_DERIVES, Method, Receiver, Resolved, Shape, Variant, derives_of,
    impl_parts,
};
use crate::ty::Ty;

impl Checker<'_> {
    // ------------------------------------------------------------ items

    /// Declare every struct and enum, lower their fields, then read `impl` signatures. Runs
    /// before function signatures so they can name these types.
    pub(super) fn collect_adts(&mut self, file: &ast::SourceFile) {
        // Names first, so fields can refer to any type in the file.
        for item in crate::adt::all_items(file) {
            let (name, is_enum, attrs_of): (Option<ast::Name>, bool, Vec<String>) = match &item {
                ast::Item::Struct(s) => (s.name(), false, derives_of(s)),
                ast::Item::Enum(e) => (e.name(), true, derives_of(e)),
                _ => continue,
            };
            let Some(name) = name else { continue };
            let n = name.text().to_string();
            if self.adts.defs.contains_key(&n) || self.fns.contains_key(&n) {
                self.error(
                    name.syntax(),
                    format!("the name `{n}` is defined multiple times"),
                );
                continue;
            }
            self.adts.defs.insert(
                n.clone(),
                AdtDef {
                    // Structs always have their one variant, even before fields are lowered.
                    variants: if is_enum {
                        Vec::new()
                    } else {
                        vec![Variant {
                            name: n.clone(),
                            shape: Shape::Unit,
                            fields: Vec::new(),
                            discr: 0,
                        }]
                    },
                    name: n,
                    is_enum,
                    derives: attrs_of,
                    display: false,
                    default_impl: false,
                    default_variant: None,
                    module: None,
                },
            );
        }
        for item in crate::adt::all_items(file) {
            match &item {
                ast::Item::Struct(s) => {
                    let Some(name) = s.name() else { continue };
                    self.reject_generics(s.generic_param_list(), "structs");
                    let variant =
                        self.lower_variant(name.text().to_string().as_str(), s.field_list());
                    if let Some(def) = self.adts.defs.get_mut(name.text().to_string().as_str())
                        && !def.is_enum
                    {
                        def.variants = vec![variant];
                    }
                }
                ast::Item::Enum(e) => {
                    let Some(name) = e.name() else { continue };
                    self.reject_generics(e.generic_param_list(), "enums");
                    let mut variants = Vec::new();
                    let mut default_variant = None;
                    let mut next_discr = 0i64;
                    for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
                        let Some(vn) = v.name() else { continue };
                        if variants
                            .iter()
                            .any(|x: &Variant| x.name == vn.text().to_string().as_str())
                        {
                            self.error(
                                vn.syntax(),
                                format!("the name `{}` is defined multiple times", vn.text()),
                            );
                            continue;
                        }
                        if v.attrs().any(|a| a.syntax().text() == "#[default]") {
                            default_variant = Some(variants.len());
                        }
                        if let Some(d) = v.const_arg().and_then(|c| c.expr()) {
                            let text: String = d
                                .syntax()
                                .text()
                                .to_string()
                                .chars()
                                .filter(|c| !c.is_whitespace() && *c != '_')
                                .collect();
                            match text.parse::<i64>() {
                                Ok(n) => next_discr = n,
                                Err(_) => self.error(
                                    d.syntax(),
                                    "enum discriminants must be integer literals",
                                ),
                            }
                        }
                        let mut variant =
                            self.lower_variant(vn.text().to_string().as_str(), v.field_list());
                        variant.discr = next_discr;
                        next_discr += 1;
                        variants.push(variant);
                    }
                    if let Some(def) = self.adts.defs.get_mut(name.text().to_string().as_str())
                        && def.is_enum
                    {
                        def.variants = variants;
                        def.default_variant = default_variant;
                        if def.derives("Default") && default_variant.is_none() {
                            self.error(
                                name.syntax(),
                                "no default declared: `#[derive(Default)]` on an enum needs a `#[default]` unit variant",
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        for item in crate::adt::all_items(file) {
            match &item {
                ast::Item::Struct(s) => {
                    if let Some(n) = s.name() {
                        self.check_derives(n.text(), s.syntax());
                    }
                }
                ast::Item::Enum(e) => {
                    if let Some(n) = e.name() {
                        self.check_derives(n.text(), e.syntax());
                    }
                }
                _ => {}
            }
        }
        self.collect_traits(file);
        // `impl` signatures.
        for item in crate::adt::all_items(file) {
            if let ast::Item::Impl(imp) = &item {
                self.collect_impl(imp);
            }
        }
        // `#[derive(Default)]` provides `T::default()`.
        let derived: Vec<String> = self
            .adts
            .defs
            .values()
            .filter(|d| d.derives("Default"))
            .map(|d| d.name.clone())
            .collect();
        for n in derived {
            self.adts
                .methods
                .entry(n.clone())
                .or_default()
                .entry("default".into())
                .or_insert(Method {
                    params: vec![],
                    ret: Ty::Adt(n),
                    is_async: false,
                    receiver: None,
                    cell_params: vec![],
                });
        }
    }

    fn reject_generics(&mut self, gp: Option<ast::GenericParamList>, what: &str) {
        if let Some(gp) = gp
            && gp
                .generic_params()
                .any(|p| !matches!(p, ast::GenericParam::LifetimeParam(_)))
        {
            self.error(
                gp.syntax(),
                format!("generic {what} are not supported yet (planned for phase 4)"),
            );
        }
    }

    fn lower_variant(&mut self, name: &str, fields: Option<ast::FieldList>) -> Variant {
        match fields {
            Some(ast::FieldList::RecordFieldList(l)) => Variant {
                discr: 0,
                name: name.into(),
                shape: Shape::Named,
                fields: l
                    .fields()
                    .map(|f| Field {
                        name: f.name().map(|n| n.text().to_string()).unwrap_or_default(),
                        ty: f.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
                    })
                    .collect(),
            },
            Some(ast::FieldList::TupleFieldList(l)) => Variant {
                discr: 0,
                name: name.into(),
                shape: Shape::Tuple,
                fields: l
                    .fields()
                    .enumerate()
                    .map(|(i, f)| Field {
                        name: i.to_string(),
                        ty: f.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
                    })
                    .collect(),
            },
            None => Variant {
                discr: 0,
                name: name.into(),
                shape: Shape::Unit,
                fields: Vec::new(),
            },
        }
    }

    /// Unknown derives, missing supertraits (`Copy` without `Clone`), and fields that don't
    /// implement a derived trait (`Eq` with an `f64` field).
    fn check_derives(&mut self, name: &str, node: &SyntaxNode) {
        let Some(def) = self.adts.defs.get(name).cloned() else {
            return;
        };
        for d in &def.derives {
            if !KNOWN_DERIVES.contains(&d.as_str()) {
                self.error(node, format!("`#[derive({d})]` is not supported"));
            }
        }
        for (t, needs) in [
            ("Copy", "Clone"),
            ("Eq", "PartialEq"),
            ("PartialOrd", "PartialEq"),
            ("Ord", "PartialOrd"),
            ("Ord", "Eq"),
        ] {
            if def.derives(t) && !def.derives(needs) {
                self.error(
                    node,
                    format!("the trait bound `{name}: {needs}` is not satisfied (`{t}` requires `{needs}`; add it to the derive)"),
                );
            }
        }
        for d in &def.derives {
            if d == "Default" && def.is_enum {
                continue;
            }
            for v in &def.variants {
                for f in &v.fields {
                    if !self.implements(&f.ty, d) {
                        self.error(
                            node,
                            format!("the trait `{d}` is not implemented for `{}` (field `{}` of `{name}`)", f.ty.describe(), f.name),
                        );
                    }
                }
            }
        }
    }

    /// Whether `ty` implements one of the derivable traits.
    pub(super) fn implements(&self, ty: &Ty, t: &str) -> bool {
        let ty = self.zonk(ty);
        match &ty {
            Ty::Adt(n) => match self.adts.defs.get(n) {
                Some(def) if t == "Default" => def.has_default(),
                Some(def) => def.derives(t),
                None => true,
            },
            Ty::Float { .. } => !matches!(t, "Eq" | "Ord" | "Hash"),
            Ty::Str | Ty::Vec(_) => {
                t != "Copy" && ty.inner_types().iter().all(|i| self.implements(i, t))
            }
            Ty::Set(_) | Ty::Map(..) => {
                !matches!(t, "Copy" | "Hash" | "PartialOrd" | "Ord")
                    && ty.inner_types().iter().all(|i| self.implements(i, t))
            }
            Ty::Option(_) | Ty::Tuple(_) | Ty::Result(..) => {
                ty.inner_types().iter().all(|i| self.implements(i, t))
            }
            Ty::RefCell(_) => {
                !matches!(t, "Copy" | "Hash")
                    && ty.inner_types().iter().all(|i| self.implements(i, t))
            }
            Ty::Closure { .. } | Ty::Iter { .. } | Ty::Future(_) | Ty::Entry(..) => false,
            Ty::Param(n) => self.param_has(n, t),
            Ty::Dyn(tr) => self.trait_includes(tr, t),
            _ => true,
        }
    }

    fn collect_impl(&mut self, imp: &ast::Impl) {
        let (trait_name, type_name) = impl_parts(imp);
        let Some(type_name) = type_name else { return };
        let node = imp
            .self_ty()
            .map(|t| t.syntax().clone())
            .unwrap_or_else(|| imp.syntax().clone());
        if !self.adts.defs.contains_key(&type_name) {
            self.error(&node, format!("cannot find type `{type_name}` in this scope (impls are for structs and enums in this file)"));
            return;
        }
        self.reject_generics(imp.generic_param_list(), "impls");
        match trait_name.as_deref() {
            None => {}
            Some("Default") => {
                if let Some(d) = self.adts.defs.get_mut(&type_name) {
                    d.default_impl = true;
                }
            }
            Some("Display") => {
                if let Some(d) = self.adts.defs.get_mut(&type_name) {
                    d.display = true;
                }
            }
            Some(t) if self.adts.traits.contains_key(t) => {
                self.register_trait_impl(imp, t, &type_name);
            }
            Some(t) => {
                let node = imp.trait_().map(|t| t.syntax().clone()).unwrap_or(node);
                self.error(
                    &node,
                    format!("`impl {t} for ..` is not supported yet (std traits other than `Display` and `Default` are derived; user traits declared in the file work)"),
                );
                return;
            }
        }
        let self_ty = Ty::Adt(type_name.clone());
        let saved = self.self_ty.replace(self_ty);
        for item in imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
        {
            let ast::AssocItem::Fn(f) = item else {
                self.error(
                    item.syntax(),
                    "only functions are supported in `impl` blocks",
                );
                continue;
            };
            let Some(name) = f.name() else { continue };
            let method = self.lower_method(&f);
            let methods = self.adts.methods.entry(type_name.clone()).or_default();
            if trait_name.is_none() && methods.contains_key(name.text().to_string().as_str()) {
                self.error(
                    name.syntax(),
                    format!("duplicate definitions with name `{}`", name.text()),
                );
                continue;
            }
            // Trait methods (`fmt`, `default`) live alongside inherent ones under their own name.
            methods.insert(name.text().to_string(), method);
        }
        self.self_ty = saved;
    }

    /// A method's signature (without `self`), with `Self` lowered per `self.self_ty`.
    pub(super) fn lower_method(&mut self, f: &ast::Fn) -> Method {
        self.reject_generics(f.generic_param_list(), "methods");
        let receiver = f.param_list().and_then(|pl| pl.self_param()).map(|sp| {
            if sp.amp_token().is_some() {
                if sp.mut_token().is_some() {
                    Receiver::MutRef
                } else {
                    Receiver::Ref
                }
            } else {
                Receiver::Value
            }
        });
        let params: Vec<Ty> = f
            .param_list()
            .map(|pl| {
                pl.params()
                    .map(|p| p.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown))
                    .collect()
            })
            .unwrap_or_default();
        let cell_params = f
            .param_list()
            .map(|pl| {
                pl.params()
                    .zip(params.iter())
                    .map(|(p, t)| {
                        matches!(p.ty(), Some(ast::Type::RefType(r)) if r.mut_token().is_some())
                            && self.cellable(t)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ret = f
            .ret_type()
            .and_then(|r| r.ty())
            .map(|t| self.lower_type(&t))
            .unwrap_or(Ty::Unit);
        Method {
            params,
            ret,
            is_async: f.async_token().is_some(),
            receiver,
            cell_params,
        }
    }

    /// Check the bodies of every function in `impl` blocks.
    pub(super) fn check_impls(&mut self, file: &ast::SourceFile) {
        for item in crate::adt::all_items(file) {
            let ast::Item::Impl(imp) = &item else {
                continue;
            };
            let (trait_name, Some(type_name)) = impl_parts(imp) else {
                continue;
            };
            if !self.adts.defs.contains_key(&type_name)
                || trait_name.as_deref().is_some_and(|t| {
                    !matches!(t, "Default" | "Display") && !self.adts.traits.contains_key(t)
                })
            {
                continue;
            }
            let saved = self.self_ty.replace(Ty::Adt(type_name.clone()));
            for item in imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                let ast::AssocItem::Fn(f) = item else {
                    continue;
                };
                let Some(name) = f.name() else { continue };
                let Some(m) = self
                    .adts
                    .method(&type_name, name.text().to_string().as_str())
                    .cloned()
                else {
                    continue;
                };
                let self_local = f
                    .param_list()
                    .and_then(|pl| pl.self_param())
                    .and_then(|sp| {
                        let n = sp.name()?;
                        let local = Local {
                            ty: Ty::Adt(type_name.clone()),
                            mutable: sp.amp_token().is_none() && sp.mut_token().is_some(),
                            deferred: false,
                            reference: sp.amp_token().map(|_| sp.mut_token().is_some()),
                            decl: None,
                        };
                        Some((n, local))
                    });
                self.check_body(&f, m.params.clone(), m.ret.clone(), m.is_async, self_local);
            }
            self.self_ty = saved;
        }
    }

    // ------------------------------------------------------------ paths

    pub(super) fn resolve(&self, segs: &[String]) -> Option<Resolved> {
        let self_ty = match &self.self_ty {
            Some(Ty::Adt(n)) => Some(n.as_str()),
            _ => None,
        };
        self.adts.resolve(self_ty, segs)
    }

    pub(super) fn ctor_name(&self, adt: &str, v: usize) -> String {
        self.adts.ctor_name(adt, v)
    }

    /// A path expression naming an ADT item: a unit struct or variant (a value), a tuple
    /// constructor or associated function (a function value). None when it isn't one.
    pub(super) fn adt_path(&mut self, p: &ast::PathExpr, segs: &[String]) -> Option<Ty> {
        match self.resolve(segs)? {
            Resolved::Ctor(adt, v) => {
                let variant = self.adts.defs[&adt].variants[v].clone();
                Some(match variant.shape {
                    Shape::Unit => Ty::Adt(adt),
                    Shape::Tuple => Ty::Closure {
                        params: variant.fields.iter().map(|f| f.ty.clone()).collect(),
                        ret: Box::new(Ty::Adt(adt)),
                    },
                    Shape::Named => {
                        let name = self.ctor_name(&adt, v);
                        self.error(
                            p.syntax(),
                            format!(
                                "expected value, found struct `{name}` (use `{name} {{ .. }}`)"
                            ),
                        );
                        Ty::Unknown
                    }
                })
            }
            Resolved::Assoc(adt, m) => match self.adts.method(&adt, &m).cloned() {
                Some(method) => {
                    let mut params = method.params.clone();
                    if method.receiver.is_some() {
                        params.insert(0, Ty::Adt(adt));
                    }
                    let ret = if method.is_async {
                        Ty::Future(Box::new(method.ret))
                    } else {
                        method.ret
                    };
                    Some(Ty::Closure {
                        params,
                        ret: Box::new(ret),
                    })
                }
                None => {
                    self.no_assoc(p.syntax(), &adt, &m);
                    Some(Ty::Unknown)
                }
            },
        }
    }

    fn no_assoc(&mut self, node: &SyntaxNode, adt: &str, m: &str) {
        let kind = if self.adts.defs[adt].is_enum {
            "enum"
        } else {
            "struct"
        };
        let what = if self.adts.defs[adt].is_enum {
            "variant or associated item"
        } else {
            "function or associated item"
        };
        self.error(
            node,
            format!("no {what} named `{m}` found for {kind} `{adt}` in the current scope"),
        );
    }

    /// `Point(1, 2)`, `Shape::Circle(r)`, `Point::new(..)`, `Point::len(&p)`.
    pub(super) fn adt_call(
        &mut self,
        c: &ast::CallExpr,
        segs: &[String],
        args: &[ast::Expr],
    ) -> Option<Ty> {
        match self.resolve(segs)? {
            Resolved::Ctor(adt, v) => {
                let variant = self.adts.defs[&adt].variants[v].clone();
                let name = self.ctor_name(&adt, v);
                if variant.shape != Shape::Tuple {
                    let kind = if variant.shape == Shape::Named {
                        "struct"
                    } else {
                        "unit"
                    };
                    self.error(
                        c.syntax(),
                        format!("expected function, found {kind} variant `{name}`"),
                    );
                    for a in args {
                        self.infer(a, None);
                    }
                    return Some(Ty::Adt(adt));
                }
                self.arg_count(
                    c.syntax(),
                    "tuple struct or variant",
                    variant.fields.len(),
                    args.len(),
                );
                for (a, f) in args.iter().zip(&variant.fields) {
                    self.check(a, &f.ty);
                }
                Some(Ty::Adt(adt))
            }
            Resolved::Assoc(adt, m) => {
                let Some(method) = self.adts.method(&adt, &m).cloned() else {
                    self.no_assoc(c.syntax(), &adt, &m);
                    for a in args {
                        self.infer(a, None);
                    }
                    return Some(Ty::Unknown);
                };
                let mut params = method.params.clone();
                if method.receiver.is_some() {
                    params.insert(0, Ty::Adt(adt.clone()));
                    if method.receiver == Some(Receiver::MutRef)
                        && let Some(first) = args.first()
                    {
                        self.check_mutable(first, Mutation::Borrow);
                    }
                }
                self.arg_count(c.syntax(), "function", params.len(), args.len());
                for (a, p) in args.iter().zip(&params) {
                    self.check(a, p);
                }
                Some(if method.is_async {
                    Ty::Future(Box::new(method.ret))
                } else {
                    method.ret
                })
            }
        }
    }

    /// `Point { x: 1, y }`, `Shape::Rect { w, ..other }`.
    pub(super) fn record_expr(&mut self, r: &ast::RecordExpr) -> Ty {
        let segs = path_segs(r.path());
        let list = r.record_expr_field_list();
        let fields: Vec<ast::RecordExprField> = list.iter().flat_map(|l| l.fields()).collect();
        let Some(Resolved::Ctor(adt, v)) = self.resolve(&segs) else {
            let text = r
                .path()
                .map(|p| p.syntax().text().to_string())
                .unwrap_or_default();
            self.error(
                r.syntax(),
                format!("cannot find struct, variant or union type `{text}` in this scope"),
            );
            for f in &fields {
                if let Some(e) = f.expr() {
                    self.infer(&e, None);
                }
            }
            return Ty::Unknown;
        };
        let variant = self.adts.defs[&adt].variants[v].clone();
        let name = self.ctor_name(&adt, v);
        let kind = if self.adts.defs[&adt].is_enum {
            "variant"
        } else {
            "struct"
        };
        let mut seen: Vec<String> = Vec::new();
        for f in &fields {
            let Some(fname) = f.field_name() else {
                continue;
            };
            let key = fname.text().to_string();
            let value = f.expr();
            match variant.field(&key) {
                Some((_, field)) => {
                    if seen.contains(&key) {
                        self.error(
                            fname.syntax(),
                            format!("field `{key}` specified more than once"),
                        );
                    }
                    seen.push(key);
                    if let Some(e) = value {
                        self.check(&e, &field.ty);
                    } else if let Some(local) =
                        self.lookup(fname.text().to_string().as_str()).cloned()
                    {
                        // Shorthand `Point { x }` reads the local `x`.
                        if !self.unify(&local.ty, &field.ty) {
                            let msg = format!(
                                "mismatched types: expected `{}`, found `{}`",
                                self.describe(&field.ty),
                                self.describe(&local.ty)
                            );
                            self.error(f.syntax(), msg);
                        }
                    } else {
                        self.error(
                            f.syntax(),
                            format!("cannot find value `{}` in this scope", fname.text()),
                        );
                    }
                }
                None => {
                    self.error(
                        fname.syntax(),
                        format!("{kind} `{name}` has no field named `{key}`"),
                    );
                    if let Some(e) = value {
                        self.infer(&e, None);
                    }
                }
            }
        }
        let spread = list.as_ref().and_then(|l| l.spread());
        if let Some(base) = &spread {
            self.check(base, &Ty::Adt(adt.clone()));
        } else {
            let missing: Vec<String> = variant
                .fields
                .iter()
                .filter(|f| !seen.contains(&f.name))
                .map(|f| format!("`{}`", f.name))
                .collect();
            if !missing.is_empty() {
                let list = join_and(&missing);
                let s = if missing.len() == 1 { "" } else { "s" };
                self.error(
                    r.syntax(),
                    format!("missing field{s} {list} in initializer of `{name}`"),
                );
            }
        }
        Ty::Adt(adt)
    }

    // ------------------------------------------------------------ fields & methods

    pub(super) fn adt_field(&mut self, f: &ast::FieldExpr, adt: &str) -> Ty {
        let Some(name) = f.name_ref() else {
            return Ty::Unknown;
        };
        let key = name.text().to_string();
        let def = &self.adts.defs[adt];
        if def.is_enum {
            let msg =
                format!("no field `{key}` on type `{adt}` (enum fields are reached with `match`)");
            self.error(f.syntax(), msg);
            return Ty::Unknown;
        }
        if let Some((_, field)) = def.variants[0].field(&key) {
            return field.ty.clone();
        }
        let msg = if self.adts.method(adt, &key).is_some() {
            format!(
                "attempted to take value of method `{key}` on type `{adt}` (call it: `{key}(..)`)"
            )
        } else {
            format!("no field `{key}` on type `{adt}`")
        };
        self.error(f.syntax(), msg);
        Ty::Unknown
    }

    pub(super) fn adt_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        adt: &str,
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        if let Some(sig) = self.adts.method(adt, method).cloned()
            && sig.receiver.is_some()
        {
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
            for a in args.iter().skip(sig.params.len()) {
                self.infer(a, None);
            }
            return if sig.is_async {
                Ty::Future(Box::new(sig.ret))
            } else {
                sig.ret
            };
        }
        let t = Ty::Adt(adt.to_string());
        let derived = |c: &mut Self, tr: &str, params: usize, ret: Ty| {
            if !c.implements(&t, tr) {
                c.error(
                    m.syntax(),
                    format!("the trait bound `{adt}: {tr}` is not satisfied (add `#[derive({tr})]` to `{adt}`)"),
                );
            }
            c.arg_count(m.syntax(), "method", params, args.len());
            for a in args.iter().take(params) {
                c.check(a, &t);
            }
            ret
        };
        match method {
            // `Rc<T>` is erased, so `rc.clone()` on a type without `Clone` is an `Rc` clone.
            "clone" if !self.implements(&t, "Clone") => {
                self.arg_count(m.syntax(), "method", 0, args.len());
                t.clone()
            }
            "clone" | "to_owned" => derived(self, "Clone", 0, t.clone()),
            "eq" | "ne" => derived(self, "PartialEq", 1, Ty::Bool),
            "cmp" => derived(self, "Ord", 1, Ty::Ordering),
            "partial_cmp" => derived(self, "PartialOrd", 1, Ty::Option(Box::new(Ty::Ordering))),
            "max" | "min" => derived(self, "Ord", 1, t.clone()),
            "lt" | "le" | "gt" | "ge" => derived(self, "PartialOrd", 1, Ty::Bool),
            "to_string" if self.adts.defs[adt].display => {
                self.arg_count(m.syntax(), "method", 0, args.len());
                Ty::Str
            }
            _ => {
                for a in args {
                    self.infer(a, None);
                }
                let kind = if self.adts.defs[adt].is_enum {
                    "enum"
                } else {
                    "struct"
                };
                let hint = if self.adts.method(adt, method).is_some() {
                    format!(" (`{method}` is an associated function: call `{adt}::{method}(..)`)")
                } else {
                    String::new()
                };
                self.error(
                    m.syntax(),
                    format!("no method named `{method}` found for {kind} `{adt}` in the current scope{hint}"),
                );
                Ty::Unknown
            }
        }
    }

    // ------------------------------------------------------------ patterns

    /// Whether a bare identifier pattern names a unit struct (`Marker`), which Rust matches
    /// rather than binds.
    pub(super) fn unit_struct_pat(&self, ip: &ast::IdentPat) -> Option<String> {
        if ip.pat().is_some() || ip.mut_token().is_some() || ip.ref_token().is_some() {
            return None;
        }
        let n = ip.name()?.text().to_string();
        match self.resolve(std::slice::from_ref(&n))? {
            Resolved::Ctor(adt, 0) if self.adts.defs[&adt].variants[0].shape == Shape::Unit => {
                Some(adt)
            }
            _ => None,
        }
    }

    /// Struct, tuple-struct and path patterns naming ADTs. Returns false when `p` isn't one.
    pub(super) fn adt_pat(&mut self, p: &ast::Pat, expected: &Ty) -> bool {
        let segs = match p {
            ast::Pat::RecordPat(rp) => path_segs(rp.path()),
            ast::Pat::TupleStructPat(tp) => path_segs(tp.path()),
            ast::Pat::PathPat(pp) => path_segs(pp.path()),
            ast::Pat::IdentPat(ip) => match self.unit_struct_pat(ip) {
                Some(n) => vec![n],
                None => return false,
            },
            _ => return false,
        };
        let Some(Resolved::Ctor(adt, v)) = self.resolve(&segs) else {
            let names_adt = segs.len() > 1 && self.resolve(&segs[..1]).is_some()
                || segs.first().is_some_and(|s| s == "Self");
            if names_adt || matches!(p, ast::Pat::RecordPat(_)) {
                let text = segs.join("::");
                self.error(
                    p.syntax(),
                    format!("cannot find tuple struct, struct or variant `{text}` in this scope"),
                );
                return true;
            }
            return false;
        };
        if !self.unify(expected, &Ty::Adt(adt.clone())) {
            let msg = format!(
                "mismatched types: expected `{}`, found `{adt}`",
                self.describe(expected)
            );
            self.error(p.syntax(), msg);
        }
        let variant = self.adts.defs[&adt].variants[v].clone();
        let name = self.ctor_name(&adt, v);
        match p {
            ast::Pat::TupleStructPat(tp) => {
                if variant.shape != Shape::Tuple {
                    self.error(
                        p.syntax(),
                        format!("expected tuple struct or tuple variant, found `{name}`"),
                    );
                    return true;
                }
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let rest = fields
                    .iter()
                    .position(|f| matches!(f, ast::Pat::RestPat(_)));
                let n = variant.fields.len();
                let named = fields.len() - usize::from(rest.is_some());
                if (rest.is_none() && named != n) || named > n {
                    let s = |k: usize| if k == 1 { "" } else { "s" };
                    self.error(
                        p.syntax(),
                        format!(
                            "this pattern has {named} field{}, but `{name}` has {n} field{}",
                            s(named),
                            s(n)
                        ),
                    );
                    return true;
                }
                for (i, f) in fields.iter().enumerate() {
                    let index = match rest {
                        Some(r) if i == r => continue,
                        Some(r) if i > r => n - (fields.len() - i),
                        _ => i,
                    };
                    self.pat(f, &variant.fields[index].ty);
                }
            }
            ast::Pat::RecordPat(rp) => {
                let Some(list) = rp.record_pat_field_list() else {
                    return true;
                };
                let mut seen = Vec::new();
                for f in list.fields() {
                    let Some(fname) = f.field_name() else {
                        continue;
                    };
                    let key = fname.text().to_string();
                    match variant.field(&key) {
                        Some((_, field)) => {
                            seen.push(key);
                            if let Some(sub) = f.pat() {
                                self.pat(&sub, &field.ty);
                            }
                        }
                        None => {
                            self.error(
                                fname.syntax(),
                                format!("`{name}` does not have a field named `{key}`"),
                            );
                        }
                    }
                }
                if list.rest_pat().is_none() {
                    let missing: Vec<String> = variant
                        .fields
                        .iter()
                        .filter(|f| !seen.contains(&f.name))
                        .map(|f| format!("`{}`", f.name))
                        .collect();
                    if !missing.is_empty() {
                        let s = if missing.len() == 1 { "" } else { "s" };
                        self.error(
                            p.syntax(),
                            format!("pattern does not mention field{s} {} (add `..` to ignore the rest)", join_and(&missing)),
                        );
                    }
                }
            }
            _ => {
                if variant.shape != Shape::Unit {
                    let kind = if variant.shape == Shape::Tuple {
                        "tuple"
                    } else {
                        "struct"
                    };
                    self.error(
                        p.syntax(),
                        format!("expected unit struct, unit variant or constant, found {kind} variant `{name}`"),
                    );
                }
            }
        }
        true
    }

    /// Declare every `const` / `static` item, then check their initializers.
    pub(super) fn check_consts(&mut self, file: &ast::SourceFile) {
        let mut bodies = Vec::new();
        for item in crate::adt::all_items(file) {
            let (name, ty, body, node) = match &item {
                ast::Item::Const(k) => (k.name(), k.ty(), k.body(), k.syntax().clone()),
                ast::Item::Static(s) => {
                    if s.mut_token().is_some() {
                        self.error(
                            s.syntax(),
                            "`static mut` is not supported yet (keep mutable state in a `let mut` and pass it around)",
                        );
                    }
                    (s.name(), s.ty(), s.body(), s.syntax().clone())
                }
                _ => continue,
            };
            let Some(name) = name else { continue };
            let ty = match ty {
                Some(t) => self.lower_type(&t),
                None => {
                    self.error(
                        &node,
                        format!("missing type for `const` item `{}`", name.text()),
                    );
                    Ty::Unknown
                }
            };
            self.consts.insert(name.text().to_string(), ty.clone());
            bodies.push((body, ty));
        }
        for (body, ty) in bodies {
            let Some(e) = body else { continue };
            self.scopes = vec![std::collections::HashMap::new()];
            self.ret_ty = Ty::Unit;
            self.in_async = false;
            self.check(&e, &ty);
            self.resolve_collects();
        }
    }

    /// A bare identifier pattern naming a `const`: compared against, not bound.
    pub(super) fn const_pat(&self, ip: &ast::IdentPat) -> Option<Ty> {
        if ip.pat().is_some() || ip.mut_token().is_some() || ip.ref_token().is_some() {
            return None;
        }
        self.consts
            .get(ip.name()?.text().to_string().as_str())
            .cloned()
    }

    /// Patterns that match every value: also struct patterns (but not enum variants) whose
    /// fields are irrefutable, and unit structs.
    pub(super) fn irrefutable_pat(&self, p: &ast::Pat) -> bool {
        let p = super::strip_pat(p);
        let segs = match &p {
            ast::Pat::RecordPat(rp) => path_segs(rp.path()),
            ast::Pat::TupleStructPat(tp) => path_segs(tp.path()),
            ast::Pat::IdentPat(ip) if self.unit_struct_pat(ip).is_some() => return true,
            ast::Pat::IdentPat(ip) if self.const_pat(ip).is_some() => return false,
            ast::Pat::IdentPat(ip) => {
                return ip.pat().is_none_or(|sub| self.irrefutable_pat(&sub))
                    && super::irrefutable(&ast::Pat::IdentPat(ip.clone()));
            }
            ast::Pat::TuplePat(tp) => return tp.fields().all(|f| self.irrefutable_pat(&f)),
            other => return super::irrefutable(other),
        };
        let Some(Resolved::Ctor(adt, _)) = self.resolve(&segs) else {
            return false;
        };
        if self.adts.defs[&adt].variants.len() != 1 {
            return false;
        }
        match &p {
            ast::Pat::RecordPat(rp) => rp
                .record_pat_field_list()
                .into_iter()
                .flat_map(|l| l.fields())
                .all(|f| f.pat().is_none_or(|sub| self.irrefutable_pat(&sub))),
            ast::Pat::TupleStructPat(tp) => tp.fields().all(|f| self.irrefutable_pat(&f)),
            _ => true,
        }
    }

    /// Exhaustiveness: a struct/variant pattern as `Ap::Ctor` over all of its fields, one entry
    /// per combination of alternatives.
    pub(super) fn adt_abstract(&self, p: &ast::Pat) -> Option<Vec<Ap>> {
        let segs = match p {
            ast::Pat::RecordPat(rp) => path_segs(rp.path()),
            ast::Pat::TupleStructPat(tp) => path_segs(tp.path()),
            ast::Pat::PathPat(pp) => path_segs(pp.path()),
            ast::Pat::IdentPat(ip) => vec![self.unit_struct_pat(ip)?],
            _ => return None,
        };
        let Resolved::Ctor(adt, v) = self.resolve(&segs)? else {
            return Some(vec![Ap::Other]);
        };
        let variant = &self.adts.defs[&adt].variants[v];
        let n = variant.fields.len();
        let mut cols: Vec<Vec<Ap>> = vec![vec![Ap::Wild]; n];
        match p {
            ast::Pat::TupleStructPat(tp) => {
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let rest = fields
                    .iter()
                    .position(|f| matches!(f, ast::Pat::RestPat(_)));
                for (i, f) in fields.iter().enumerate() {
                    let index = match rest {
                        Some(r) if i == r => continue,
                        Some(r) if i > r => n.checked_sub(fields.len() - i)?,
                        _ => i,
                    };
                    if index < n {
                        cols[index] = self.abstract_pat(f);
                    }
                }
            }
            ast::Pat::RecordPat(rp) => {
                for f in rp
                    .record_pat_field_list()
                    .into_iter()
                    .flat_map(|l| l.fields())
                {
                    let (Some(fname), Some(sub)) = (f.field_name(), f.pat()) else {
                        continue;
                    };
                    if let Some((i, _)) = variant.field(fname.text().to_string().as_str()) {
                        cols[i] = self.abstract_pat(&sub);
                    }
                }
            }
            _ => {}
        }
        let mut product: Vec<Vec<Ap>> = vec![vec![]];
        for col in cols {
            product = product
                .into_iter()
                .flat_map(|prefix| {
                    col.iter()
                        .map(move |c| [prefix.clone(), vec![c.clone()]].concat())
                })
                .collect();
            if product.len() > 512 {
                return Some(vec![Ap::Ctor(v, vec![Ap::Wild; n])]);
            }
        }
        Some(
            product
                .into_iter()
                .map(|fields| Ap::Ctor(v, fields))
                .collect(),
        )
    }

    /// The witness text for a constructor with rendered fields: `Point { x: true, .. }`,
    /// `Shape::Circle(_)`, `Marker`.
    pub(super) fn render_ctor(&self, adt: &str, v: usize, fields: &[String]) -> String {
        let variant = &self.adts.defs[adt].variants[v];
        let name = self.ctor_name(adt, v);
        match variant.shape {
            Shape::Unit => name,
            Shape::Tuple => format!("{name}({})", fields.join(", ")),
            Shape::Named => {
                let shown: Vec<String> = variant
                    .fields
                    .iter()
                    .zip(fields)
                    .filter(|(_, w)| *w != "_")
                    .map(|(f, w)| format!("{}: {w}", f.name))
                    .collect();
                match (shown.is_empty(), shown.len() == fields.len()) {
                    (true, _) => format!("{name} {{ .. }}"),
                    (false, true) => format!("{name} {{ {} }}", shown.join(", ")),
                    (false, false) => format!("{name} {{ {}, .. }}", shown.join(", ")),
                }
            }
        }
    }

    // ------------------------------------------------------------ traits used by operators

    /// `a == b` / `a < b` on ADTs need `PartialEq` / `PartialOrd`.
    pub(super) fn require_trait(&mut self, node: &SyntaxNode, t: &Ty, tr: &str) {
        if let Ty::Param(_) = self.shallow(t) {
            let what = if tr == "PartialOrd" {
                "comparing with `<`"
            } else {
                "comparing with `==`"
            };
            self.require_param_bound(node, t, tr, what);
            return;
        }
        if let Ty::Adt(n) = self.shallow(t)
            && !self.implements(t, tr)
        {
            self.error(
                node,
                format!(
                    "the trait bound `{n}: {tr}` is not satisfied (add `#[derive({tr})]` to `{n}`)"
                ),
            );
        }
    }

    /// Sets and map keys of a type without `#[derive(Hash, Eq)]`.
    pub(super) fn unhashable_adt(&self, t: &Ty) -> Option<String> {
        let mut found = None;
        t.walk(&mut |x| {
            if let Ty::Set(k) | Ty::Map(k, _) = x
                && found.is_none()
                && let Some(n) = first_adt(k)
                && !(self.implements(&Ty::Adt(n.clone()), "Hash")
                    && self.implements(&Ty::Adt(n.clone()), "Eq"))
            {
                found = Some(n);
            }
        });
        found
    }
}

fn first_adt(t: &Ty) -> Option<String> {
    let mut found = None;
    t.walk(&mut |x| {
        if let Ty::Adt(n) = x
            && found.is_none()
        {
            found = Some(n.clone());
        }
    });
    found
}

/// The segment names of a path (`Shape::Circle` → `["Shape", "Circle"]`).
pub(super) fn path_segs(path: Option<ast::Path>) -> Vec<String> {
    path.map(|p| {
        p.segments()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect()
    })
    .unwrap_or_default()
}

/// `a`, `a and b`, `a, b and c`.
fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}
