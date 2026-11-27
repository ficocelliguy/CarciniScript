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
use crate::ty::{RangeKind, Ty};

impl Checker<'_> {
    // ------------------------------------------------------------ items

    /// Declare every struct and enum, lower their fields, then read `impl` signatures. Runs
    /// before function signatures so they can name these types.
    pub(super) fn collect_adts(&mut self, file: &ast::SourceFile) {
        for item in crate::adt::all_items(file) {
            if let ast::Item::Trait(t) = &item
                && let Some(n) = t.name()
            {
                self.file_trait_names.insert(n.text().to_string());
            }
        }
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
        let mut const_structs: Vec<(String, ast::Struct)> = Vec::new();
        let mut const_enums: Vec<(String, Option<ast::GenericParamList>)> = Vec::new();
        for item in crate::adt::all_items(file) {
            if let ast::Item::Struct(s) = &item
                && let Some(n) = s.name()
            {
                const_structs.push((n.text().to_string(), s.clone()));
            }
            if let ast::Item::Enum(e) = &item
                && let Some(n) = e.name()
            {
                const_enums.push((n.text().to_string(), e.generic_param_list()));
            }
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
            let bounds = type_param_bounds(&item);
            if bounds.iter().any(|b| !b.is_empty()) {
                self.adts.param_bounds.insert(name.text().to_string(), bounds);
            }
        }
        // And those an inherent impl declares (`impl<S: RawStream> Auto<S> { fn always(s: S) }`),
        // which settle a new value's type arguments the same way. (Only inferred from, so one
        // impl's bound reaching another impl's calls is lenient, not wrong.)
        for item in crate::adt::all_items(file) {
            let ast::Item::Impl(imp) = &item else { continue };
            if imp.trait_().is_some() {
                continue;
            }
            for (adt, pos, bounds) in impl_param_bounds(imp) {
                let Some(def) = self.adts.defs.get(&adt) else { continue };
                let n = def.type_params.len();
                let all = self
                    .adts
                    .param_bounds
                    .entry(adt)
                    .or_insert_with(|| vec![Vec::new(); n]);
                if let Some(slot) = all.get_mut(pos) {
                    for b in bounds {
                        if !slot.contains(&b) {
                            slot.push(b);
                        }
                    }
                }
            }
        }
        // Const parameters (`struct Buf<T, const N: usize = 4>`), then how each is read off
        // the fields (`data: [T; N]`).
        // (An enum's values keep theirs too; none is read off a field.)
        let lists = const_structs
            .iter()
            .map(|(n, s)| (n, s.generic_param_list()))
            .chain(const_enums.iter().map(|(n, l)| (n, l.clone())));
        for (n, list) in lists {
            let mut cps = Vec::new();
            for p in list.into_iter().flat_map(|l| l.generic_params()) {
                if let ast::GenericParam::ConstParam(cp) = p
                    && let Some(name) = cp.name()
                {
                    self.const_param_ty(&cp, true);
                    let default = match cp.default_val().and_then(|d| d.expr()) {
                        Some(e) => match self.lower_const_arg(&e) {
                            Ty::ConstVal(v) => Some(v),
                            _ => None,
                        },
                        None => None,
                    };
                    cps.push(crate::adt::ConstParam {
                        name: name.text().to_string(),
                        default,
                        derive: None,
                    });
                }
            }
            if !cps.is_empty() {
                self.adts.const_params.insert(n.clone(), cps);
            }
        }
        for (n, s) in &const_structs {
            let Some(mut cps) = self.adts.const_params.get(n).cloned() else {
                continue;
            };
            let fields: Vec<Option<ast::Type>> = match s.field_list() {
                Some(ast::FieldList::RecordFieldList(l)) => l.fields().map(|f| f.ty()).collect(),
                Some(ast::FieldList::TupleFieldList(l)) => l.fields().map(|f| f.ty()).collect(),
                None => Vec::new(),
            };
            for cp in &mut cps {
                cp.derive = fields.iter().enumerate().find_map(|(k, t)| {
                    Some((
                        k,
                        crate::adt::const_path(t.as_ref()?, &cp.name, &self.adts)?,
                    ))
                });
            }
            self.adts.const_params.insert(n.clone(), cps);
        }
        for item in crate::adt::all_items(file) {
            match &item {
                ast::Item::Struct(s) => {
                    let Some(name) = s.name() else { continue };
                    self.enter_type_params(&type_param_names(&item));
                    self.note_type_defaults(&name.text(), s.generic_param_list());
                    let saved_bounds = self.enter_param_bounds(&item);
                    // `struct Pairs<I: Iterator<Item = T>> { it: I }`: the runtime's iterator.
                    for tp in s
                        .generic_param_list()
                        .into_iter()
                        .flat_map(|l| l.type_or_const_params())
                    {
                        if let ast::TypeOrConstParam::Type(tp) = tp
                            && let Some(n) = tp.name()
                        {
                            if let Some(it) = self.iter_param(&tp) {
                                self.generics.insert(n.text().to_string(), it);
                            } else if let Some(c) = self.fn_bound_param(&tp, s.where_clause()) {
                                // `struct Memo<F: Fn(u64) -> u64> { f: F }` (or `where F: Fn(..)`):
                                // the field is a closure, so a literal's closure takes its
                                // parameter types from it.
                                self.generics.insert(n.text().to_string(), c);
                            }
                        }
                    }
                    let variant =
                        self.lower_variant(name.text().to_string().as_str(), s.field_list());
                    self.generics.clear();
                    self.param_bounds = saved_bounds;
                    if let Some(def) = self.adts.defs.get_mut(name.text().to_string().as_str())
                        && !def.is_enum
                    {
                        def.variants = vec![variant];
                    }
                }
                ast::Item::Enum(e) => {
                    let Some(name) = e.name() else { continue };
                    self.enter_type_params(&type_param_names(&item));
                    self.note_type_defaults(&name.text(), e.generic_param_list());
                    let saved_bounds = self.enter_param_bounds(&item);
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
                        if v.attrs()
                            .any(|a| a.syntax().text().to_string().starts_with("#[cfg"))
                        {
                            let en = e.name().map(|n| n.text().to_string()).unwrap_or_default();
                            self.adts.cfg_variants.insert((en, variants.len()));
                        }
                        if let Some(d) = v.const_arg().and_then(|c| c.expr()) {
                            match const_eval(file, &d, 0).and_then(|n| i64::try_from(n).ok()) {
                                Some(n) => next_discr = n,
                                None => self.error(
                                    d.syntax(),
                                    "enum discriminants must be constant integer expressions (literals, `const`s, other enums' variants, arithmetic and `as` casts)",
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
                    self.param_bounds = saved_bounds;
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
        // `impl` signatures. (`Deref` ones first: a `DerefMut` impl's `-> &mut Self::Target` is
        // their `type Target`, whichever is written first.)
        let mut impls: Vec<ast::Impl> = crate::adt::all_items(file)
            .into_iter()
            .filter_map(|item| match item {
                ast::Item::Impl(imp) => Some(imp),
                _ => None,
            })
            .collect();
        // (And an impl for `String` / `Vec<T>` after one for `str` / `[T]`: see `own_twins`. And
        // inherent impls before trait impls: a trait method an inherent one shares its name with
        // goes under `Trait$method`, whichever is written first.)
        impls.sort_by_key(|imp| {
            (
                !imp.trait_()
                    .is_some_and(|t| t.syntax().text().to_string().ends_with("Deref")),
                crate::adt::twin_side(imp) == Some(true),
                imp.trait_().is_some(),
            )
        });
        for imp in &impls {
            self.collect_impl(imp);
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
            self.check_serde(&item);
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
            // (`Parser::<u8>::default()`: generic like an `impl<T> Parser<T>`'s function.)
            let args: Vec<Ty> = self.adts.defs[&n]
                .type_params
                .iter()
                .map(|p| Ty::Param(p.clone()))
                .collect();
            self.adts
                .methods
                .entry(n.clone())
                .or_default()
                .entry("default".into())
                .or_insert(Method {
                    params: vec![],
                    ret: Ty::Adt(n, args.clone()),
                    is_async: false,
                    receiver: None,
                    cell_params: vec![],
                    into_params: vec![],
                    impl_args: args,
                    type_params: vec![],
                    ref_params: vec![],
                    ret_owned: true,
                    ret_mut_ref: false,
                    ret_slot: None,
                    opt_slot_params: vec![],
                    cell_self: false,
                    trait_params: vec![],
                });
        }
    }

    /// A type parameter bounded by `Iterator<Item = T>`, as the runtime's iterator over `T`.
    fn iter_param(&mut self, tp: &ast::TypeParam) -> Option<Ty> {
        let item = super::bounds_iterator_item(&tp.type_bound_list()?)?;
        let elem = match item {
            Some(t) => self.lower_type(&t),
            None => self.fresh(super::VarKind::General),
        };
        Some(Ty::Iter {
            elem: Box::new(elem),
            mutable: false,
        })
    }

    /// `T`, `U` as type parameters for lowering field and signature types.
    /// The closure type a struct's type parameter gets from an `Fn` bound, inline or in the
    /// `where` clause.
    fn fn_bound_param(&mut self, tp: &ast::TypeParam, wc: Option<ast::WhereClause>) -> Option<Ty> {
        let n = tp.name()?.text().to_string();
        let lists: Vec<ast::TypeBoundList> = tp
            .type_bound_list()
            .into_iter()
            .chain(where_clause_bounds(wc, &n))
            .collect();
        lists.iter().find_map(|l| self.lower_fn_bounds(l))
    }

    /// A struct's or enum's parameter bounds while its fields are lowered, so `A::Item` under
    /// `struct S<A: Array>` is `A`'s projection; returns the bounds to restore.
    fn enter_param_bounds(&mut self, item: &ast::Item) -> HashMap<String, Vec<String>> {
        let saved = self.param_bounds.clone();
        let gp = match item {
            ast::Item::Struct(s) => s.generic_param_list(),
            ast::Item::Enum(e) => e.generic_param_list(),
            _ => None,
        };
        let names = gp.into_iter().flat_map(|l| l.type_or_const_params()).filter_map(|p| match p {
            ast::TypeOrConstParam::Type(tp) => tp.name().map(|n| n.text().to_string()),
            _ => None,
        });
        for (n, b) in names.zip(type_param_bounds(item)) {
            if !b.is_empty() {
                self.param_bounds.insert(n, b);
            }
        }
        saved
    }

    /// `struct Map<K, S = RandomState>`: `S`'s default, for `Map<K>` written as a type.
    fn note_type_defaults(&mut self, adt: &str, list: Option<ast::GenericParamList>) {
        let mut out = Vec::new();
        for p in list.into_iter().flat_map(|l| l.generic_params()) {
            if let ast::GenericParam::TypeParam(tp) = p
                && let (Some(n), Some(d)) = (tp.name(), tp.default_type())
            {
                let t = self.lower_type(&d);
                out.push((n.text().to_string(), t));
            }
        }
        if !out.is_empty() {
            self.adts.type_defaults.insert(adt.to_string(), out);
        }
    }

    fn enter_type_params(&mut self, names: &[String]) {
        self.generics.clear();
        for n in names {
            self.generics.insert(n.clone(), Ty::Param(n.clone()));
        }
    }

    /// Type parameters to their arguments for `adt` (unknown when the arguments aren't known).
    pub(super) fn adt_subst(&self, adt: &str, args: &[Ty]) -> HashMap<String, Ty> {
        let Some(def) = self.adts.defs.get(adt) else {
            return HashMap::new();
        };
        let mut map: HashMap<String, Ty> = def
            .type_params
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), args.get(i).cloned().unwrap_or(Ty::Unknown)))
            .collect();
        // `A::Item` of `struct SmallVec<A: Array>` for a known `A` (`[i32; 4]`): its impl's.
        if self.adts.param_bounds.contains_key(adt) {
            for (p, a) in def.type_params.iter().zip(args) {
                let z = self.zonk(a);
                let key = match &z {
                    Ty::Adt(n, _) => Some(n.clone()),
                    Ty::Var(_) | Ty::Unknown | Ty::Param(_) => None,
                    other => crate::adt::ext_key_of(other).map(str::to_string),
                };
                let Some(key) = key else { continue };
                for ((k, tr), assoc) in &self.adts.impl_assoc {
                    if *k != key {
                        continue;
                    }
                    for (name, t) in assoc {
                        let t = self.through_impl_head(&z, k, tr, t);
                        map.entry(format!("{p}::{name}")).or_insert(t);
                    }
                }
            }
        }
        map
    }

    /// Fresh type arguments for a new value of `adt`.
    pub(super) fn fresh_args(&mut self, adt: &str) -> Vec<Ty> {
        let n = self
            .adts
            .defs
            .get(adt)
            .map(|d| d.type_params.len())
            .unwrap_or(0);
        let args: Vec<Ty> = (0..n).map(|_| self.fresh(VarKind::General)).collect();
        self.note_adt_bounds(adt, &args);
        args
    }

    /// The user-trait bounds `adt` declares on its type parameters, for `args` (see
    /// `adt_bounds`).
    fn note_adt_bounds(&mut self, adt: &str, args: &[Ty]) {
        let Some(bounds) = self.adts.param_bounds.get(adt) else {
            return;
        };
        for (a, bs) in args.iter().zip(bounds) {
            for b in bs {
                if self.adts.traits.contains_key(b) {
                    self.adt_bounds.push((a.clone(), b.clone()));
                }
            }
        }
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

    /// Bind an `impl`'s type argument pattern (`T`, or `Vec<T>` of `impl<T> S<Vec<T>>`) to the
    /// receiver's argument (`arg`), or to fresh variables for an associated function call.
    fn bind_impl_arg(
        &mut self,
        pat: &Ty,
        arg: Option<&Ty>,
        map: &mut HashMap<String, Ty>,
        assoc: bool,
    ) {
        match pat {
            Ty::Param(u) => {
                let a = match arg {
                    Some(a) => a.clone(),
                    None => self.fresh(VarKind::General),
                };
                // (An associated function takes classes for its `impl`'s parameters too.)
                if assoc && !map.contains_key(u) {
                    self.last_type_args.push(a.clone());
                }
                map.insert(u.clone(), a);
            }
            other => {
                let inner = other.inner_types();
                if inner.is_empty() {
                    return;
                }
                let arg_inner: Vec<Ty> = arg
                    .map(|a| self.shallow(a).inner_types())
                    .unwrap_or_default();
                for (k, p) in inner.iter().enumerate() {
                    self.bind_impl_arg(p, arg_inner.get(k), map, assoc);
                }
            }
        }
    }

    /// A method's signature for a receiver with type arguments `recv_args` (None for an
    /// associated function call, whose impl parameters get fresh variables).
    pub(super) fn method_inst(&mut self, m: &Method, recv_args: Option<&[Ty]>) -> Method {
        let mut map = HashMap::new();
        // (The method's own type arguments, in order, for `T::make()` in its body.)
        self.last_type_args.clear();
        for p in &m.type_params {
            let v = self.fresh(VarKind::General);
            self.last_type_args.push(v.clone());
            map.insert(p.clone(), v);
        }
        for (i, ia) in m.impl_args.iter().enumerate() {
            let arg = recv_args.map(|args| args.get(i).cloned().unwrap_or(Ty::Unknown));
            let assoc = recv_args.is_none() && m.receiver.is_none();
            self.bind_impl_arg(ia, arg.as_ref(), &mut map, assoc);
        }
        // `-> F::Container` with `where F: KeyMethod<Key, I>` on the method: settled once `F` is
        // known, from its impl of the one trait that declares `Container`.
        let mut projs: Vec<String> = Vec::new();
        for t in m.params.iter().chain([&m.ret]) {
            t.walk(&mut |x| {
                if let Ty::Param(p) = x
                    && let Some((s, _)) = p.split_once("::")
                    && map.contains_key(s)
                    && !projs.contains(p)
                {
                    projs.push(p.clone());
                }
            });
        }
        for p in projs {
            let (s, a) = p.split_once("::").unwrap_or_default();
            let declaring: Vec<&String> = self
                .adts
                .traits
                .iter()
                .filter(|(_, d)| d.assoc_types.iter().any(|x| x == a))
                .map(|(n, _)| n)
                .collect();
            let [tr] = declaring.as_slice() else { continue };
            let (tr, a, sv) = ((*tr).clone(), a.to_string(), map[s].clone());
            let x = self.fresh(VarKind::General);
            self.pending_projections.push((sv, tr, a, x.clone()));
            map.insert(p, x);
        }
        // `impl<A, B, C, D> Equivalent<(C, D)> for (A, B)`, `impl<T: PartialEq<U>, U>
        // PartialEq<U> for Sp<T>`: impl parameters only the trait's arguments name are fresh per
        // call (unless the call is in that impl, where they're in scope).
        // (They follow the other dict names in `last_type_args`.)
        for p in &m.trait_params {
            if m.type_params.contains(p) || m.receiver.is_none() && m.impl_args.contains(&Ty::Param(p.clone())) {
                continue;
            }
            if !map.contains_key(p) && !self.generics.contains_key(p) {
                let v = self.fresh(VarKind::General);
                self.last_type_args.push(v.clone());
                map.insert(p.clone(), v);
            } else {
                let t = map.get(p).cloned().unwrap_or_else(|| Ty::Param(p.clone()));
                self.last_type_args.push(t);
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
            into_params: m.into_params.clone(),
            impl_args: m.impl_args.iter().map(|t| subst(t, &map)).collect(),
            type_params: m.type_params.clone(),
            ref_params: m.ref_params.clone(),
            ret_owned: m.ret_owned,
            ret_mut_ref: m.ret_mut_ref,
            ret_slot: m.ret_slot,
            opt_slot_params: m.opt_slot_params.clone(),
            cell_self: m.cell_self,
            trait_params: m.trait_params.clone(),
        }
    }

    /// Add a method's own type parameters (and bounds) to the impl's; returns their names.
    /// (See `where_bounds` for the `where` clause.)
    fn enter_method_generics(&mut self, f: &ast::Fn) -> Vec<String> {
        let mut own = Vec::new();
        // `where A::Item: PartialEq` on the method.
        self.note_where_projections(f.where_clause());
        if let Some(gp) = f.generic_param_list() {
            // Parameters whose bounds name other parameters (`OP: FnOnce() -> R`, `S: Into<T>`,
            // `I: IntoIterator<Item = P>`) come after those, whatever the order written; then
            // those with trait arguments after the plain ones.
            let mut gps: Vec<ast::GenericParam> = gp.generic_params().collect();
            let lists_of = |tp: &ast::TypeParam| -> Option<(String, Vec<ast::TypeBoundList>)> {
                let n = tp.name()?.text().to_string();
                let lists = tp
                    .type_bound_list()
                    .into_iter()
                    .chain(where_bounds(f, &n))
                    .collect();
                Some((n, lists))
            };
            let all: Vec<(String, Vec<ast::TypeBoundList>)> = gps
                .iter()
                .filter_map(|p| match p {
                    ast::GenericParam::TypeParam(tp) => lists_of(tp),
                    _ => None,
                })
                .collect();
            let depths = super::generic::mention_depths(&all);
            gps.sort_by_key(|p| match p {
                ast::GenericParam::TypeParam(tp) => lists_of(tp).map_or((0, false), |(n, l)| {
                    (
                        depths.get(&n).copied().unwrap_or(0),
                        l.iter().any(bounds_name_params),
                    )
                }),
                _ => (0, false),
            });
            for p in gps {
                match p {
                    ast::GenericParam::TypeParam(tp) => {
                        let Some(n) = tp.name() else { continue };
                        let n = n.text().to_string();
                        // Inline bounds and the `where` clause's.
                        let lists: Vec<ast::TypeBoundList> = tp
                            .type_bound_list()
                            .into_iter()
                            .chain(where_bounds(f, &n))
                            .collect();
                        let bounds: Vec<String> = lists
                            .iter()
                            .flat_map(|l| l.bounds())
                            .filter_map(|b| match b.ty()? {
                                ast::Type::PathType(pt) => {
                                    Some(pt.path()?.segment()?.name_ref()?.text().to_string())
                                }
                                _ => None,
                            })
                            .collect();
                        // An `F: Fn(..)` parameter is a closure type; an `I: Iterator<Item = T>`
                        // one, the runtime's iterator.
                        let closure = lists.iter().find_map(|l| {
                            if let Some(t) = self.lower_fn_bounds(l) {
                                return Some(t);
                            }
                            // `S: Into<String>` / `S: AsRef<str>`: the target type.
                            if let Some(target) = super::conversion_target(l) {
                                return Some(self.lower_type(&target));
                            }
                            // `H: Hasher` in an `impl Hash`.
                            if super::bounds_hasher(l) {
                                return Some(Ty::Hasher);
                            }
                            if super::crates::bounds_rng(l) {
                                return Some(Ty::Rng);
                            }
                            // `I: IntoIterator<Item = T>`: any iterable of `T`.
                            if let Some(item) = super::bounds_into_iter_item(l) {
                                let elem = match item {
                                    Some(t) => self.lower_type(&t),
                                    None => self.fresh(super::VarKind::General),
                                };
                                return Some(Ty::IntoIter(Box::new(elem)));
                            }
                            let item = super::bounds_iterator_item(l)?;
                            let elem = match item {
                                Some(t) => self.lower_type(&t),
                                None => self.fresh(super::VarKind::General),
                            };
                            Some(Ty::Iter {
                                elem: Box::new(elem),
                                mutable: false,
                            })
                        });
                        match closure {
                            Some(t) => {
                                self.generics.insert(n.clone(), t);
                            }
                            None => {
                                self.generics.insert(n.clone(), Ty::Param(n.clone()));
                                self.param_bounds.insert(n.clone(), bounds);
                                self.record_bound_args(&n, &lists);
                                self.record_from_bound(&n, &lists, false);
                                self.note_where_projections(f.where_clause());
                                own.push(n);
                            }
                        }
                    }
                    ast::GenericParam::ConstParam(cp) => {
                        let Some(n) = cp.name() else { continue };
                        let n = n.text().to_string();
                        let ty = self.const_param_ty(&cp, true);
                        self.generics.insert(n.clone(), Ty::Param(n.clone()));
                        self.const_tys.insert(n.clone(), ty);
                        own.push(n);
                    }
                    ast::GenericParam::LifetimeParam(_) => {}
                }
            }
            // (Back in the written order: turbofish arguments follow it.)
            let order: Vec<String> = gp
                .generic_params()
                .filter_map(|p| match p {
                    ast::GenericParam::TypeParam(tp) => Some(tp.name()?.text().to_string()),
                    ast::GenericParam::ConstParam(cp) => Some(cp.name()?.text().to_string()),
                    ast::GenericParam::LifetimeParam(_) => None,
                })
                .collect();
            own.sort_by_key(|n| order.iter().position(|x| x == n));
        }
        // `fn log(&self) where T: Display` on `impl<T> Value<T>`: the impl's `T`, bounded
        // further in this method.
        for pred in f.where_clause().into_iter().flat_map(|w| w.predicates()) {
            let Some(ast::Type::PathType(pt)) = pred.ty() else {
                continue;
            };
            let Some(n) = pt.path().and_then(|p| p.as_single_name_ref()) else {
                continue;
            };
            let n = n.text().to_string();
            if own.contains(&n) || !self.param_bounds.contains_key(&n) {
                continue;
            }
            let lists: Vec<ast::TypeBoundList> = pred.type_bound_list().into_iter().collect();
            // (`where K: Borrow<Q>`: `self.map.get(k)` with a `&Q`.)
            self.record_from_bound(&n, &lists, true);
            let extra: Vec<String> = super::generic::bound_names(&lists)
                .into_iter()
                .map(|(b, _)| b)
                .filter(|b| {
                    super::generic::KNOWN_BOUNDS.contains(&b.as_str())
                        || self.adts.traits.contains_key(b)
                })
                .collect();
            if extra.is_empty() {
                continue;
            }
            let before = self.param_bounds[&n].clone();
            if !self.method_where.iter().any(|(x, _)| *x == n) {
                self.method_where.push((n.clone(), before));
            }
            self.param_bounds.get_mut(&n).unwrap().extend(extra);
            self.record_bound_args(&n, &lists);
        }
        // `x: impl Display`: an anonymous parameter of the method's, after the written ones.
        for (name, bounds, key) in super::generic::impl_arg_params(f) {
            self.generics.insert(key, Ty::Param(name.clone()));
            self.generics.insert(name.clone(), Ty::Param(name.clone()));
            self.param_bounds.insert(name.clone(), bounds);
            own.push(name);
        }
        own
    }

    /// The impl's parameters as they were before a method's `where` clause (see
    /// `enter_method_generics`).
    fn leave_method_where(&mut self) {
        for (n, bounds) in std::mem::take(&mut self.method_where) {
            self.param_bounds.insert(n, bounds);
        }
        // (A method's `where K: Borrow<Q>` names the method's own `Q`.)
        self.borrow_args.clear();
    }

    /// The `impl`'s type parameters (with bounds) for lowering and checking its methods; returns
    /// the impl's self type (`Stack<U>`).
    fn enter_impl(&mut self, imp: &ast::Impl, type_name: &str) -> Ty {
        self.generics.clear();
        self.param_bounds.clear();
        self.param_trait_args.clear();
        self.param_trait_alts.clear();
        self.op_args.clear();
        self.cmp_args.clear();
        self.lenient_projs.clear();
        self.const_tys.clear();
        self.from_args.clear();
        self.try_from_args.clear();
        self.borrow_args.clear();
        let mut impl_params: Vec<(String, Option<ast::TypeBoundList>)> = Vec::new();
        if let Some(gp) = imp.generic_param_list() {
            for p in gp.generic_params() {
                // `impl<const N: usize> Buf<N>`: a value in the methods.
                if let ast::GenericParam::ConstParam(cp) = &p
                    && let Some(n) = cp.name()
                {
                    let n = n.text().to_string();
                    let ty = self.const_param_ty(cp, false);
                    self.generics.insert(n.clone(), Ty::Param(n.clone()));
                    self.const_tys.insert(n, ty);
                    continue;
                }
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
                    // `impl<T> .. where T: Debug`.
                    let mut bounds = bounds;
                    if let Some(wc) = imp.where_clause() {
                        for pred in wc.predicates() {
                            if let Some(ast::Type::PathType(pt)) = pred.ty()
                                && pt
                                    .path()
                                    .and_then(|p| p.as_single_name_ref())
                                    .is_some_and(|x| x.text() == n.as_str())
                            {
                                bounds.extend(pred.type_bound_list().into_iter().flat_map(|l| {
                                    l.bounds()
                                        .filter_map(|b| match b.ty()? {
                                            ast::Type::PathType(p) => Some(
                                                p.path()?.segment()?.name_ref()?.text().to_string(),
                                            ),
                                            _ => None,
                                        })
                                        .collect::<Vec<_>>()
                                }));
                            }
                        }
                    }
                    // `impl<I: Iterator> Stats for I` without `Item = ..`: an iterator of
                    // `I::Item`, which each call takes from its receiver (`blanket_inst`).
                    if crate::adt::blanket_impl(imp).is_some_and(|(_, t, _)| t == n)
                        && tp
                            .type_bound_list()
                            .and_then(|l| super::bounds_iterator_item(&l))
                            .is_some_and(|item| item.is_none())
                    {
                        let item = Ty::Param(format!("{n}::Item"));
                        self.generics.insert(format!("{n}::Item"), item.clone());
                        self.generics.insert(
                            n,
                            Ty::Iter {
                                elem: Box::new(item),
                                mutable: false,
                            },
                        );
                        continue;
                    }
                    // `impl<I: Iterator<Item = T>> .. for Pairs<I>`: the runtime's iterator.
                    // Without `Item = ..` (inline or `where I: Iterator`), its items are a
                    // parameter of their own, `I::Item`.
                    let lists: Vec<ast::TypeBoundList> = tp
                        .type_bound_list()
                        .into_iter()
                        .chain(where_clause_bounds(imp.where_clause(), &n))
                        .collect();
                    if let Some(item) = lists.iter().find_map(super::bounds_iterator_item) {
                        let elem = match item {
                            Some(t) => self.lower_type(&t),
                            None => {
                                let p = format!("{n}::Item");
                                // (With `where I::Item: Display`, as `Item: Display` becomes.)
                                let item_lists: Vec<ast::TypeBoundList> = imp
                                    .where_clause()
                                    .into_iter()
                                    .flat_map(|w| w.predicates())
                                    .filter(|pr| {
                                        pr.ty().is_some_and(|t| t.syntax().text() == p.as_str())
                                    })
                                    .filter_map(|pr| pr.type_bound_list())
                                    .collect();
                                let bounds: Vec<String> =
                                    super::generic::bound_names(&item_lists)
                                        .into_iter()
                                        .map(|(b, _)| b)
                                        .filter(|b| {
                                            super::generic::KNOWN_BOUNDS.contains(&b.as_str())
                                                || self.adts.traits.contains_key(b)
                                        })
                                        .collect();
                                self.generics.insert(p.clone(), Ty::Param(p.clone()));
                                self.param_bounds.insert(p.clone(), bounds);
                                Ty::Param(p)
                            }
                        };
                        let it = Ty::Iter {
                            elem: Box::new(elem),
                            mutable: false,
                        };
                        self.generics.insert(n, it);
                        continue;
                    }
                    self.generics.insert(n.clone(), Ty::Param(n.clone()));
                    self.param_bounds.insert(n.clone(), bounds);
                    impl_params.push((n, tp.type_bound_list()));
                }
            }
        }
        // The bounds' arguments, inline and in the `where` clause (`where I: Iter<Item = &T>`).
        for (n, inline) in impl_params {
            let mut lists: Vec<ast::TypeBoundList> = inline.into_iter().collect();
            for pred in imp.where_clause().into_iter().flat_map(|w| w.predicates()) {
                if let Some(ast::Type::PathType(pt)) = pred.ty()
                    && pt
                        .path()
                        .and_then(|p| p.as_single_name_ref())
                        .is_some_and(|x| x.text() == n.as_str())
                    && let Some(l) = pred.type_bound_list()
                {
                    lists.push(l);
                }
            }
            self.record_op_bounds(&n, &lists);
            self.record_bound_args(&n, &lists);
            // `impl<F, T> Memo<F, T> where F: Fn(u64) -> T`: `F` is a closure type (once every
            // parameter it names is declared).
            if let Some(closure) = lists.iter().find_map(|l| self.lower_fn_bounds(l)) {
                self.generics.insert(n, closure);
            } else if let Some(out) = lists.iter().find_map(super::bounds_future_output) {
                // `impl<F: Future> Future for Counted<F>`: a future of `F::Output` (`F`'s own
                // parameter, which a value's type argument settles).
                let out = match out {
                    Some(t) => self.lower_type(&t),
                    None => {
                        let o = format!("{n}::Output");
                        self.generics.insert(o.clone(), Ty::Param(o.clone()));
                        self.param_bounds.insert(o.clone(), Vec::new());
                        Ty::Param(o)
                    }
                };
                self.generics.insert(n, Ty::Future(Box::new(out)));
            } else if let Some(target) = lists.iter().find_map(super::conversion_target) {
                // `impl<T: AsRef<str>> Display for K<T>`: a string, as in a function's generics.
                // Only strings: other targets (`AsRef<[u8]>` of a `&str`) convert where a
                // function's argument is passed, and a value of the impl's type has no such step.
                let t = self.lower_type(&target);
                if t == Ty::Str {
                    self.generics.insert(n, t);
                }
            }
        }
        self.note_where_projections(imp.where_clause());
        match imp.self_ty() {
            Some(t) => self.lower_type(&t),
            None => Ty::Adt(type_name.to_string(), vec![]),
        }
    }

    fn lower_variant(&mut self, name: &str, fields: Option<ast::FieldList>) -> Variant {
        let variant_name = name;
        match fields {
            Some(ast::FieldList::RecordFieldList(l)) => Variant {
                discr: 0,
                name: name.into(),
                shape: Shape::Named,
                fields: l
                    .fields()
                    .map(|f| {
                        let name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
                        let ty = f.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                        if self.copy_array(f.ty().as_ref(), None, &ty) {
                            self.array_fields.insert((variant_name.to_string(), name.clone()));
                        }
                        if f.ty().as_ref().is_some_and(shared_ref_type) {
                            self.shared_ref_fields
                                .insert((variant_name.to_string(), name.clone()));
                        }
                        if f.ty().as_ref().is_some_and(rc_type) {
                            self.rc_fields.insert((variant_name.to_string(), name.clone()));
                        }
                        if matches!(f.ty(), Some(ast::Type::ArrayType(_))) {
                            self.array_ann_fields
                                .insert((variant_name.to_string(), name.clone()));
                        }
                        if f.ty().as_ref().is_some_and(fn_ptr_type) {
                            self.fn_ptr_fields
                                .insert((variant_name.to_string(), name.clone()));
                        }
                        if f.ty().as_ref().is_some_and(super::written_mut_ref_type) {
                            self.mut_borrow_fields
                                .insert((variant_name.to_string(), name.clone()));
                        }
                        // `n: &'a mut i32`: the field holds a slot over the borrowed place.
                        if f.ty().as_ref().is_some_and(super::written_mut_ref_type)
                            && self.cellable(&ty)
                        {
                            self.mut_ref_fields
                                .insert((variant_name.to_string(), name.clone()));
                        }
                        if let Some(t) = f.ty()
                            && super::is_ptr_type(&t)
                        {
                            self.ptrs.fields.insert(name.clone());
                            if self.cellable(&ty) {
                                self.error(
                                    t.syntax(),
                                    "a raw pointer to a number, string or bool in a struct isn't supported yet (point at a struct holding it)",
                                );
                            }
                        }
                        Field {
                            js: name.clone(),
                            name,
                            ty,
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
                    .map(|(i, f)| {
                        let ty = f.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                        // (`struct Threads([u32; 4])`: a `Copy` array, as a named field would be.)
                        if self.copy_array(f.ty().as_ref(), None, &ty) {
                            self.array_fields
                                .insert((variant_name.to_string(), i.to_string()));
                        }
                        if f.ty().as_ref().is_some_and(shared_ref_type) {
                            self.shared_ref_fields
                                .insert((variant_name.to_string(), i.to_string()));
                        }
                        if f.ty().as_ref().is_some_and(rc_type) {
                            self.rc_fields.insert((variant_name.to_string(), i.to_string()));
                        }
                        if matches!(f.ty(), Some(ast::Type::ArrayType(_))) {
                            self.array_ann_fields
                                .insert((variant_name.to_string(), i.to_string()));
                        }
                        if f.ty().as_ref().is_some_and(fn_ptr_type) {
                            self.fn_ptr_fields
                                .insert((variant_name.to_string(), i.to_string()));
                        }
                        if f.ty().as_ref().is_some_and(super::written_mut_ref_type) {
                            self.mut_borrow_fields
                                .insert((variant_name.to_string(), i.to_string()));
                        }
                        if f.ty().as_ref().is_some_and(super::written_mut_ref_type)
                            && self.cellable(&ty)
                        {
                            self.mut_ref_fields
                                .insert((variant_name.to_string(), i.to_string()));
                        }
                        Field {
                            name: i.to_string(),
                            ty,
                            js: i.to_string(),
                        }
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
            if def.derives(t) && !def.implements(needs) {
                self.error(
                    node,
                    format!("the trait bound `{name}: {needs}` is not satisfied (`{t}` requires `{needs}`; add it to the derive)"),
                );
            }
        }
        for d in &def.derives {
            if (d == "Default" && def.is_enum) || matches!(d.as_str(), "Serialize" | "Deserialize")
            {
                continue;
            }
            let written = written_field_types(node);
            // Derived impls require the trait of the type parameters (`impl<T: Clone> Clone for ..`),
            // so a field of type `T` is fine.
            let unknowns: HashMap<String, Ty> = def
                .type_params
                .iter()
                .map(|p| (p.clone(), Ty::Unknown))
                .collect();
            for v in &def.variants {
                for f in &v.fields {
                    // A `[T; N]` of `Copy` values is `Copy` (a `Vec` isn't).
                    let copy_array = d == "Copy"
                        && self
                            .array_fields
                            .contains(&(v.name.clone(), f.name.clone()));
                    // A shared reference is `Copy` and `Clone` whatever it points at.
                    let shared_ref = matches!(d.as_str(), "Copy" | "Clone")
                        && self
                            .shared_ref_fields
                            .contains(&(v.name.clone(), f.name.clone()));
                    // `Rc<T>` / `Arc<T>` is `Clone` whatever `T` is.
                    let rc = d == "Clone"
                        && self.rc_fields.contains(&(v.name.clone(), f.name.clone()));
                    // `[T; N]` is `Copy` when `T` is (also a type parameter, as derived impls require).
                    let param_array = d == "Copy"
                        && self
                            .array_ann_fields
                            .contains(&(v.name.clone(), f.name.clone()))
                        && matches!(&f.ty, Ty::Vec(e) if !matches!(**e, Ty::Str) && self.implements(&subst(e, &unknowns), "Copy"));
                    let fn_ptr =
                        matches!(d.as_str(), "Copy" | "Clone" | "Debug" | "PartialEq" | "Eq")
                        && self
                            .fn_ptr_fields
                            .contains(&(v.name.clone(), f.name.clone()));
                    let wt = written.get(&(v.name.clone(), f.name.clone()));
                    // `pred: F` with `F: FnMut(..)`: the derived impl is bounded `F: Debug` (as
                    // for any parameter), so the derive itself stands, whatever `F` lowers to.
                    let bare_param = wt.is_some_and(|t| {
                        bare_path_name(t).is_some_and(|n| def.type_params.contains(&n))
                    });
                    // `std::any::TypeId` (a string in JS) is `Copy`, `Hash` and `Ord`.
                    let type_id = d != "Default"
                        && wt.and_then(bare_path_name).as_deref() == Some("TypeId")
                        && !self.adts.defs.contains_key("TypeId");
                    if !copy_array
                        && !shared_ref
                        && !fn_ptr
                        && !bare_param
                        && !type_id
                        && !rc
                        && !param_array
                        && !self.implements(&subst(&f.ty, &unknowns), d)
                    {
                        self.error(
                            node,
                            format!("the trait `{d}` is not implemented for `{}` (field `{}` of `{name}`)", f.ty.describe(), f.name),
                        );
                    }
                }
            }
        }
    }

    /// serde's attributes the JSON schema can't honour, reported where serde itself would
    /// (at compile time for a tagged enum's tuple variants, at run time for the rest):
    /// `#[serde(flatten)]` on anything but a struct or map, a tagged newtype variant whose
    /// payload isn't one, and `with` / `serialize_with` / `deserialize_with`.
    fn check_serde(&mut self, item: &ast::Item) {
        let Some((name, info)) = crate::json::info_of(item) else {
            return;
        };
        let Some(def) = self.adts.defs.get(&name).cloned() else {
            return;
        };
        let node = item.syntax();
        for v in &def.variants {
            let variant = if def.is_enum { v.name.as_str() } else { "" };
            for f in &v.fields {
                let Some(a) = info.fields.get(&(variant.to_string(), f.name.clone())) else {
                    continue;
                };
                if let Some(k) = &a.with {
                    self.error(
                        node,
                        format!("`#[serde({k} = \"..\")]` is not supported (field `{}` of `{name}`): JSON goes by the field's type", f.name),
                    );
                }
                if a.flatten && !self.flattenable(&f.ty) {
                    self.error(
                        node,
                        format!("can only flatten structs and maps (field `{}` of `{name}` is `{}`)", f.name, f.ty.describe()),
                    );
                }
            }
        }
        if info.content.is_some() && info.tag.is_none() {
            self.error(node, "`#[serde(content = \"..\")]` can only be used together with `tag = \"..\"`");
        }
        if info.untagged && info.tag.is_some() {
            self.error(node, "enum cannot be both untagged and internally tagged");
        }
        if def.is_enum && info.tag.is_some() && info.content.is_none() {
            for v in &def.variants {
                if v.shape != Shape::Tuple {
                    continue;
                }
                if v.fields.len() > 1 {
                    self.error(node, "#[serde(tag = \"...\")] cannot be used with tuple variants");
                } else if !self.flattenable(&v.fields[0].ty) {
                    self.error(
                        node,
                        format!("cannot serialize tagged newtype variant `{name}::{}` containing `{}` (its fields would have to be written next to the tag)", v.name, v.fields[0].ty.describe()),
                    );
                }
            }
        }
    }

    /// Whether a value of `ty` is written as a JSON object (so it can be flattened into another).
    fn flattenable(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Map(..) | Ty::Param(_) | Ty::Unknown => true,
            Ty::Coll(_, inner) | Ty::Option(inner) | Ty::RefCell(inner) => self.flattenable(inner),
            Ty::Adt(n, _) => self
                .adts
                .defs
                .get(n)
                .is_none_or(|d| d.is_enum || d.variants[0].shape == Shape::Named),
            _ => false,
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
            // (An iterator clones what's left: `κσ.Iter.$clone`.)
            Ty::Iter { mutable: false, .. } if t == "Clone" => true,
            // (`{:?}` shows what's left, as `slice::Iter` and `IterMut` do: `Iter([1, 2])`.)
            Ty::Iter { elem, .. } if t == "Debug" => self.implements(elem, t),
            Ty::Closure { .. } | Ty::Iter { .. } | Ty::Future(_) | Ty::Entry(..) => false,
            Ty::Param(n) => self.param_has(n, t),
            // (`std::error::Error`'s supertraits: `Box<dyn Error>` fields derive `Debug`.)
            Ty::Dyn(tr, _) if tr == "Error" && !self.adts.traits.contains_key("Error") => {
                matches!(t, "Debug" | "Display" | "Error")
            }
            // (`dyn Any` is `Debug`: `Any { .. }`.)
            Ty::Dyn(tr, _) if tr == "Any" && !self.adts.traits.contains_key("Any") => t == "Debug",
            // (Or `impl Clone for Box<dyn Tr>`.)
            Ty::Dyn(tr, _) => {
                self.trait_includes(tr, t)
                    || self
                        .adts
                        .trait_impls
                        .get(&format!("dyn${tr}"))
                        .is_some_and(|ts| ts.iter().any(|x| x == t))
            }
            _ => true,
        }
    }

    fn collect_impl(&mut self, imp: &ast::Impl) {
        let (trait_name, type_name) = impl_parts(imp);
        let Some(mut type_name) = type_name.or_else(|| crate::adt::impl_self_name(imp)) else {
            return;
        };
        // (`impl<T> (T,) {}`: only core's docs write inherent impls on tuples.)
        if trait_name.is_none() && crate::adt::tuple_arity(&type_name).is_some() {
            return;
        }
        let node = imp
            .self_ty()
            .map(|t| t.syntax().clone())
            .unwrap_or_else(|| imp.syntax().clone());
        // `impl<T: Tr> Tr for &T` doing more than `T`'s impl: a class of its own, reached by a
        // generic call whose `T` is a reference (see `adt::wrapper_impl`).
        if let Some((key, _)) = crate::adt::wrapper_impl(imp)
            && trait_name.as_deref().is_some_and(|t| self.adts.traits.contains_key(t))
        {
            type_name = key;
            self.adts.ext_types.insert(type_name.clone());
        }
        // `impl<T: Display> Shout for T`: for every type meeting the bounds.
        if let Some((tr, param, bounds)) = crate::adt::blanket_impl(imp)
            && self.adts.traits.contains_key(&tr)
        {
            type_name = format!("{tr}$all");
            self.adts.ext_types.insert(type_name.clone());
            let others = imp
                .generic_param_list()
                .into_iter()
                .flat_map(|l| l.generic_params())
                .filter_map(|p| match p {
                    ast::GenericParam::TypeParam(tp) => Some(tp.name()?.text().to_string()),
                    _ => None,
                })
                .filter(|n| *n != param)
                .collect();
            self.adts.blanket_others.insert(tr.clone(), others);
            self.adts.blanket_params.insert(tr.clone(), param);
            self.adts.blankets.insert(tr, bounds);
        }
        // `impl Money for f64`: a user trait for a built-in type (and `impl From<Id> for String`,
        // `impl From<Sq> for Box<dyn Shape>`).
        if !self.adts.defs.contains_key(&type_name)
            && let Some(key) = crate::adt::ext_key_any(&type_name)
            && (key.starts_with("dyn$")
                || trait_name.as_deref().is_some_and(|t| {
                    self.adts.traits.contains_key(t)
                        || matches!(t, "From" | "PartialEq" | "PartialOrd")
                        || crate::adt::op_method(t).is_some()
                }))
        {
            // (`impl Clone for Box<dyn Tr>`: what `implements` asks of `dyn Tr`.)
            if key.starts_with("dyn$")
                && let Some(t) = &trait_name
            {
                self.adts.trait_impls.entry(key.clone()).or_default().push(t.clone());
            }
            type_name = key;
            self.adts.ext_types.insert(type_name.clone());
        }
        if !self.adts.defs.contains_key(&type_name) && !self.adts.ext_types.contains(&type_name) {
            // `impl<T: Tr> Tr for &T` whose methods do more than call `T`'s: `&T` and `T` are
            // the same value in JS, so they can't have different impls.
            if crate::adt::wrapping_impl(imp) {
                self.error(&node, "an impl for `&T` / `&mut T` / `Box<T>` of a trait not declared in this file must hand each method on to `T`'s impl (`(**self).method(args)`, overriding the default methods `T`'s impls override): references and boxes are erased in JS, so `&T` can't behave differently from `T` there");
                return;
            }
            // `impl<T: Bound> Other for T` with `Other` from another file or crate: what any impl
            // of a trait not declared here gets.
            if let Some((tr, _, _)) = crate::adt::blanket_impl(imp)
                && !self.adts.traits.contains_key(&tr)
            {
                let node = imp.trait_().map(|t| t.syntax().clone()).unwrap_or(node);
                self.error(&node, format!("`impl {tr} for ..` is not supported yet (of the std traits, `Display`, `Debug`, `Default`, `From`, `Into`, `TryFrom`, `FromStr`, `FromIterator`, `Extend`, `Ord`, `PartialOrd`, `PartialEq`, `Eq`, `Hash`, `Clone`, `Copy`, `Drop`, `Iterator`, `Deref`, `AsRef` and the `std::ops` operators can be implemented; derive the others; user traits declared in the file work)"));
                return;
            }
            // `impl Format for u8` with `Format` neither declared here, imported from a `mod`
            // file nor one of std's: the trait is what's missing, not the type.
            if let Some(t) = trait_name.as_deref()
                && !self.adts.traits.contains_key(t)
                && !impl_std_trait(t)
            {
                let node = imp.trait_().map(|t| t.syntax().clone()).unwrap_or(node);
                self.error(&node, unknown_impl_trait(t));
                return;
            }
            self.error(&node, format!("cannot find type `{type_name}` in this scope (impls are for structs and enums in this file)"));
            return;
        }
        let mut unknown_trait = false;
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
            // (`impl PartialEq<u32> for Id` compares with something else, not two `Id`s.)
            Some("PartialOrd" | "PartialEq") if foreign_rhs(imp, &type_name) => {}
            // (Not the file's own `trait Error`, serde's `de::Error`: a user trait.)
            Some(t @ ("Ord" | "PartialOrd" | "PartialEq" | "Eq" | "Clone" | "Debug" | "Error"))
                if !(t == "Error"
                    && self.adts.traits.get("Error").is_some_and(|d| d.module.is_none())
                    && imp
                        .trait_()
                        .is_some_and(|tr| !tr.syntax().text().to_string().contains("error::"))) =>
            {
                if let Some(d) = self.adts.defs.get_mut(&type_name) {
                    d.manual.push(t.to_string());
                }
            }
            Some(t) if crate::adt::op_method(t).is_some() && !self.adts.traits.contains_key(t) => {}
            // `impl Sum for T`: its `sum(iter)` is what `iter.sum::<T>()` calls.
            Some("Sum" | "Product") => {}
            // Marker traits (`unsafe impl Send for T {}`): nothing to do in JS.
            Some("Iterator" | "IntoIterator" | "Send" | "Sync" | "Unpin") => {}
            // A hand-written future: `poll` is an ordinary method; `.await` polls it.
            Some("Future") => {}
            // More markers: `len()` comes from `size_hint`, `next_back` is an ordinary method
            // (`.rev()` calls it), unwinding isn't a thing in JS, and a hasher's `write` /
            // `finish` (a builder's `build_hasher`) are ordinary methods too.
            Some(
                "ExactSizeIterator"
                | "FusedIterator"
                | "DoubleEndedIterator"
                | "TrustedLen"
                | "UnwindSafe"
                | "RefUnwindSafe"
                | "Hasher"
                | "BuildHasher",
            ) => {}
            // `deref` / `deref_mut` are ordinary methods; auto-deref calls them.
            Some("Deref") => {}
            // `as_ref()` / `as_mut()` / `borrow()`: ordinary methods too (`impl AsRef<str> for Host`).
            Some("AsRef" | "AsMut" | "Borrow" | "BorrowMut") => {}
            // `from_str` is an associated function; `s.parse::<T>()` calls it.
            Some("FromStr") => {}
            // `x.try_into()` / `T::try_from(x)`, `collect()` into the type, `t.extend(..)`.
            Some("TryFrom" | "FromIterator" | "Extend") => {}
            // `impl Drop for T`: its `drop` runs when a value goes out of scope (codegen).
            Some("Drop") => {
                if let Some(d) = self.adts.defs.get_mut(&type_name) {
                    d.manual.push("Drop".into());
                }
            }
            // `impl Copy for P {}`: as `#[derive(Copy)]`.
            Some("Copy") => {
                if let Some(d) = self.adts.defs.get_mut(&type_name)
                    && !d.derives("Copy")
                {
                    d.derives.push("Copy".into());
                }
            }
            // Its `hash` feeds the type's `HashMap` / `HashSet` keys (with a hand-written `Eq`).
            Some("Hash") => {
                if let Some(d) = self.adts.defs.get_mut(&type_name) {
                    d.manual.push("Hash".into());
                }
            }
            Some("DerefMut") => {
                self.adts.deref_mut.insert(type_name.clone());
            }
            Some(t) if self.adts.traits.contains_key(t) => {
                self.register_trait_impl(imp, t, &type_name);
            }
            Some(t) => {
                let at = imp.trait_().map(|t| t.syntax().clone()).unwrap_or(node.clone());
                self.error(&at, unknown_impl_trait(t));
                // (Its methods are still registered, as inherent ones: the trait is what's
                // missing, and `self.is_match(..)` elsewhere shouldn't fail for it too.)
                unknown_trait = true;
            }
        }
        let self_ty = self.enter_impl(imp, &type_name);
        let impl_args = match &self_ty {
            Ty::Adt(_, args) => args.clone(),
            // A blanket impl's `T`: its associated functions take `T`'s class (as `this`).
            Ty::Param(p) if type_name.ends_with("$all") => vec![Ty::Param(p.clone())],
            // (And a `&T` impl's: its methods take `T`'s class, `$elem`.)
            Ty::Param(p) if crate::adt::is_wrapper_key(&type_name) => vec![Ty::Param(p.clone())],
            // `impl Total for HashMap<String, i64>` / `impl<T> Tr for Option<T>`.
            t => crate::adt::ext_args(t),
        };
        let saved = self.self_ty.replace(self_ty);
        // `type Target = Vec<i32>;`
        if trait_name.as_deref() == Some("Deref")
            && let Some(target) = imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
                .find_map(|a| match a {
                    ast::AssocItem::TypeAlias(t)
                        if t.name().is_some_and(|n| n.text() == "Target") =>
                    {
                        t.ty()
                    }
                    _ => None,
                })
        {
            let t = self.lower_type(&target);
            self.adts.deref_targets.insert(type_name.clone(), t);
        }
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
            // (`impl<S: Into<X>> From<S> for Y`: what it takes is what converts into an `X`, and
            // `o.into()` in its body hands that on.)
            let via = arg.as_ref().and_then(|t| match t {
                ast::Type::PathType(p) => super::conv::param_into_target(
                    &p.path()?.as_single_name_ref()?.text().to_string(),
                    "Into",
                    imp.generic_param_list(),
                    imp.where_clause(),
                ),
                _ => None,
            });
            let via_into = via.is_some();
            // (`impl<'h, H: ?Sized + AsRef<[u8]>> From<&'h H> for Input<'h>`: it takes what is
            // a `[u8]` here, a string too.)
            let as_ref = (!via_into)
                .then(|| match arg.as_ref()? {
                    ast::Type::RefType(r) => match r.ty()? {
                        ast::Type::PathType(p) => super::conv::param_into_target(
                            &p.path()?.as_single_name_ref()?.text().to_string(),
                            "AsRef",
                            imp.generic_param_list(),
                            imp.where_clause(),
                        ),
                        _ => None,
                    },
                    _ => None,
                })
                .flatten();
            let via_as_ref = as_ref.is_some();
            let ty = via
                .or(as_ref)
                .or(arg)
                .map(|t| self.lower_type(&t))
                .unwrap_or(Ty::Unknown);
            (ty, via_into, via_as_ref)
        });
        // `impl Add<R> for T`: the right operand's type (`Self` by default), and the impl's
        // `type Output = ..;`, which `-> Self::Output` names.
        // (A trait of the file's own named `Add` is that trait, not `std::ops::Add`.)
        let op = trait_name
            .as_deref()
            .filter(|t| !self.adts.traits.contains_key(*t))
            .and_then(crate::adt::op_method);
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
        let into_iter = trait_name.as_deref() == Some("IntoIterator");
        let is_iter = trait_name.as_deref() == Some("Iterator") || into_iter;
        let is_future = trait_name.as_deref() == Some("Future");
        let output = imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
            .find_map(|i| match i {
                // (`IntoIterator`'s `type IntoIter` isn't needed: `into_iter()` says it.)
                ast::AssocItem::TypeAlias(ta)
                    if op.is_some()
                        || (is_iter && ta.name().is_some_and(|n| n.text() == "Item"))
                        || (is_future && ta.name().is_some_and(|n| n.text() == "Output")) =>
                {
                    ta.ty()
                }
                _ => None,
            })
            .map(|t| self.lower_type(&t));
        if into_iter {
            self.adts.into_iters.insert(type_name.clone());
        }
        // `type Item = X;`: values of the type iterate over `X`.
        if is_iter {
            let item = output.clone().unwrap_or(Ty::Unknown);
            self.adts.iter_items.insert(type_name.clone(), item);
            // `type Item = &'a mut T;`: what the loop variable holds is a `&mut`.
            let mut_item = imp
                .assoc_item_list()
                .into_iter()
                .flat_map(|l| l.assoc_items())
                .any(|i| match i {
                    ast::AssocItem::TypeAlias(ta) => {
                        ta.name().is_some_and(|n| n.text() == "Item")
                            && ta.ty().is_some_and(|t| {
                                matches!(t, ast::Type::RefType(r) if r.mut_token().is_some())
                            })
                    }
                    _ => false,
                });
            if mut_item {
                self.adts.iter_mut_items.insert(type_name.clone());
            }
        }
        if is_future {
            let out = output.clone().unwrap_or(Ty::Unknown);
            self.adts.future_outputs.insert(type_name.clone(), out);
        }
        let saved_output = std::mem::replace(&mut self.assoc_output, output);
        // Every `type X = ..;` of the impl, for `Self::X`.
        let assoc: HashMap<String, Ty> = imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
            .filter_map(|i| match i {
                ast::AssocItem::TypeAlias(ta) => Some((ta.name()?.text().to_string(), ta.ty()?)),
                _ => None,
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(n, t)| (n, self.lower_type(&t)))
            .collect();
        let saved_assoc = std::mem::replace(&mut self.impl_assoc, assoc);
        for item in imp
            .assoc_item_list()
            .into_iter()
            .flat_map(|l| l.assoc_items())
        {
            let ast::AssocItem::Fn(f) = item else {
                let is_deref = trait_name.as_deref() == Some("Deref");
                // (`type Err` / `type Error`; a `BuildHasher`'s `type Hasher`.)
                let from_str = matches!(
                    trait_name.as_deref(),
                    Some("FromStr" | "TryFrom" | "BuildHasher")
                );
                let user_trait = trait_name
                    .as_deref()
                    .is_some_and(|t| self.adts.traits.contains_key(t));
                // (An unknown trait's `type X = ..;` and consts: the trait's error says it.)
                if unknown_trait {
                    continue;
                }
                if (op.is_some() || is_iter || is_future || is_deref || from_str || user_trait)
                    && matches!(item, ast::AssocItem::TypeAlias(_))
                {
                    continue;
                }
                // `const W: usize = 3;`
                if let ast::AssocItem::Const(k) = &item
                    && let Some(n) = k.name()
                {
                    let ann = k.ty();
                    let ty = ann.as_ref().map(|t| self.lower_type(t)).unwrap_or(Ty::Unknown);
                    if self.copy_array(ann.as_ref(), None, &ty) {
                        self.copy_array_assoc.insert((type_name.clone(), n.text().to_string()));
                    }
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
            // (A `&mut self` method on a string, number, bool, char or `Option` takes a cell as
            // `this`: see `codegen_adt::cell_self`.)
            method.impl_args = impl_args.clone();
            method.trait_params = trait_only_params(imp);
            // `impl Inv for &f32` (next to `impl Inv for f32`, one type in JS): under `inv$ref`,
            // picked per call (`pick_ref_impl`).
            if let Some(kind) = crate::adt::concrete_ref_impl(imp) {
                let key = format!("{}${kind}", name.text());
                let at = self.range(name.syntax()).0;
                self.adts.alt_keys_at.insert(at, key.clone());
                if let Some(tr) = &trait_name {
                    self.adts.ref_impl_traits.insert(tr.clone());
                }
                self.adts
                    .methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert(key, method);
                continue;
            }
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
            if let Some((src, via_into, via_as_ref)) = &from_src
                && name.text() == "from"
            {
                let at = self.range(name.syntax()).0;
                let impls = self.adts.from_impls.entry(type_name.clone()).or_default();
                let key = format!("from${}", impls.len());
                impls.push(crate::adt::FromImpl {
                    src: src.clone(),
                    key: key.clone(),
                    at,
                    via_into: *via_into,
                    via_as_ref: *via_as_ref,
                });
                self.adts
                    .methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert(key, method);
                continue;
            }
            // (`TryFrom<&str>` / `Extend<&'a T>`: how the argument is written, to tell it from a
            // `TryFrom<String>` / `Extend<T>` impl at a call.)
            let written = match trait_name.as_deref() {
                Some("TryFrom" | "FromIterator" | "Extend") => imp
                    .trait_()
                    .and_then(|t| match t {
                        ast::Type::PathType(p) => p.path()?.segment()?.generic_arg_list(),
                        _ => None,
                    })
                    .and_then(|l| l.generic_args().next())
                    .and_then(|g| match g {
                        ast::GenericArg::TypeArg(ta) => ta.ty(),
                        _ => None,
                    })
                    .map(|t| crate::adt::written_arg_ref(&t)),
                _ => None,
            };
            // One trait for both `str` and `String` (`[T]` and `Vec<T>`), one type in JS: the
            // owned impl's methods under `m$own`, picked at each call by how the receiver is
            // written.
            if let Some(tr) = trait_name.as_deref()
                && matches!(type_name.as_str(), "String" | "Vec")
                && self.adts.traits.contains_key(tr)
            {
                match crate::adt::twin_side(imp) {
                    Some(false) => {
                        self.adts
                            .twin_borrowed
                            .insert((type_name.clone(), tr.to_string()));
                    }
                    Some(true)
                        if self
                            .adts
                            .twin_borrowed
                            .contains(&(type_name.clone(), tr.to_string())) =>
                    {
                        let key = format!("{}$own", name.text());
                        let at = self.range(name.syntax()).0;
                        self.adts.alt_keys_at.insert(at, key.clone());
                        self.adts
                            .own_twins
                            .insert(format!("{type_name}::{}", name.text()));
                        self.adts
                            .methods
                            .entry(type_name.clone())
                            .or_default()
                            .insert(key, method);
                        continue;
                    }
                    _ => {}
                }
            }
            // A generic trait's second impl for this type (`impl IntoS<u16> for Vec<u8>` after
            let foreign_cmp = matches!(trait_name.as_deref(), Some("PartialEq" | "PartialOrd"))
                && foreign_rhs(imp, &type_name);
            // `impl IntoS<u8> for Vec<u8>`, `impl TryFrom<Vec<u8>>` after `impl TryFrom<&str>`):
            // its methods under keys of their own.
            if let Some(tr) = trait_name.as_deref()
                && (matches!(tr, "TryFrom" | "FromIterator" | "Extend")
                    || (matches!(tr, "PartialEq" | "PartialOrd") && foreign_rhs(imp, &type_name))
                    || self
                        .adts
                        .traits
                        .get(tr)
                        .is_some_and(|d| !d.type_params.is_empty()))
                && self
                    .adts
                    .methods
                    .get(&type_name)
                    .is_some_and(|ms| ms.contains_key(name.text().to_string().as_str()))
            {
                let n = name.text().to_string();
                let alts = self
                    .adts
                    .method_alts
                    .entry(format!("{type_name}::{n}"))
                    .or_insert_with(|| vec![n.clone()]);
                let key = format!("{n}${}", alts.len());
                alts.push(key.clone());
                if foreign_cmp {
                    self.adts.foreign_cmp.insert(format!("{type_name}::{key}"));
                }
                let at = self.range(name.syntax()).0;
                self.adts.alt_keys_at.insert(at, key.clone());
                if let Some(w) = written {
                    self.adts
                        .alt_arg_refs
                        .insert(format!("{type_name}::{key}"), w);
                }
                self.adts
                    .methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert(key, method);
                continue;
            }
            if let Some(w) = written {
                self.adts
                    .alt_arg_refs
                    .insert(format!("{type_name}::{}", name.text()), w);
            }
            // One trait for another tuple of this length (`impl Show for (String, String)` next to
            // `impl Show for (u8, u8)`): its methods under keys of their own, picked by the
            // receiver's element types (and at run time by the elements' classes or values).
            if let Some(tr) = trait_name.as_deref()
                && crate::adt::array_alts_key(&type_name)
                && self.adts.traits.contains_key(tr)
                && let Some(existing) = self.adts.methods.get(&type_name)
                && existing.contains_key(name.text().to_string().as_str())
            {
                let n = name.text().to_string();
                let keys = self
                    .adts
                    .method_alts
                    .get(&format!("{type_name}::{n}"))
                    .cloned()
                    .unwrap_or_else(|| vec![n.clone()]);
                if crate::adt::self_bounded_impl(imp, tr) {
                    let k = crate::adt::self_bounded_key(&type_name, tr, &impl_args);
                    self.adts.self_bounded.insert(k);
                }
                let existing = self.adts.methods.get(&type_name).cloned().unwrap_or_default();
                let disjoint = keys.iter().all(|k| {
                    existing.get(k).is_some_and(|m| {
                        crate::adt::separable_args(&self.adts, tr, &type_name, &m.impl_args, &impl_args)
                    })
                });
                if disjoint {
                    let alts = self
                        .adts
                        .method_alts
                        .entry(format!("{type_name}::{n}"))
                        .or_insert_with(|| vec![n.clone()]);
                    let key = format!("{n}${}", alts.len());
                    alts.push(key.clone());
                    let at = self.range(name.syntax()).0;
                    self.adts.alt_keys_at.insert(at, key.clone());
                    self.adts.inherent_alts.insert(format!("{type_name}::{n}"));
                    self.adts
                        .methods
                        .entry(type_name.clone())
                        .or_default()
                        .insert(key, method);
                    continue;
                }
            }
            // A second user trait with a method of this name (`impl UsernameWidget for Form {
            // fn get }` next to `impl AgeWidget for Form { fn get }`), or one an inherent method
            // has: under `Trait$method`, what `Trait::method(&x)` calls (the plain name stays
            // the first one's, or the inherent one's).
            if let Some(tr) = trait_name.as_deref()
                && self.adts.traits.contains_key(tr)
                && self
                    .adts
                    .methods
                    .get(&type_name)
                    .is_some_and(|ms| ms.contains_key(name.text().to_string().as_str()))
            {
                let key = format!("{tr}${}", name.text());
                let at = self.range(name.syntax()).0;
                self.adts.alt_keys_at.insert(at, key.clone());
                self.adts
                    .methods
                    .entry(type_name.clone())
                    .or_default()
                    .insert(key, method);
                continue;
            }
            // An inherent impl for another instantiation (`impl W<String> { fn get }` next to
            // `impl W<i32> { fn get }`): its methods under keys of their own, picked by the
            // receiver's type arguments.
            if trait_name.is_none()
                && !impl_args.is_empty()
                && let Some(existing) = self.adts.methods.get(&type_name)
                && existing.contains_key(name.text().to_string().as_str())
            {
                let n = name.text().to_string();
                let keys = self
                    .adts
                    .method_alts
                    .get(&format!("{type_name}::{n}"))
                    .cloned()
                    .unwrap_or_else(|| vec![n.clone()]);
                let disjoint = keys.iter().all(|k| {
                    existing.get(k).is_some_and(|m| {
                        m.impl_args.len() == impl_args.len()
                            && m.impl_args
                                .iter()
                                .zip(&impl_args)
                                .any(|(a, b)| crate::adt::disjoint_args(a, b))
                    })
                });
                if disjoint {
                    let alts = self
                        .adts
                        .method_alts
                        .entry(format!("{type_name}::{n}"))
                        .or_insert_with(|| vec![n.clone()]);
                    let key = format!("{n}${}", alts.len());
                    alts.push(key.clone());
                    let at = self.range(name.syntax()).0;
                    self.adts.alt_keys_at.insert(at, key.clone());
                    self.adts.inherent_alts.insert(format!("{type_name}::{n}"));
                    self.adts
                        .methods
                        .entry(type_name.clone())
                        .or_default()
                        .insert(key, method);
                    continue;
                }
                // `impl<L, R> Either<&L, &R> { fn cloned }` next to `Either<&mut L, &mut R>`:
                // references are erased, so the two are one instantiation here and the first
                // serves.
                let written_ref = imp.self_ty().is_some_and(|t| {
                    t.syntax()
                        .descendants()
                        .any(|d| ast::RefType::can_cast(d.kind()))
                });
                if written_ref
                    && keys
                        .iter()
                        .any(|k| existing.get(k).is_some_and(|m| m.impl_args == impl_args))
                {
                    continue;
                }
            }
            let key = format!("{type_name}::{}", name.text());
            if unknown_trait {
                let methods = self.adts.methods.entry(type_name.clone()).or_default();
                if !methods.contains_key(name.text()) {
                    methods.insert(name.text().to_string(), method);
                    self.adts.unknown_trait_methods.insert(key);
                }
                continue;
            }
            let guessed = self.adts.unknown_trait_methods.remove(&key);
            let methods = self.adts.methods.entry(type_name.clone()).or_default();
            if trait_name.is_none()
                && methods.contains_key(name.text().to_string().as_str())
                && !guessed
            {
                self.error(
                    name.syntax(),
                    format!("duplicate definitions with name `{}`", name.text()),
                );
                continue;
            }
            if foreign_cmp {
                self.adts.foreign_cmp.insert(key);
            }
            // Trait methods (`fmt`, `default`) live alongside inherent ones under their own name.
            methods.insert(name.text().to_string(), method);
        }
        self.self_ty = saved;
        self.assoc_output = saved_output;
        self.impl_assoc = saved_assoc;
        self.generics.clear();
        self.param_bounds.clear();
        self.param_trait_args.clear();
        self.param_trait_alts.clear();
        self.op_args.clear();
        self.cmp_args.clear();
        self.lenient_projs.clear();
        self.const_tys.clear();
        self.from_args.clear();
        self.try_from_args.clear();
        self.borrow_args.clear();
    }

    /// A method's signature (without `self`), with `Self` lowered per `self.self_ty`.
    pub(super) fn lower_method(&mut self, f: &ast::Fn) -> Method {
        // The method's own type parameters join the impl's while its signature is lowered.
        let own = self.enter_method_generics(f);
        let receiver = f.param_list().and_then(|pl| pl.self_param()).map(|sp| {
            // (`self: Pin<&mut Self>`, a future's `poll`: as `&mut self`.)
            let typed = sp.ty().map(|t| t.syntax().text().to_string());
            if let Some(t) = typed.filter(|t| t.contains('&')) {
                if t.contains("&mut") {
                    Receiver::MutRef
                } else {
                    Receiver::Ref
                }
            } else if sp.amp_token().is_some() {
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
                        (matches!(p.ty(), Some(ast::Type::RefType(r)) if r.mut_token().is_some())
                            || p.ty().is_some_and(|t| super::is_ptr_type(&t)))
                            && self.cell_param(t)
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ret = f
            .ret_type()
            .and_then(|r| r.ty())
            .map(|t| self.lower_ret_type(&t))
            .unwrap_or(Ty::Unit);
        let ref_params = f
            .param_list()
            .map(|pl| pl.params().map(|p| p.ty().as_ref().and_then(super::written_ref)).collect())
            .unwrap_or_default();
        let ret_owned = f
            .ret_type()
            .and_then(|r| r.ty())
            .is_some_and(|t| super::written_owned(&t) && !matches!(ret, Ty::Param(_) | Ty::Dyn(_, _) | Ty::Unknown));
        let ret_mut_ref = f.ret_type().and_then(|r| r.ty()).is_some_and(|t| super::written_mut_ref(&t));
        let ret_slot = self.ret_slot_kind(f, &ret);
        if let Some(Ty::Adt(adt, _)) = self.self_ty.clone()
            && let Some(n) = f.name()
        {
            self.note_ret_item_slots(f, format!("{adt}::{n}"));
        }
        let opt_slot_params = self.opt_slot_params(f, &params);
        let method = Method {
            params,
            ret,
            is_async: f.async_token().is_some(),
            receiver,
            cell_params,
            into_params: super::into_param_flags(f),
            impl_args: vec![],
            type_params: own.clone(),
            ref_params,
            ret_owned,
            ret_mut_ref,
            ret_slot,
            opt_slot_params,
            cell_self: false,
            trait_params: vec![],
        };
        for p in &own {
            self.generics.remove(p);
            self.param_bounds.remove(p);
            self.const_tys.remove(p);
        }
        self.leave_method_where();
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
            if let Some(key) = crate::adt::ext_key_any(&type_name)
                && !self.adts.defs.contains_key(&type_name)
                && self.adts.ext_types.contains(&key)
            {
                type_name = key;
            }
            if let Some((tr, _, _)) = crate::adt::blanket_impl(imp)
                && self.adts.blankets.contains_key(&tr)
            {
                type_name = format!("{tr}$all");
            }
            if let Some((key, _)) = crate::adt::wrapper_impl(imp)
                && self.adts.ext_types.contains(&key)
            {
                type_name = key;
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
                            | "Future"
                            | "FromStr"
                            | "TryFrom"
                            | "FromIterator"
                            | "Extend"
                            | "Hash"
                            | "Sum"
                            | "Product"
                            | "DerefMut"
                            // (Their methods are ordinary ones: checked like the rest.)
                            | "Deref"
                            | "AsRef"
                            | "AsMut"
                            | "Borrow"
                            | "BorrowMut"
                            | "Drop"
                            | "DoubleEndedIterator"
                            | "ExactSizeIterator"
                            | "IntoIterator"
                            | "Hasher"
                            | "BuildHasher"
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
                        // (`impl<const P: u128> U<P> { const PER: u128 = P * 1000; }`)
                        for (n, ty) in self.const_tys.clone() {
                            let local = Local {
                                ty,
                                mutable: false,
                                deferred: false,
                                reference: None,
                                owned: false,
                                js: false,
                                decl: None,
                            };
                            self.scopes[0].insert(n, local);
                        }
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
                            owned: false,
                            js: false,
                            decl: None,
                        };
                        Some((n, local))
                    });
                let own = self.enter_method_generics(&f);
                // (An associated function takes its `impl`'s const parameters too; a method
                // reads them off `self`.)
                let impl_consts: Vec<String> = if m.receiver.is_none() {
                    imp.generic_param_list()
                        .into_iter()
                        .flat_map(|l| l.generic_params())
                        .filter_map(|p| match p {
                            ast::GenericParam::ConstParam(cp) => {
                                Some(cp.name()?.text().to_string())
                            }
                            _ => None,
                        })
                        .filter(|n| m.impl_args.contains(&Ty::Param(n.clone())))
                        .collect()
                } else {
                    Vec::new()
                };
                self.note_const_params(&format!("{type_name}::{key}"), &f, impl_consts);
                let saved_fn = self.cur_fn.replace(format!("{type_name}::{key}"));
                self.check_body(&f, m.params.clone(), m.ret.clone(), m.is_async, self_local);
                self.cur_fn = saved_fn;
                // A blanket impl's `*self = ..` / `*self += ..`: `self` may be a number or a
                // string, so the method takes the receiver's cell (`Method::cell_self`).
                if type_name.ends_with("$all")
                    && m.receiver == Some(crate::adt::Receiver::MutRef)
                    && let Some(body) = f.body()
                {
                    for e in body.syntax().descendants().filter_map(ast::BinExpr::cast) {
                        let assigns = matches!(e.op_kind(), Some(ast::BinaryOp::Assignment { .. }));
                        let to_self = matches!(e.lhs(), Some(ast::Expr::PrefixExpr(p))
                            if p.op_kind() == Some(ast::UnaryOp::Deref)
                                && matches!(p.expr(), Some(ast::Expr::PathExpr(s)) if s.syntax().text() == "self"));
                        if assigns && to_self {
                            if let Some(sig) = self
                                .adts
                                .methods
                                .get_mut(type_name.as_str())
                                .and_then(|ms| ms.get_mut(key.as_str()))
                            {
                                sig.cell_self = true;
                            }
                        }
                    }
                }
                for p in own {
                    self.generics.remove(&p);
                    self.param_bounds.remove(&p);
                    self.const_tys.remove(&p);
                }
                self.leave_method_where();
            }
            self.self_ty = saved;
            self.generics.clear();
            self.param_bounds.clear();
            self.param_trait_args.clear();
            self.param_trait_alts.clear();
            self.op_args.clear();
            self.cmp_args.clear();
            self.lenient_projs.clear();
            self.const_tys.clear();
            self.from_args.clear();
            self.try_from_args.clear();
            self.borrow_args.clear();
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

    /// A generic struct's or enum's construction, for the classes its values may carry.
    fn note_ctor(&mut self, adt: &str, targs: &[Ty]) {
        if !targs.is_empty() {
            self.ctor_sites
                .push((self.cur_fn.clone(), adt.to_string(), targs.to_vec()));
        }
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
                self.ctor_turbofish(p.path(), &args);
                if self.adts.const_params.contains_key(&adt) {
                    let r = self.range(p.syntax());
                    self.const_lits.push((r, adt.clone(), args.clone()));
                }
                self.note_ctor(&adt, &args);
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
                // (`let mut a = Self::QS;`: JS would share the item's array.)
                if self.copy_array_assoc.contains(&(adt.clone(), m.clone())) {
                    let r = self.range(p.syntax());
                    self.array_reads.insert(r);
                }
                let t = self.adts.assoc_consts[&adt][&m].clone();
                // (A generic impl's `const ZERO: Self`: `W<T>` with `T` fresh, or as written,
                // `W::<f64>::ZERO`.)
                let mut ps = Vec::new();
                Self::params_in(&t, &mut ps);
                ps.retain(|p| !self.generics.contains_key(p) && p != "Self");
                if ps.is_empty() {
                    return Some(t);
                }
                let m: HashMap<String, Ty> =
                    ps.iter().map(|p| (p.clone(), self.fresh(VarKind::General))).collect();
                let t = super::generic::subst(&t, &m);
                if let Ty::Adt(a, args) = &t
                    && *a == adt
                {
                    use ra_ap_syntax::ast::HasGenericArgs;
                    let written: Vec<ast::Type> = p
                        .path()
                        .into_iter()
                        .flat_map(|p| p.segments().collect::<Vec<_>>())
                        .find(|s| s.name_ref().is_some_and(|n| n.text() == adt.as_str()))
                        .and_then(|s| s.generic_arg_list())
                        .into_iter()
                        .flat_map(|l| l.generic_args())
                        .filter_map(|g| match g {
                            ast::GenericArg::TypeArg(ta) => ta.ty(),
                            _ => None,
                        })
                        .collect();
                    for (a, w) in args.clone().iter().zip(&written) {
                        let w = self.lower_type(w);
                        self.unify(a, &w);
                    }
                }
                Some(t)
            }
            Resolved::Assoc(adt, m) => match self.adts.method(&adt, &m).cloned() {
                Some(method) => {
                    let (own_count, dict_count) =
                        (method.type_params.len(), method.dict_names().len());
                    let impl_args = method.impl_args.clone();
                    let method = self.method_inst(&method, None);
                    // As a value (`parse_with(Stack::<T>::make)`): the classes go with it.
                    if dict_count > 0 {
                        let own = std::mem::take(&mut self.last_type_args);
                        self.dict_turbofish(p, &own, own_count, &impl_args);
                        let r = self.range(p.syntax());
                        let caller = self.cur_fn.clone().unwrap_or_default();
                        self.generic_calls
                            .push((r, caller, format!("{adt}::{m}"), own));
                    }
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

    /// Type arguments written on an associated function's path: `Type::<A>::f::<B>`. `vars`
    /// holds the function's own type variables, then its `impl`'s (by `impl_args` order).
    fn dict_turbofish(&mut self, p: &ast::PathExpr, vars: &[Ty], own: usize, impl_args: &[Ty]) {
        let args_of = |seg: Option<ast::PathSegment>| -> Vec<ast::GenericArg> {
            seg.and_then(|s| s.generic_arg_list())
                .map(|l| {
                    l.generic_args()
                        .filter(|g| {
                            matches!(
                                g,
                                ast::GenericArg::TypeArg(_) | ast::GenericArg::ConstArg(_)
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        let Some(path) = p.path() else { return };
        for (v, g) in vars.iter().take(own).zip(args_of(path.segment())) {
            if let Some(t) = self.generic_arg_ty(&g) {
                self.unify(v, &t);
            }
        }
        // `Punctuated::<Ident, Comma>::f`: the type's arguments, for the `impl`'s parameters.
        let qual = path.qualifier().and_then(|q| q.segment());
        // `Self::f()`: the impl's parameters are the enclosing impl's (`T` for `T`).
        let self_args = qual
            .as_ref()
            .filter(|s| s.self_type_token().is_some())
            .and_then(|_| {
                let (ty, m) = self.cur_fn.as_deref()?.split_once("::")?;
                Some(self.adts.method(ty, m)?.impl_args.clone())
            });
        if let Some(cur) = self_args {
            let mut k = own;
            let mut seen: Vec<String> = Vec::new();
            for (i, ia) in impl_args.iter().enumerate() {
                if let Ty::Param(u) = ia
                    && !seen.contains(u)
                {
                    seen.push(u.clone());
                    if let (Some(v), Some(t)) = (vars.get(k), cur.get(i)) {
                        let t = t.clone();
                        self.unify(v, &t);
                    }
                    k += 1;
                }
            }
            return;
        }
        let type_args = args_of(qual);
        let mut k = own;
        let mut seen: Vec<String> = Vec::new();
        for (i, ia) in impl_args.iter().enumerate() {
            if let Ty::Param(u) = ia
                && !seen.contains(u)
            {
                seen.push(u.clone());
                if let (Some(v), Some(g)) = (vars.get(k), type_args.get(i))
                    && let Some(t) = self.generic_arg_ty(g)
                {
                    self.unify(v, &t);
                }
                k += 1;
            }
        }
    }

    /// `Tag::<7>`, `Buf::<u8, 4> { .. }`: a struct path's own arguments.
    fn ctor_turbofish(&mut self, path: Option<ast::Path>, targs: &[Ty]) {
        // (On the variant, or on its enum: `Buf::<2>::Empty`.)
        let given: Vec<ast::GenericArg> = path
            .and_then(|p| {
                p.segment()
                    .and_then(|s| s.generic_arg_list())
                    .or_else(|| p.qualifier()?.segment()?.generic_arg_list())
            })
            .map(|l| l.generic_args().collect())
            .unwrap_or_default();
        let given: Vec<Ty> = given
            .iter()
            .filter_map(|g| self.generic_arg_ty(g))
            .collect();
        for (v, t) in targs.iter().zip(given) {
            self.unify(v, &t);
        }
    }

    fn no_assoc(&mut self, node: &SyntaxNode, adt: &str, m: &str) {
        let kind = if self.adts.defs.get(adt).is_some_and(|d| d.is_enum) {
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

    /// The key of `adt::m` among inherent impls for several instantiations, by the path's type
    /// arguments (`W::<i32>::kind()`) or the impl's own (`Self::kind()`); recorded for codegen.
    fn pick_inherent_assoc(&mut self, c: &ast::CallExpr, adt: &str, m: &str) -> String {
        let Some(keys) = self.adts.method_alts.get(&format!("{adt}::{m}")).cloned() else {
            return m.to_string();
        };
        let qual = match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p.path().and_then(|p| p.qualifier()),
            _ => None,
        };
        let written: Vec<ast::GenericArg> = qual
            .as_ref()
            .and_then(|q| q.segment()?.generic_arg_list())
            .map(|l| l.generic_args().collect())
            .unwrap_or_default();
        let targs: Vec<Ty> = if !written.is_empty() {
            written
                .iter()
                .map(|g| self.generic_arg_ty(g).unwrap_or(Ty::Unknown))
                .collect()
        } else if qual.as_ref().is_some_and(|q| q.syntax().text() == "Self")
            && let Some(Ty::Adt(_, a)) = self.self_ty.clone()
        {
            a
        } else {
            // (rustc resolves `W::get` before looking at the arguments: every impl applies.)
            self.error(
                c.syntax(),
                format!(
                    "multiple applicable items in scope: `{m}` is defined in `impl`s for several instantiations of `{adt}`; name the type arguments (`{adt}::<..>::{m}`)"
                ),
            );
            Vec::new()
        };
        let key = self.alts_for_args(adt, keys, &targs).swap_remove(0);
        let r = self.range(c.syntax());
        self.method_keys.insert(r, key.clone());
        key
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
                if let Some(ast::Expr::PathExpr(p)) = c.expr() {
                    self.ctor_turbofish(p.path(), &targs);
                }
                if self.adts.const_params.contains_key(&adt) {
                    let r = self.range(c.syntax());
                    self.const_lits.push((r, adt.clone(), targs.clone()));
                }
                self.note_ctor(&adt, &targs);
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
                    // `Slot::Live(&mut a)` with `Live(&'a mut i32)`: the field keeps the slot.
                    if self
                        .mut_ref_fields
                        .contains(&(variant.name.clone(), f.name.clone()))
                    {
                        self.keep_slot(a);
                    }
                }
                Some(Ty::Adt(adt, targs))
            }
            Resolved::Assoc(adt, m) => {
                if m == "from"
                    && self.adts.method(&adt, &m).is_none()
                    && (self.adts.from_impls.contains_key(&adt) || args.len() == 1)
                {
                    return Some(self.conv_from_call(c, &adt, args));
                }
                // `W::<i32>::kind()` / `Self::kind()` with inherent impls for several
                // instantiations: the one for those type arguments.
                let m = if self.adts.inherent_alts.contains(&format!("{adt}::{m}")) {
                    self.pick_inherent_assoc(c, &adt, &m)
                } else {
                    m
                };
                // `Name::try_from(bytes)` with several `impl TryFrom<X> for Name`: the one for
                // the argument.
                let m = if let [a] = args
                    && self.adts.method_alts.contains_key(&format!("{adt}::{m}"))
                {
                    let at = self.infer(a, None);
                    self.pick_alt_by_item(&adt, &m, 0, &at, c.syntax())
                } else {
                    m
                };
                let Some(method) = self.adts.method(&adt, &m).cloned() else {
                    self.no_assoc(c.syntax(), &adt, &m);
                    for a in args {
                        self.infer(a, None);
                    }
                    return Some(Ty::Unknown);
                };
                let (own_count, dict_count, impl_args) = self
                    .adts
                    .method(&adt, &m)
                    .map(|x| {
                        (
                            x.type_params.len(),
                            x.dict_names().len(),
                            x.impl_args.clone(),
                        )
                    })
                    .unwrap_or_default();
                let method = self.method_inst(&method, None);
                // (`Auto::always(Vec::new())` with `struct Auto<S: RawStream>`.)
                if method.impl_args.iter().any(|a| matches!(a, Ty::Var(_))) {
                    let args = method.impl_args.clone();
                    self.note_adt_bounds(&adt, &args);
                }
                if dict_count > 0 {
                    let own = std::mem::take(&mut self.last_type_args);
                    if let Some(ast::Expr::PathExpr(p)) = c.expr() {
                        self.dict_turbofish(&p, &own, own_count, &impl_args);
                    }
                    let r = self.range(c.syntax());
                    let caller = self.cur_fn.clone().unwrap_or_default();
                    self.generic_calls
                        .push((r, caller, format!("{adt}::{m}"), own));
                }
                let mut params = method.params.clone();
                let mut into_params = method.into_params.clone();
                let mut ref_params = method.ref_params.clone();
                let mut cell_params = method.cell_params.clone();
                if method.receiver.is_some() {
                    params.insert(0, Ty::Adt(adt.clone(), method.impl_args.clone()));
                    into_params.insert(0, false);
                    ref_params.insert(
                        0,
                        match method.receiver {
                            Some(Receiver::Ref) => Some(format!("&{adt}")),
                            Some(Receiver::MutRef) => Some(format!("&mut {adt}")),
                            _ => None,
                        },
                    );
                    cell_params.insert(0, false);
                    if method.receiver == Some(Receiver::MutRef)
                        && let Some(first) = args.first()
                    {
                        self.check_mutable(first, Mutation::Borrow);
                    }
                }
                self.arg_count(c.syntax(), "function", params.len(), args.len());
                for (i, (a, p)) in args.iter().zip(&params).enumerate() {
                    if into_params.get(i) == Some(&true) {
                        self.check_into_arg(a, p);
                    } else {
                        let before = self.diags.len();
                        self.check(a, p);
                        if self.diags.len() == before
                            && cell_params.get(i) != Some(&true)
                            && let Some(Some(shown)) = ref_params.get(i)
                        {
                            self.check_ref_arg(a, shown);
                        }
                    }
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
            // `RunOptions { .. }`: an interface of the NS bindings.
            if let Some(t) = self.iface_record(r, &segs) {
                return t;
            }
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
        self.ctor_turbofish(r.path(), &targs);
        if self.adts.const_params.contains_key(&adt) {
            let r = self.range(r.syntax());
            self.const_lits.push((r, adt.clone(), targs.clone()));
        }
        self.note_ctor(&adt, &targs);
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
                        // `W { n: &mut s.count }` with `n: &mut i32`: the field keeps the slot.
                        if self
                            .mut_ref_fields
                            .contains(&(adt.clone(), fname.text().to_string()))
                        {
                            self.keep_slot(&e);
                        }
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
        // `w.sign` through `impl Deref for Wrap { type Target = Span; }`: `(w).deref().sign`,
        // through as many `Deref`s as it takes (`(w).deref().deref().sign`).
        if let Some(recv) = f.expr() {
            let mut steps = Vec::new();
            let mut cur = Ty::Adt(adt.to_string(), targs.to_vec());
            while steps.len() < 8
                && let Some((n, target)) = self.deref_target(&cur)
            {
                if let Ty::Adt(ta, ta_args) = self.shallow(&target)
                    && self
                        .adts
                        .defs
                        .get(&ta)
                        .is_some_and(|d| !d.is_enum && d.variants[0].field(&key).is_some())
                {
                    let r = self.range(recv.syntax());
                    if steps.is_empty() {
                        self.deref_steps.remove(&r);
                    } else {
                        self.deref_steps.insert(r, steps);
                    }
                    self.auto_deref(&recv, &n, &target);
                    return self.adt_field(f, &ta, &ta_args);
                }
                steps.push(self.adts.deref_mut.contains(&n));
                cur = target;
            }
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
            self.note_trait_call(m, adt, method, &Ty::Adt(adt.to_string(), targs.to_vec()));
            let own_params = sig.dict_names().len();
            let declared = sig.ret.clone();
            let raw_params = sig.params.clone();
            let sig = self.method_inst(&sig, Some(targs));
            if own_params > 0 {
                let own = std::mem::take(&mut self.last_type_args);
                // `x.parse_args::<LitStr>()`
                if let Some(gal) = m.generic_arg_list() {
                    let given: Vec<Ty> = gal
                        .generic_args()
                        .collect::<Vec<_>>()
                        .iter()
                        .filter_map(|g| self.generic_arg_ty(g))
                        .collect();
                    for (v, t) in own.iter().zip(given) {
                        self.unify(v, &t);
                    }
                }
                let r = self.range(m.syntax());
                let caller = self.cur_fn.clone().unwrap_or_default();
                self.note_twin_method_targs(r, adt, method, &raw_params, args);
                self.generic_calls
                    .push((r, caller, format!("{adt}::{method}"), own));
            }
            if sig.receiver == Some(Receiver::MutRef) {
                self.check_mutable(recv, Mutation::Borrow);
            }
            self.arg_count(m.syntax(), "method", sig.params.len(), args.len());
            for (i, (a, p)) in args.iter().zip(&sig.params).enumerate() {
                if sig.cell_params.get(i) == Some(&true) {
                    self.cell_arg(a);
                }
                if sig.into_params.get(i) == Some(&true) {
                    self.check_into_arg(a, p);
                } else {
                    let before = self.diags.len();
                    self.check(a, p);
                    // `p.dist(q)` with `q: Point` where the parameter is `&Point`: E0308.
                    if self.diags.len() == before
                        && sig.cell_params.get(i) != Some(&true)
                        && let Some(Some(shown)) = sig.ref_params.get(i)
                    {
                        self.check_ref_arg(a, shown);
                    }
                }
            }
            for a in args.iter().skip(sig.params.len()) {
                self.infer(a, None);
            }
            // `*s.at(i) += 1` with `fn at(&mut self, i: usize) -> &mut i32`: the call is a slot.
            let call = ast::Expr::MethodCallExpr(m.clone());
            self.note_call_slot(&call, sig.ret_slot, &declared, &sig.ret);
            self.finish_slot_args(args, &sig.opt_slot_params);
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
        // `self.inner.fmt(f)` of a derived `Debug`, your own `Hasher`'s `write_u32`, ..
        if let Some(r) = self.gaps_method(m, recv, &t, method, args) {
            return r;
        }
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
            "to_string" if self.adts.defs.get(adt).is_some_and(|d| d.display) => {
                self.arg_count(m.syntax(), "method", 0, args.len());
                Ty::Str
            }
            _ => {
                for a in args {
                    self.infer(a, None);
                }
                let kind = if self.adts.defs.get(adt).is_some_and(|d| d.is_enum) {
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
            if let ast::Pat::RecordPat(rp) = p
                && let Some(k) = super::range_struct_pat(&segs)
            {
                self.range_record_pat(rp, k, expected);
                return true;
            }
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
                    // rustc's caret is on the pattern's fields.
                    let at = match (fields.first(), fields.last()) {
                        (Some(a), Some(b)) => (self.range(a.syntax()).0, self.range(b.syntax()).1),
                        _ => self.range(p.syntax()),
                    };
                    let what = if name.contains("::") { "tuple variant" } else { "tuple struct" };
                    self.error_spans(
                        at,
                        format!(
                            "this pattern has {named} field{}, but the corresponding {what} has {n} field{}",
                            s(named),
                            s(n)
                        ),
                        None,
                        Vec::new(),
                    );
                    // (Its names are still bound, so their uses don't report again.)
                    for f in &fields {
                        self.pat(f, &Ty::Unknown);
                    }
                    return true;
                }
                for (i, f) in fields.iter().enumerate() {
                    let index = match rest {
                        Some(r) if i == r => continue,
                        Some(r) if i > r => n - (fields.len() - i),
                        _ => i,
                    };
                    self.pat(f, &variant.fields[index].ty);
                    // `Slot::Live(r)` with `Live(&'a mut i32)`: `r` binds the field's slot.
                    if self
                        .mut_ref_fields
                        .contains(&(variant.name.clone(), variant.fields[index].name.clone()))
                    {
                        self.bind_slot_shape(f, &super::slots::SlotShape::Slot);
                    }
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
                    if let Some(n) = s.name() {
                        if s.mut_token().is_some() {
                            self.static_muts.insert(n.text().to_string());
                        } else {
                            self.statics.insert(n.text().to_string());
                        }
                    }
                    (s.name(), s.ty(), s.body(), s.syntax().clone())
                }
                _ => continue,
            };
            let Some(name) = name else { continue };
            let ty = match ty {
                Some(t) => {
                    let lowered = self.lower_type(&t);
                    if self.copy_array(Some(&t), None, &lowered) {
                        self.copy_array_consts.insert(name.text().to_string());
                    }
                    lowered
                }
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

    /// A path pattern naming a constant (`S::ZERO`, `Self::MAX`, `<S as Tr>::TOP`, `u8::MAX`,
    /// a `const` as a range bound): compared against, not bound. Its type, and its integer
    /// value when known (for exhaustiveness).
    pub(super) fn const_path_pat(&self, pp: &ast::PathPat) -> Option<(Ty, Option<i128>)> {
        let segs = crate::adt::const_pat_segs(pp.path()?);
        let value = || {
            self.eval_const_segs(pp.syntax(), &segs)
                .and_then(|v| v.int())
        };
        if let [name] = segs.as_slice() {
            let ty = self.consts.get(name)?.clone();
            return Some((ty, value()));
        }
        let tail: Vec<&str> = segs.iter().map(String::as_str).collect();
        if let Some((_, ty)) = crate::ty::std_const(crate::ty::std_tail(&tail)) {
            return Some((ty, value()));
        }
        match self.resolve(&segs)? {
            Resolved::Assoc(adt, m) if self.adts.method(&adt, &m).is_none() => {
                let ty = self.adts.assoc_consts.get(&adt)?.get(&m)?.clone();
                Some((ty, value()))
            }
            _ => None,
        }
    }

    /// `let Range { start, end } = r;`: a std range's public fields (`RangeFrom`'s `start`,
    /// `RangeTo` / `RangeToInclusive`'s `end`).
    fn range_record_pat(&mut self, rp: &ast::RecordPat, k: RangeKind, expected: &Ty) {
        let name = path_segs(rp.path()).join("::");
        let elem = self.fresh(super::VarKind::General);
        let range = Ty::Range(Box::new(elem.clone()), k);
        if !self.unify(expected, &range) {
            let msg = format!(
                "mismatched types: expected `{}`, found `{name}`",
                self.describe(expected)
            );
            self.error(rp.syntax(), msg);
        }
        let fields: &[&str] = match k {
            RangeKind::Range => &["start", "end"],
            RangeKind::From => &["start"],
            _ => &["end"],
        };
        let Some(list) = rp.record_pat_field_list() else { return };
        let mut seen = Vec::new();
        for f in list.fields() {
            let Some(fname) = f.field_name() else { continue };
            let key = fname.text().to_string();
            if !fields.contains(&key.as_str()) {
                self.error(
                    fname.syntax(),
                    format!("`{name}` does not have a field named `{key}`"),
                );
                continue;
            }
            seen.push(key);
            if let Some(sub) = f.pat() {
                self.pat(&sub, &elem);
            }
        }
        let missing: Vec<String> = fields
            .iter()
            .filter(|f| !seen.iter().any(|s| s == *f))
            .map(|f| format!("`{f}`"))
            .collect();
        if list.rest_pat().is_none() && !missing.is_empty() {
            let s = if missing.len() == 1 { "" } else { "s" };
            self.error(
                rp.syntax(),
                format!("pattern does not mention field{s} {} (add `..` to ignore the rest)", join_and(&missing)),
            );
        }
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
            // (`Range { start, end }`)
            return matches!(&p, ast::Pat::RecordPat(rp)
                if super::range_struct_pat(&segs).is_some()
                    && rp
                        .record_pat_field_list()
                        .into_iter()
                        .flat_map(|l| l.fields())
                        .all(|f| f.pat().is_none_or(|sub| self.irrefutable_pat(&sub))));
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
        if self.resolve(&segs).is_none()
            && super::range_struct_pat(&segs).is_some()
            && self.irrefutable_pat(p)
        {
            return Some(vec![Ap::Wild]);
        }
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

/// `Rc<T>` / `Arc<T>` (`std::rc::Rc<T>`).
fn rc_type(t: &ast::Type) -> bool {
    match t {
        ast::Type::ParenType(p) => p.ty().is_some_and(|t| rc_type(&t)),
        ast::Type::PathType(p) => p
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .is_some_and(|n| matches!(&*n.text(), "Rc" | "Arc")),
        _ => false,
    }
}

/// A shared reference type (`&'a [u8]`, `Option<&T>`): `Copy` whatever it points at.
fn shared_ref_type(t: &ast::Type) -> bool {
    match t {
        ast::Type::RefType(r) => r.mut_token().is_none(),
        ast::Type::ParenType(p) => p.ty().is_some_and(|t| shared_ref_type(&t)),
        // `Option<&T>`
        ast::Type::PathType(p) => p
            .path()
            .and_then(|p| p.segment())
            .filter(|s| s.name_ref().is_some_and(|n| n.text() == "Option"))
            .and_then(|s| s.generic_arg_list())
            .and_then(|l| l.generic_args().next())
            .is_some_and(|a| match a {
                ast::GenericArg::TypeArg(ta) => ta.ty().is_some_and(|t| shared_ref_type(&t)),
                _ => false,
            }),
        _ => false,
    }
}

/// `fn(u8) -> u8` or `Option<fn(..)>`: `Copy`, `Clone`, `Debug` and `PartialEq` whatever its
/// signature (a closure type isn't).
fn fn_ptr_type(t: &ast::Type) -> bool {
    match t {
        ast::Type::FnPtrType(_) => true,
        ast::Type::ForType(f) => f.ty().is_some_and(|t| fn_ptr_type(&t)),
        ast::Type::ParenType(p) => p.ty().is_some_and(|t| fn_ptr_type(&t)),
        ast::Type::PathType(p) => p
            .path()
            .and_then(|p| p.segment())
            .filter(|s| s.name_ref().is_some_and(|n| n.text() == "Option"))
            .and_then(|s| s.generic_arg_list())
            .and_then(|l| l.generic_args().next())
            .is_some_and(|a| match a {
                ast::GenericArg::TypeArg(ta) => ta.ty().is_some_and(|t| fn_ptr_type(&t)),
                _ => false,
            }),
        _ => false,
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
pub(super) fn join_and(items: &[String]) -> String {
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
            ast::GenericParam::ConstParam(cp) => cp.name().map(|n| n.text().to_string()),
            _ => None,
        })
        .collect()
}

/// The traits a struct or enum bounds each of its type parameters by (`struct Auto<S: RawStream>`,
/// `where S: RawStream`), by name; const parameters have none.
fn type_param_bounds(item: &ast::Item) -> Vec<Vec<String>> {
    let (gp, wc) = match item {
        ast::Item::Struct(s) => (s.generic_param_list(), s.where_clause()),
        ast::Item::Enum(e) => (e.generic_param_list(), e.where_clause()),
        _ => (None, None),
    };
    let names = |l: ast::TypeBoundList| {
        l.bounds()
            .filter_map(|b| match b.ty() {
                Some(ast::Type::PathType(p)) => {
                    p.path()?.segment()?.name_ref().map(|n| n.text().to_string())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    gp.into_iter()
        .flat_map(|l| l.generic_params())
        .filter_map(|p| match p {
            ast::GenericParam::TypeParam(tp) => {
                let name = tp.name()?.text().to_string();
                let mut out: Vec<String> = tp.type_bound_list().map(names).unwrap_or_default();
                for l in where_clause_bounds(wc.clone(), &name) {
                    out.extend(names(l));
                }
                Some(out)
            }
            ast::GenericParam::ConstParam(_) => Some(Vec::new()),
            _ => None,
        })
        .collect()
}

/// For `impl<S: RawStream> Auto<S>`: (`Auto`, the argument's position, `S`'s bounds' names) for
/// each argument of the self type that is one of the impl's type parameters with bounds.
fn impl_param_bounds(imp: &ast::Impl) -> Vec<(String, usize, Vec<String>)> {
    use ra_ap_syntax::ast::HasGenericArgs;
    let Some(ast::Type::PathType(pt)) = imp.self_ty() else {
        return Vec::new();
    };
    let Some(seg) = pt.path().and_then(|p| p.segment()) else {
        return Vec::new();
    };
    let Some(adt) = seg.name_ref().map(|n| n.text().to_string()) else {
        return Vec::new();
    };
    let params: Vec<ast::TypeParam> = imp
        .generic_param_list()
        .into_iter()
        .flat_map(|l| l.type_or_const_params())
        .filter_map(|p| match p {
            ast::TypeOrConstParam::Type(tp) => Some(tp),
            _ => None,
        })
        .collect();
    let args = seg
        .generic_arg_list()
        .into_iter()
        .flat_map(|l| l.generic_args())
        .filter(|a| !matches!(a, ast::GenericArg::LifetimeArg(_)));
    let mut out = Vec::new();
    for (pos, a) in args.enumerate() {
        let ast::GenericArg::TypeArg(ta) = a else { continue };
        let Some(ast::Type::PathType(at)) = ta.ty() else { continue };
        let Some(n) = at.path().and_then(|p| p.as_single_name_ref()) else { continue };
        let Some(tp) = params.iter().find(|tp| tp.name().is_some_and(|x| x.text() == n.text()))
        else {
            continue;
        };
        let bounds: Vec<String> = tp
            .type_bound_list()
            .into_iter()
            .chain(where_clause_bounds(imp.where_clause(), &n.text()))
            .flat_map(|l| l.bounds())
            .filter_map(|b| match b.ty()? {
                ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
                _ => None,
            })
            .collect();
        if !bounds.is_empty() {
            out.push((adt.clone(), pos, bounds));
        }
    }
    out
}

/// The bound lists a `where` clause gives the type parameter `name` (`where S: Into<String>`).
fn where_bounds(f: &ast::Fn, name: &str) -> Vec<ast::TypeBoundList> {
    where_clause_bounds(f.where_clause(), name)
}

/// The bound lists `wc` gives the type parameter `name`.
fn where_clause_bounds(wc: Option<ast::WhereClause>, name: &str) -> Vec<ast::TypeBoundList> {
    let Some(wc) = wc else {
        return Vec::new();
    };
    wc.predicates()
        .filter(|p| match p.ty() {
            Some(ast::Type::PathType(pt)) => pt
                .path()
                .and_then(|p| p.as_single_name_ref())
                .is_some_and(|n| n.text() == name),
            _ => false,
        })
        .filter_map(|p| p.type_bound_list())
        .collect()
}

/// Whether a bound list's traits take arguments (`FnOnce() -> R`, `Into<T>`,
/// `IntoIterator<Item = P>`), which may name the function's other type parameters.
fn bounds_name_params(l: &ast::TypeBoundList) -> bool {
    l.bounds().any(|b| match b.ty() {
        Some(ast::Type::PathType(p)) => p.path().and_then(|p| p.segment()).is_some_and(|s| {
            ast::HasGenericArgs::generic_arg_list(&s).is_some()
                || s.parenthesized_arg_list().is_some()
                || s.ret_type().is_some()
        }),
        _ => false,
    })
}

/// The value of a constant integer expression, for enum discriminants (see `consteval`).
fn const_eval(file: &ast::SourceFile, e: &ast::Expr, _depth: u32) -> Option<i128> {
    match crate::consteval::eval(file, &crate::consteval::Known::default(), e)? {
        crate::consteval::CVal::Int(n) => Some(n),
        crate::consteval::CVal::Bool(_) | crate::consteval::CVal::U128(_) => None,
    }
}

/// The impl's type parameters only its trait's arguments name. Where some trait argument is
/// just one of them (`E` of `impl<E: Default> Tr<E> for (Ix, Ix)`), they come first, one per
/// trait argument up to the last such, a `_{i}` placeholder for the others: a call through a
/// bound hands the trait arguments' classes by position (see `finish_needs`). Then the rest
/// (`C` and `D` of `Equivalent<(C, D)>`).
fn trait_only_params(imp: &ast::Impl) -> Vec<String> {
    let (Some(tr), Some(own)) = (imp.trait_(), imp.self_ty()) else {
        return Vec::new();
    };
    let names = |n: &SyntaxNode| -> Vec<String> {
        n.descendants()
            .filter_map(ast::NameRef::cast)
            .map(|r| r.text().to_string())
            .collect()
    };
    let (in_trait, in_own) = (names(tr.syntax()), names(own.syntax()));
    let only: Vec<String> = imp
        .generic_param_list()
        .into_iter()
        .flat_map(|l| l.type_or_const_params())
        .filter_map(|p| match p {
            ast::TypeOrConstParam::Type(tp) => tp.name().map(|n| n.text().to_string()),
            ast::TypeOrConstParam::Const(_) => None,
        })
        .filter(|n| in_trait.contains(n) && !in_own.contains(n))
        .collect();
    use ra_ap_syntax::ast::HasGenericArgs;
    let targs: Vec<Option<String>> = match &tr {
        ast::Type::PathType(p) => p
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.generic_arg_list())
            .into_iter()
            .flat_map(|l| l.generic_args())
            .filter_map(|g| match g {
                ast::GenericArg::TypeArg(t) => Some(match t.ty() {
                    Some(ast::Type::PathType(p)) => p
                        .path()
                        .filter(|p| p.qualifier().is_none())
                        .and_then(|p| p.segment())
                        .filter(|s| s.generic_arg_list().is_none())
                        .and_then(|s| s.name_ref())
                        .map(|n| n.text().to_string())
                        .filter(|n| only.contains(n)),
                    _ => None,
                }),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let last = targs.iter().rposition(Option::is_some);
    let mut out: Vec<String> = match last {
        Some(l) => targs[..=l]
            .iter()
            .enumerate()
            .map(|(i, n)| n.clone().unwrap_or_else(|| format!("_{i}")))
            .collect(),
        None => Vec::new(),
    };
    for n in only {
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// Whether an `impl PartialEq<R> for T` (or `PartialOrd<R>`) compares `T` with another type:
/// `R` is written and is neither `Self` nor `T`.
fn foreign_rhs(imp: &ast::Impl, type_name: &str) -> bool {
    let Some(ast::Type::PathType(p)) = imp.trait_() else {
        return false;
    };
    let Some(rhs) = p
        .path()
        .and_then(|p| p.segment())
        .and_then(|s| s.generic_arg_list())
        .and_then(|l| l.generic_args().next())
    else {
        return false;
    };
    let text = rhs.syntax().text().to_string();
    let base = text.split('<').next().unwrap_or("").trim();
    let base = base.rsplit("::").next().unwrap_or(base);
    let own = imp
        .self_ty()
        .map(|t| t.syntax().text().to_string())
        .unwrap_or_default();
    let own = own.split('<').next().unwrap_or("").trim().to_string();
    let own = own.rsplit("::").next().unwrap_or(&own);
    base != "Self" && base != own && base != type_name
}

/// The std traits `collect_impl` knows, by name (the `std::ops` operators too).
fn impl_std_trait(t: &str) -> bool {
    crate::adt::op_method(t).is_some()
        || matches!(
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
                | "Sum"
                | "Product"
                | "Iterator"
                | "IntoIterator"
                | "Send"
                | "Sync"
                | "Unpin"
                | "Future"
                | "ExactSizeIterator"
                | "FusedIterator"
                | "DoubleEndedIterator"
                | "TrustedLen"
                | "UnwindSafe"
                | "RefUnwindSafe"
                | "Hasher"
                | "BuildHasher"
                | "Deref"
                | "DerefMut"
                | "AsRef"
                | "AsMut"
                | "Borrow"
                | "BorrowMut"
                | "FromStr"
                | "TryFrom"
                | "FromIterator"
                | "Extend"
                | "Drop"
                | "Copy"
                | "Hash"
        )
}

/// `impl Tr for ..` of a trait that's neither the file's, a loaded module's, nor one of std's
/// that can be implemented.
fn unknown_impl_trait(t: &str) -> String {
    format!(
        "`impl {t} for ..` is not supported yet (of the std traits, `Display`, `Debug`, `Default`, `From`, `Into`, `TryFrom`, `FromStr`, `FromIterator`, `Extend`, `Ord`, `PartialOrd`, `PartialEq`, `Eq`, `Hash`, `Clone`, `Copy`, `Drop`, `Iterator`, `Deref`, `AsRef` and the `std::ops` operators can be implemented; derive the others; user traits declared in the file or a loaded `mod` file work)"
    )
}

/// `a`, `a or b`, `a, b or c`.
pub(super) fn join_or(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
    }
}

/// The written type of each field of a struct or enum, by (variant, field) as `lower_variant`
/// names them (a struct's one variant is the struct's name; tuple fields are `0`, `1`, ..).
fn written_field_types(node: &SyntaxNode) -> HashMap<(String, String), ast::Type> {
    let mut out = HashMap::new();
    let mut add = |variant: String, fields: Option<ast::FieldList>| match fields {
        Some(ast::FieldList::RecordFieldList(l)) => {
            for f in l.fields() {
                if let (Some(n), Some(t)) = (f.name(), f.ty()) {
                    out.insert((variant.clone(), n.text().to_string()), t);
                }
            }
        }
        Some(ast::FieldList::TupleFieldList(l)) => {
            for (i, f) in l.fields().enumerate() {
                if let Some(t) = f.ty() {
                    out.insert((variant.clone(), i.to_string()), t);
                }
            }
        }
        None => {}
    };
    if let Some(s) = ast::Struct::cast(node.clone()) {
        if let Some(n) = s.name() {
            add(n.text().to_string(), s.field_list());
        }
    } else if let Some(e) = ast::Enum::cast(node.clone()) {
        for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
            if let Some(n) = v.name() {
                add(n.text().to_string(), v.field_list());
            }
        }
    }
    out
}

/// `F` or `std::any::TypeId`: the last segment's name of a path type with no generic arguments.
fn bare_path_name(t: &ast::Type) -> Option<String> {
    let ast::Type::PathType(p) = t else {
        return None;
    };
    let seg = p.path()?.segment()?;
    if seg.generic_arg_list().is_some() {
        return None;
    }
    Some(seg.name_ref()?.text().to_string())
}
