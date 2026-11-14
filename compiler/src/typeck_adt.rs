//! Type checking for structs, enums and `impl` blocks. A child module of `typeck`, so it works
//! directly on the checker's state.

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasAttrs, HasGenericArgs, HasGenericParams, HasName, HasTypeBounds},
};

use std::collections::HashMap;

use super::generic::subst;
use super::{Ap, Checker, Local, Mutation, VarKind};
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
        // `use std::cmp::Reverse;`: a generic tuple struct whose class is the runtime's, with the
        // order reversed there.
        let uses_reverse = file
            .syntax()
            .descendants()
            .filter_map(ast::Use::cast)
            .any(|u| {
                let text: String = u.syntax().text().to_string().split_whitespace().collect();
                text.contains("cmp::") && text.contains("Reverse")
            })
            // or `std::cmp::Reverse(x)` written out
            || file.syntax().descendants().filter_map(ast::Path::cast).any(|p| {
                p.syntax().text().to_string().replace(' ', "").ends_with("cmp::Reverse")
            });
        if uses_reverse {
            self.adts.defs.insert(
                "Reverse".into(),
                AdtDef {
                    name: "Reverse".into(),
                    is_enum: false,
                    variants: vec![Variant {
                        name: "Reverse".into(),
                        shape: Shape::Tuple,
                        fields: vec![Field {
                            name: "0".into(),
                            ty: Ty::Param("T".into()),
                            js: "0".into(),
                        }],
                        discr: 0,
                    }],
                    derives: [
                        "Debug",
                        "Clone",
                        "Copy",
                        "PartialEq",
                        "Eq",
                        "PartialOrd",
                        "Ord",
                        "Hash",
                    ]
                    .map(String::from)
                    .to_vec(),
                    display: false,
                    default_impl: false,
                    default_variant: None,
                    module: Some("κσ".into()),
                    type_params: vec!["T".into()],
                    manual: vec![],
                },
            );
        }
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
                    type_params: type_param_names(&item),
                    manual: vec![],
                },
            );
        }
        for item in crate::adt::all_items(file) {
            match &item {
                ast::Item::Struct(s) => {
                    let Some(name) = s.name() else { continue };
                    self.reject_const_generics(s.generic_param_list());
                    self.enter_type_params(&type_param_names(&item));
                    let variant =
                        self.lower_variant(name.text().to_string().as_str(), s.field_list());
                    self.generics.clear();
                    if let Some(def) = self.adts.defs.get_mut(name.text().to_string().as_str())
                        && !def.is_enum
                    {
                        def.variants = vec![variant];
                    }
                }
                ast::Item::Enum(e) => {
                    let Some(name) = e.name() else { continue };
                    self.reject_const_generics(e.generic_param_list());
                    self.enter_type_params(&type_param_names(&item));
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
                    self.generics.clear();
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
        self.collect_traits(file);
        // `impl` signatures.
        for item in crate::adt::all_items(file) {
            if let ast::Item::Impl(imp) = &item {
                self.collect_impl(imp);
            }
        }
        // Derives need the field types' traits, hand-written impls included.
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
        // Hand-written comparison impls need their supertraits, as in rustc.
        let mut missing = Vec::new();
        for def in self.adts.defs.values() {
            for m in &def.manual {
                let needs: &[&str] = match m.as_str() {
                    "Ord" => &["Eq", "PartialOrd"],
                    "Error" => &["Debug", "Display"],
                    "PartialOrd" | "Eq" => &["PartialEq"],
                    _ => &[],
                };
                for n in needs {
                    if !(def.implements(n) || (*n == "Display" && def.display)) {
                        missing.push((def.name.clone(), m.clone(), n.to_string()));
                    }
                }
            }
        }
        for item in crate::adt::all_items(file) {
            if let ast::Item::Impl(imp) = &item
                && let (Some(tr), Some(ty)) = impl_parts(imp)
            {
                for (d, m, n) in missing.iter().filter(|(d, m, _)| *d == ty && *m == tr) {
                    let node = imp.syntax().clone();
                    self.error(
                        &node,
                        format!("the trait bound `{d}: {n}` is not satisfied (`{m}` requires it; derive or implement `{n}`)"),
                    );
                }
            }
        }
        // A field named like one of the type's methods gets another JS property name.
        for def in self.adts.defs.values_mut() {
            if def.module.is_some() {
                continue;
            }
            let Some(methods) = self.adts.methods.get(&def.name) else {
                continue;
            };
            for v in &mut def.variants {
                for f in &mut v.fields {
                    if v.shape == Shape::Named && methods.contains_key(&f.name) {
                        f.js = format!("{}$f", f.name);
                    }
                }
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
                    ret: Ty::Adt(n, vec![]),
                    is_async: false,
                    receiver: None,
                    cell_params: vec![],
                    impl_args: vec![],
                    type_params: vec![],
                });
        }
    }

    /// `T`, `U` as type parameters for lowering field and signature types.
    fn enter_type_params(&mut self, names: &[String]) {
        self.generics.clear();
        for n in names {
            self.generics.insert(n.clone(), Ty::Param(n.clone()));
        }
    }

    fn reject_const_generics(&mut self, gp: Option<ast::GenericParamList>) {
        if let Some(gp) = gp
            && gp
                .generic_params()
                .any(|p| matches!(p, ast::GenericParam::ConstParam(_)))
        {
            self.error(
                gp.syntax(),
                "const generic parameters are not supported yet (planned for phase 4)",
            );
        }
    }

    /// Type parameters to their arguments for `adt` (unknown when the arguments aren't known).
    pub(super) fn adt_subst(&self, adt: &str, args: &[Ty]) -> HashMap<String, Ty> {
        let Some(def) = self.adts.defs.get(adt) else {
            return HashMap::new();
        };
        def.type_params
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), args.get(i).cloned().unwrap_or(Ty::Unknown)))
            .collect()
    }

    /// Fresh type arguments for a new value of `adt`.
    pub(super) fn fresh_args(&mut self, adt: &str) -> Vec<Ty> {
        let n = self
            .adts
            .defs
            .get(adt)
            .map(|d| d.type_params.len())
            .unwrap_or(0);
        (0..n).map(|_| self.fresh(VarKind::General)).collect()
    }

    /// Variant `v` of `adt` with its field types instantiated for `args`.
    pub(super) fn variant_inst(&self, adt: &str, v: usize, args: &[Ty]) -> Variant {
        let m = self.adt_subst(adt, args);
        let mut variant = self.adts.defs[adt].variants[v].clone();
        for f in &mut variant.fields {
            f.ty = subst(&f.ty, &m);
        }
        variant
    }

    /// A method's signature for a receiver with type arguments `recv_args` (None for an
    /// associated function call, whose impl parameters get fresh variables).
    pub(super) fn method_inst(&mut self, m: &Method, recv_args: Option<&[Ty]>) -> Method {
        let mut map = HashMap::new();
        for p in &m.type_params {
            let v = self.fresh(VarKind::General);
            map.insert(p.clone(), v);
        }
        for (i, ia) in m.impl_args.iter().enumerate() {
            if let Ty::Param(u) = ia {
                let arg = match recv_args {
                    Some(args) => args.get(i).cloned().unwrap_or(Ty::Unknown),
                    None => self.fresh(VarKind::General),
                };
                map.insert(u.clone(), arg);
            }
        }
        if map.is_empty() {
            return m.clone();
        }
        Method {
            params: m.params.iter().map(|t| subst(t, &map)).collect(),
            ret: subst(&m.ret, &map),
            is_async: m.is_async,
            receiver: m.receiver,
            cell_params: m.cell_params.clone(),
            impl_args: m.impl_args.iter().map(|t| subst(t, &map)).collect(),
            type_params: m.type_params.clone(),
        }
    }

    /// Add a method's own type parameters (and bounds) to the impl's; returns their names.
    fn enter_method_generics(&mut self, f: &ast::Fn) -> Vec<String> {
        let mut own = Vec::new();
        if let Some(gp) = f.generic_param_list() {
            for p in gp.generic_params() {
                match p {
                    ast::GenericParam::TypeParam(tp) => {
                        let Some(n) = tp.name() else { continue };
                        let n = n.text().to_string();
                        let bounds: Vec<String> = tp
                            .type_bound_list()
                            .into_iter()
                            .flat_map(|l| l.bounds())
                            .filter_map(|b| match b.ty()? {
                                ast::Type::PathType(pt) => {
                                    Some(pt.path()?.segment()?.name_ref()?.text().to_string())
                                }
                                _ => None,
                            })
                            .collect();
                        // An `F: Fn(..)` parameter is a closure type.
                        let closure = tp.type_bound_list().and_then(|l| self.lower_fn_bounds(&l));
                        match closure {
                            Some(t) => {
                                self.generics.insert(n.clone(), t);
                            }
                            None => {
                                self.generics.insert(n.clone(), Ty::Param(n.clone()));
                                self.param_bounds.insert(n.clone(), bounds);
                                own.push(n);
                            }
                        }
                    }
                    ast::GenericParam::ConstParam(_) => {
                        self.error(
                            p.syntax(),
                            "const generic parameters are not supported yet (planned for phase 4)",
                        );
                    }
                    ast::GenericParam::LifetimeParam(_) => {}
                }
            }
        }
        own
    }

    /// The `impl`'s type parameters (with bounds) for lowering and checking its methods; returns
    /// the impl's self type (`Stack<U>`).
    fn enter_impl(&mut self, imp: &ast::Impl, type_name: &str) -> Ty {
        self.generics.clear();
        self.param_bounds.clear();
        if let Some(gp) = imp.generic_param_list() {
            for p in gp.generic_params() {
                if let ast::GenericParam::TypeParam(tp) = p
                    && let Some(n) = tp.name()
                {
                    let n = n.text().to_string();
                    let bounds: Vec<String> = tp
                        .type_bound_list()
                        .into_iter()
                        .flat_map(|l| l.bounds())
                        .filter_map(|b| match b.ty()? {
                            ast::Type::PathType(pt) => {
                                Some(pt.path()?.segment()?.name_ref()?.text().to_string())
                            }
                            _ => None,
                        })
                        .collect();
                    self.generics.insert(n.clone(), Ty::Param(n.clone()));
                    self.param_bounds.insert(n, bounds);
                }
            }
        }
        match imp.self_ty() {
            Some(t) => self.lower_type(&t),
            None => Ty::Adt(type_name.to_string(), vec![]),
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
                    .map(|f| {
                        let name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
                        Field {
                            js: name.clone(),
                            name,
                            ty: f.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
                        }
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
                        js: i.to_string(),
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
            // Derived impls require the trait of the type parameters (`impl<T: Clone> Clone for ..`),
            // so a field of type `T` is fine.
            let unknowns: HashMap<String, Ty> = def
                .type_params
                .iter()
                .map(|p| (p.clone(), Ty::Unknown))
                .collect();
            for v in &def.variants {
                for f in &v.fields {
                    if !self.implements(&subst(&f.ty, &unknowns), d) {
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
        // `None`, empty collections and `""` are defaults whatever the element type.
        if t == "Default"
            && matches!(
                ty,
                Ty::Option(_) | Ty::Vec(_) | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..) | Ty::Str
            )
        {
            return true;
        }
        match &ty {
            Ty::Adt(n, _) => match self.adts.defs.get(n) {
                Some(def) if t == "Default" => def.has_default(),
                Some(def) => def.implements(t),
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
            // A heap can't be compared; the others compare like their elements.
            Ty::Coll(k, _) => {
                t != "Copy"
                    && !(*k == crate::ty::Coll::BinaryHeap
                        && matches!(t, "PartialEq" | "Eq" | "Hash" | "PartialOrd" | "Ord"))
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
        let Some(mut type_name) = type_name.or_else(|| crate::adt::impl_self_name(imp)) else {
            return;
        };
        let node = imp
            .self_ty()
            .map(|t| t.syntax().clone())
            .unwrap_or_else(|| imp.syntax().clone());
        // `impl Money for f64`: a user trait for a built-in type.
        if !self.adts.defs.contains_key(&type_name)
            && let Some(key) = crate::adt::ext_key(&type_name)
            && trait_name
                .as_deref()
                .is_some_and(|t| self.adts.traits.contains_key(t))
        {
            type_name = key.to_string();
            self.adts.ext_types.insert(type_name.clone());
        }
        if !self.adts.defs.contains_key(&type_name) && !self.adts.ext_types.contains(&type_name) {
            self.error(&node, format!("cannot find type `{type_name}` in this scope (impls are for structs and enums in this file)"));
            return;
        }
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
            Some("From") => {}
            // Hand-written comparisons and clones: codegen bridges `$cmp` / `$eq` / `$clone` to them.
            Some(t @ ("Ord" | "PartialOrd" | "PartialEq" | "Eq" | "Clone" | "Debug" | "Error")) => {
                if let Some(d) = self.adts.defs.get_mut(&type_name) {
                    d.manual.push(t.to_string());
                }
            }
            Some(t) if crate::adt::op_method(t).is_some() => {}
            Some("Iterator") => {}
            Some(t) if self.adts.traits.contains_key(t) => {
                self.register_trait_impl(imp, t, &type_name);
            }
            Some(t) => {
                let node = imp.trait_().map(|t| t.syntax().clone()).unwrap_or(node);
                self.error(
                    &node,
                    format!("`impl {t} for ..` is not supported yet (of the std traits, `Display`, `Default`, `From`, `Ord`, `PartialOrd`, `PartialEq`, `Eq`, `Clone`, `Debug` and the `std::ops` operators can be implemented; derive the others; user traits declared in the file work)"),
                );
                return;
            }
        }
        let self_ty = self.enter_impl(imp, &type_name);
        let impl_args = match &self_ty {
            Ty::Adt(_, args) => args.clone(),
            _ => vec![],
        };
        let saved = self.self_ty.replace(self_ty);
        // `impl From<X> for Y`: the source type.
        let from_src = (trait_name.as_deref() == Some("From")).then(|| {
            let arg = match imp.trait_() {
                Some(ast::Type::PathType(p)) => p
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.generic_arg_list())
                    .and_then(|l| l.generic_args().next())
                    .and_then(|g| match g {
                        ast::GenericArg::TypeArg(ta) => ta.ty(),
                        _ => None,
                    }),
                _ => None,
            };
            arg.map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown)
        });
        // `impl Add<R> for T`: the right operand's type (`Self` by default), and the impl's
        // `type Output = ..;`, which `-> Self::Output` names.
        let op = trait_name.as_deref().and_then(crate::adt::op_method);
        let op_rhs = op.map(|_| {
            let arg = match imp.trait_() {
                Some(ast::Type::PathType(p)) => p
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.generic_arg_list())
                    .and_then(|l| l.generic_args().next())
                    .and_then(|g| match g {
                        ast::GenericArg::TypeArg(ta) => ta.ty(),
                        _ => None,
                    }),
                _ => None,
            };
            match arg {
                Some(t) => self.lower_type(&t),
                None => self.self_ty.clone().unwrap_or(Ty::Unknown),
            }
        });
        let is_iter = trait_name.as_deref() == Some("Iterator");
        let output = imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
            .find_map(|i| match i {
                ast::AssocItem::TypeAlias(ta) if op.is_some() || is_iter => ta.ty(),
                _ => None,
            })
            .map(|t| self.lower_type(&t));
        // `type Item = X;`: values of the type iterate over `X`.
        if is_iter {
            let item = output.clone().unwrap_or(Ty::Unknown);
            self.adts.iter_items.insert(type_name.clone(), item);
        }
        let saved_output = std::mem::replace(&mut self.assoc_output, output);
        for item in imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
        {
            let ast::AssocItem::Fn(f) = item else {
                if (op.is_some() || is_iter) && matches!(item, ast::AssocItem::TypeAlias(_)) {
                    continue;
                }
                // `const W: usize = 3;`
                if let ast::AssocItem::Const(k) = &item
                    && let Some(n) = k.name()
                {
                    let ty = k.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                    let consts = self.adts.assoc_consts.entry(type_name.clone()).or_default();
                    if consts.insert(n.text().to_string(), ty).is_some() {
                        self.error(
                            n.syntax(),
                            format!("duplicate definitions with name `{}`", n.text()),
                        );
                    }
                    continue;
                }
                self.error(
                    item.syntax(),
                    "only functions are supported in `impl` blocks",
                );
                continue;
            };
            let Some(name) = f.name() else { continue };
            let mut method = self.lower_method(&f);
            method.impl_args = impl_args.clone();
            if let (Some(op), Some(rhs)) = (op, &op_rhs)
                && name.text() == op
            {
                let at = self.range(name.syntax()).0;
                let impls = self.adts.op_impls.entry(type_name.clone()).or_default();
                let key = format!("{op}${}", impls.len());
                impls.push(crate::adt::OpImpl {
                    trait_name: trait_name.clone().unwrap_or_default(),
                    rhs: rhs.clone(),
                    key: key.clone(),
                    at,
                });
                self.adts
                    .methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert(key, method);
                continue;
            }
            // `impl fmt::Debug`: its `fmt` is `fmt$dbg`, so a `Display` impl can have `fmt` too.
            if trait_name.as_deref() == Some("Debug") && name.text() == "fmt" {
                let at = self.range(name.syntax()).0;
                self.adts.debug_fmt.insert(type_name.clone(), at);
                self.adts
                    .methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert("fmt$dbg".into(), method);
                continue;
            }
            if let Some(src) = &from_src
                && name.text() == "from"
            {
                let at = self.range(name.syntax()).0;
                let impls = self.adts.from_impls.entry(type_name.clone()).or_default();
                let key = format!("from${}", impls.len());
                impls.push(crate::adt::FromImpl {
                    src: src.clone(),
                    key: key.clone(),
                    at,
                });
                self.adts
                    .methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert(key, method);
                continue;
            }
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
        self.assoc_output = saved_output;
        self.generics.clear();
        self.param_bounds.clear();
    }

    /// A method's signature (without `self`), with `Self` lowered per `self.self_ty`.
    pub(super) fn lower_method(&mut self, f: &ast::Fn) -> Method {
        // The method's own type parameters join the impl's while its signature is lowered.
        let own = self.enter_method_generics(f);
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
        let method = Method {
            params,
            ret,
            is_async: f.async_token().is_some(),
            receiver,
            cell_params,
            impl_args: vec![],
            type_params: own.clone(),
        };
        for p in &own {
            self.generics.remove(p);
            self.param_bounds.remove(p);
        }
        method
    }

    /// Check the bodies of every function in `impl` blocks.
    pub(super) fn check_impls(&mut self, file: &ast::SourceFile) {
        for item in crate::adt::all_items(file) {
            let ast::Item::Impl(imp) = &item else {
                continue;
            };
            let (trait_name, type_name) = impl_parts(imp);
            let Some(mut type_name) = type_name.or_else(|| crate::adt::impl_self_name(imp)) else {
                continue;
            };
            if let Some(key) = crate::adt::ext_key(&type_name)
                && !self.adts.defs.contains_key(&type_name)
                && self.adts.ext_types.contains(key)
            {
                type_name = key.to_string();
            }
            if !(self.adts.defs.contains_key(&type_name)
                || self.adts.ext_types.contains(&type_name))
                || trait_name.as_deref().is_some_and(|t| {
                    !matches!(
                        t,
                        "Default"
                            | "Display"
                            | "From"
                            | "Ord"
                            | "PartialOrd"
                            | "PartialEq"
                            | "Eq"
                            | "Clone"
                            | "Debug"
                            | "Error"
                            | "Iterator"
                    ) && crate::adt::op_method(t).is_none()
                        && !self.adts.traits.contains_key(t)
                })
            {
                continue;
            }
            let self_ty = self.enter_impl(imp, &type_name);
            let saved = self.self_ty.replace(self_ty.clone());
            for item in imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
            {
                let ast::AssocItem::Fn(f) = item else {
                    // An associated constant's value.
                    if let ast::AssocItem::Const(k) = &item
                        && let (Some(n), Some(body)) = (k.name(), k.body())
                    {
                        let ty = self
                            .adts
                            .assoc_consts
                            .get(&type_name)
                            .and_then(|c| c.get(n.text()))
                            .cloned()
                            .unwrap_or(Ty::Unknown);
                        self.scopes = vec![std::collections::HashMap::new()];
                        self.ret_ty = Ty::Unit;
                        self.in_async = false;
                        self.check(&body, &ty);
                    }
                    continue;
                };
                let Some(name) = f.name() else { continue };
                let key = self.adts.method_key(
                    &type_name,
                    name.text().to_string().as_str(),
                    self.range(name.syntax()).0,
                );
                let Some(m) = self.adts.method(&type_name, &key).cloned() else {
                    continue;
                };
                let self_local = f
                    .param_list()
                    .and_then(|pl| pl.self_param())
                    .and_then(|sp| {
                        let n = sp.name()?;
                        let local = Local {
                            ty: self_ty.clone(),
                            mutable: sp.amp_token().is_none() && sp.mut_token().is_some(),
                            deferred: false,
                            reference: sp.amp_token().map(|_| sp.mut_token().is_some()),
                            decl: None,
                        };
                        Some((n, local))
                    });
                let own = self.enter_method_generics(&f);
                self.check_body(&f, m.params.clone(), m.ret.clone(), m.is_async, self_local);
                for p in own {
                    self.generics.remove(&p);
                    self.param_bounds.remove(&p);
                }
            }
            self.self_ty = saved;
            self.generics.clear();
            self.param_bounds.clear();
        }
    }

    // ------------------------------------------------------------ paths

    pub(super) fn resolve(&self, segs: &[String]) -> Option<Resolved> {
        let self_ty = match &self.self_ty {
            Some(Ty::Adt(n, _)) => Some(n.as_str()),
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
                let args = self.fresh_args(&adt);
                let variant = self.variant_inst(&adt, v, &args);
                Some(match variant.shape {
                    Shape::Unit => Ty::Adt(adt, args),
                    Shape::Tuple => Ty::Closure {
                        params: variant.fields.iter().map(|f| f.ty.clone()).collect(),
                        ret: Box::new(Ty::Adt(adt, args)),
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
            Resolved::Assoc(adt, m)
                if self.adts.method(&adt, &m).is_none()
                    && self
                        .adts
                        .assoc_consts
                        .get(&adt)
                        .is_some_and(|c| c.contains_key(&m)) =>
            {
                Some(self.adts.assoc_consts[&adt][&m].clone())
            }
            Resolved::Assoc(adt, m) => match self.adts.method(&adt, &m).cloned() {
                Some(method) => {
                    let method = self.method_inst(&method, None);
                    let mut params = method.params.clone();
                    if method.receiver.is_some() {
                        params.insert(0, Ty::Adt(adt, method.impl_args.clone()));
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
                let targs = self.fresh_args(&adt);
                let variant = self.variant_inst(&adt, v, &targs);
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
                    return Some(Ty::Adt(adt, targs));
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
                Some(Ty::Adt(adt, targs))
            }
            Resolved::Assoc(adt, m) => {
                if m == "from"
                    && self.adts.method(&adt, &m).is_none()
                    && self.adts.from_impls.contains_key(&adt)
                {
                    return Some(self.conv_from_call(c, &adt, args));
                }
                let Some(method) = self.adts.method(&adt, &m).cloned() else {
                    self.no_assoc(c.syntax(), &adt, &m);
                    for a in args {
                        self.infer(a, None);
                    }
                    return Some(Ty::Unknown);
                };
                let method = self.method_inst(&method, None);
                let mut params = method.params.clone();
                if method.receiver.is_some() {
                    params.insert(0, Ty::Adt(adt.clone(), method.impl_args.clone()));
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
        let targs = self.fresh_args(&adt);
        let variant = self.variant_inst(&adt, v, &targs);
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
            self.check(base, &Ty::Adt(adt.clone(), targs.clone()));
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
        Ty::Adt(adt, targs)
    }

    // ------------------------------------------------------------ fields & methods

    pub(super) fn adt_field(&mut self, f: &ast::FieldExpr, adt: &str, targs: &[Ty]) -> Ty {
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
        if def.variants[0].field(&key).is_some() {
            let variant = self.variant_inst(adt, 0, targs);
            return variant
                .field(&key)
                .map(|(_, f)| f.ty.clone())
                .unwrap_or(Ty::Unknown);
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
        targs: &[Ty],
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        if let Some(sig) = self.adts.method(adt, method).cloned()
            && sig.receiver.is_some()
        {
            let sig = self.method_inst(&sig, Some(targs));
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
        let t = Ty::Adt(adt.to_string(), targs.to_vec());
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
        let targs = self.fresh_args(&adt);
        if !self.unify(expected, &Ty::Adt(adt.clone(), targs.clone())) {
            let msg = format!(
                "mismatched types: expected `{}`, found `{adt}`",
                self.describe(expected)
            );
            self.error(p.syntax(), msg);
        }
        let variant = self.variant_inst(&adt, v, &targs);
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
        if let Ty::Adt(n, _) = self.shallow(t)
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
                && !(self.implements(&Ty::Adt(n.clone(), vec![]), "Hash")
                    && self.implements(&Ty::Adt(n.clone(), vec![]), "Eq"))
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
        if let Ty::Adt(n, _) = x
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

/// A struct's or enum's type parameter names (`struct Stack<T>` → `["T"]`).
fn type_param_names(item: &ast::Item) -> Vec<String> {
    let gp = match item {
        ast::Item::Struct(s) => s.generic_param_list(),
        ast::Item::Enum(e) => e.generic_param_list(),
        _ => None,
    };
    gp.into_iter()
        .flat_map(|l| l.generic_params())
        .filter_map(|p| match p {
            ast::GenericParam::TypeParam(tp) => tp.name().map(|n| n.text().to_string()),
            _ => None,
        })
        .collect()
}
