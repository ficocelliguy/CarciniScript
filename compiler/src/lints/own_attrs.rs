//! The lints the program's own attributes ask for: `deprecated` on a use of a `#[deprecated]`
//! item, and `unused_must_use` on a dropped call of a `#[must_use]` function or method, or a
//! dropped value of a `#[must_use]` type.
//!
//! Items are found by name in the file as written. A name declared twice (or bound by a pattern,
//! for a one-segment path) resolves to nothing, so a use this pass can't pin down stays silent.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, AstToken, SyntaxKind as K, SyntaxNode,
    ast::{self, HasAttrs, HasName},
};

use super::{Linter, Range, impl_self_name, range};
use crate::ty::Ty;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Fn,
    AssocFn,
    Method,
    Struct,
    TupleStruct,
    UnitStruct,
    Enum,
    Union,
    Trait,
    TypeAlias,
    Const,
    Static,
    Mod,
    UnitVariant,
    TupleVariant,
    RecordVariant,
    Other,
}

#[derive(Clone, Debug)]
struct Item {
    kind: Kind,
    /// The path rustc's `deprecated` message prints: from the crate root (`m::inner`,
    /// `m::P::f`, `DE::A`).
    path: String,
    /// The trimmed path its `unused_must_use` message prints (`calc`, `P::g`, `Tr::t`).
    short: String,
    /// `#[deprecated]`, with its note; a variant's or a field's comes from its enum or struct.
    deprecated: Option<Option<String>>,
    must_use: Option<Option<String>>,
}

#[derive(Default)]
pub(super) struct Own {
    /// Whether the file has either attribute at all (else both lints are skipped).
    pub(super) any: bool,
    /// Items outside impls and traits, by name (all of them: a name declared twice is
    /// ambiguous).
    by_name: HashMap<String, Vec<Item>>,
    /// Associated functions of inherent impls and traits, and enum variants: by (type, name).
    assoc: HashMap<(String, String), Vec<Item>>,
    /// Trait methods by (trait, name).
    trait_methods: HashMap<(String, String), Item>,
    /// The traits each type implements.
    impls: HashMap<String, Vec<String>>,
    /// Names bound by patterns and parameters (they may shadow an item).
    locals: HashSet<String>,
    /// Type names the file also writes behind another crate's path (`::other::Tool`,
    /// `cursor::SyntaxToken`): the checker may call that type by the file's own, so a value of
    /// the name isn't known to be ours.
    foreign: HashSet<String>,
}

/// `#[name]`, `#[name = "x"]` or `#[name(.., note = "x", ..)]`: `Some` with the message.
fn attr_arg(attrs: impl Iterator<Item = ast::Attr>, name: &str) -> Option<Option<String>> {
    attrs.into_iter().find_map(|a| {
        let string = |t: ra_ap_syntax::SyntaxToken| ast::String::cast(t).and_then(|s| s.value().ok().map(|v| v.into_owned()));
        match a.meta()? {
            ast::Meta::PathMeta(m) if m.path()?.syntax().text() == name => Some(None),
            ast::Meta::KeyValueMeta(m) if m.path()?.syntax().text() == name => match m.expr()? {
                ast::Expr::Literal(l) => Some(l.syntax().first_token().and_then(string)),
                _ => Some(None),
            },
            ast::Meta::TokenTreeMeta(m) if m.path()?.syntax().text() == name => {
                let toks: Vec<_> = m
                    .token_tree()?
                    .syntax()
                    .children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .filter(|t| !t.kind().is_trivia())
                    .collect();
                let note = toks.windows(3).find_map(|w| {
                    (w[0].text() == "note" && w[1].kind() == K::EQ).then(|| string(w[2].clone())).flatten()
                });
                Some(note)
            }
            _ => None,
        }
    })
}

/// The `mod` names around `n`, outermost first, each followed by `::`.
fn mod_prefix(n: &SyntaxNode) -> String {
    let mut names: Vec<String> = n
        .ancestors()
        .skip(1)
        .filter_map(ast::Module::cast)
        .filter_map(|m| m.name().map(|n| n.text().to_string()))
        .collect();
    names.reverse();
    names.iter().map(|n| format!("{n}::")).collect()
}

fn kind_word(k: Kind, value: bool) -> &'static str {
    match k {
        Kind::Fn => "function",
        Kind::AssocFn => "associated function",
        Kind::Method => "method",
        Kind::Struct => "struct",
        Kind::TupleStruct if value => "tuple struct",
        Kind::UnitStruct if value => "unit struct",
        Kind::TupleStruct | Kind::UnitStruct => "struct",
        Kind::Enum => "enum",
        Kind::Union => "union",
        Kind::Trait => "trait",
        Kind::TypeAlias => "type alias",
        Kind::Const => "constant",
        Kind::Static => "static",
        Kind::Mod => "module",
        Kind::UnitVariant => "unit variant",
        Kind::TupleVariant => "tuple variant",
        Kind::RecordVariant | Kind::Other => "",
    }
}

fn struct_kind(fl: Option<ast::FieldList>) -> Kind {
    match fl {
        Some(ast::FieldList::RecordFieldList(_)) => Kind::Struct,
        Some(ast::FieldList::TupleFieldList(_)) => Kind::TupleStruct,
        None => Kind::UnitStruct,
    }
}

impl Own {
    pub(super) fn collect(l: &Linter) -> Own {
        let mut own = Own {
            any: l.src.contains("#[must_use") || l.src.contains("#[deprecated"),
            ..Default::default()
        };
        if !own.any {
            return own;
        }
        for n in l.file.syntax().descendants() {
            if let Some(ip) = ast::IdentPat::cast(n.clone()) {
                if let Some(name) = ip.name() {
                    own.locals.insert(name.text().to_string());
                }
                continue;
            }
            if l.skipped(&n) {
                continue;
            }
            let prefix = mod_prefix(&n);
            let item = |kind: Kind, name: &str, attrs: Vec<ast::Attr>| {
                Item {
                    kind,
                    path: format!("{prefix}{name}"),
                    short: name.to_string(),
                    deprecated: attr_arg(attrs.iter().cloned(), "deprecated"),
                    must_use: attr_arg(attrs.into_iter(), "must_use"),
                }
            };
            let named = |x: &dyn HasName| x.name().map(|n| n.text().to_string());
            if let Some(f) = ast::Fn::cast(n.clone()) {
                let Some(name) = named(&f) else { continue };
                let owner = n.parent().filter(|p| p.kind() == K::ASSOC_ITEM_LIST).and_then(|p| p.parent());
                let method = f.param_list().and_then(|p| p.self_param()).is_some();
                match owner {
                    None => own.by_name.entry(name.clone()).or_default().push(item(Kind::Fn, &name, f.attrs().collect())),
                    Some(o) => {
                        let kind = if method { Kind::Method } else { Kind::AssocFn };
                        if let Some(i) = ast::Impl::cast(o.clone()) {
                            // (A trait impl's methods take the trait's attributes.)
                            let Some(ty) = impl_self_name(&i) else { continue };
                            if i.trait_().is_some() {
                                continue;
                            }
                            let mut it = item(kind, &name, f.attrs().collect());
                            it.path = format!("{}{ty}::{name}", mod_prefix(&o));
                            it.short = format!("{ty}::{name}");
                            own.assoc.entry((ty, name)).or_default().push(it);
                        } else if let Some(t) = ast::Trait::cast(o.clone()) {
                            let Some(tr) = named(&t) else { continue };
                            let mut it = item(kind, &name, f.attrs().collect());
                            it.path = format!("{}{tr}::{name}", mod_prefix(&o));
                            it.short = format!("{tr}::{name}");
                            own.assoc.entry((tr.clone(), name.clone())).or_default().push(it.clone());
                            own.trait_methods.insert((tr, name), it);
                        }
                    }
                }
                continue;
            }
            if let Some(i) = ast::Impl::cast(n.clone()) {
                if let (Some(ty), Some(ast::Type::PathType(tr))) = (impl_self_name(&i), i.trait_())
                    && let Some(tr) = tr.path().and_then(|p| p.segment()).and_then(|s| s.name_ref())
                {
                    own.impls.entry(ty).or_default().push(tr.text().to_string());
                }
                continue;
            }
            // (Items inside an impl or trait other than functions: associated consts and types.)
            if n.parent().is_some_and(|p| p.kind() == K::ASSOC_ITEM_LIST) {
                continue;
            }
            let (kind, name, attrs) = if let Some(s) = ast::Struct::cast(n.clone()) {
                (struct_kind(s.field_list()), named(&s), s.attrs().collect())
            } else if let Some(e) = ast::Enum::cast(n.clone()) {
                let Some(en) = named(&e) else { continue };
                let it = item(Kind::Enum, &en, e.attrs().collect());
                for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
                    let Some(vn) = named(&v) else { continue };
                    let vk = match struct_kind(v.field_list()) {
                        Kind::Struct => Kind::RecordVariant,
                        Kind::TupleStruct => Kind::TupleVariant,
                        _ => Kind::UnitVariant,
                    };
                    let mut vi = item(vk, &vn, v.attrs().collect());
                    vi.path = format!("{}::{vn}", it.path);
                    vi.short = format!("{en}::{vn}");
                    vi.deprecated = vi.deprecated.or_else(|| it.deprecated.clone());
                    own.assoc.entry((en.clone(), vn)).or_default().push(vi);
                }
                own.by_name.entry(en).or_default().push(it);
                continue;
            } else if let Some(x) = ast::Union::cast(n.clone()) {
                (Kind::Union, named(&x), x.attrs().collect())
            } else if let Some(x) = ast::Trait::cast(n.clone()) {
                (Kind::Trait, named(&x), x.attrs().collect())
            } else if let Some(x) = ast::TypeAlias::cast(n.clone()) {
                (Kind::TypeAlias, named(&x), x.attrs().collect())
            } else if let Some(x) = ast::Const::cast(n.clone()) {
                (Kind::Const, named(&x), x.attrs().collect())
            } else if let Some(x) = ast::Static::cast(n.clone()) {
                (Kind::Static, named(&x), x.attrs().collect())
            } else if let Some(x) = ast::Module::cast(n.clone()) {
                // (A `mod x;` file's items are elsewhere: not a module paths resolve through.)
                let kind = if x.item_list().is_some() { Kind::Mod } else { Kind::Other };
                (kind, named(&x), x.attrs().collect())
            } else if let Some(x) = ast::MacroRules::cast(n.clone()) {
                (Kind::Other, named(&x), x.attrs().collect())
            } else {
                continue;
            };
            let Some(name) = name else { continue };
            let it = item(kind, &name, attrs);
            own.by_name.entry(name).or_default().push(it);
        }
        for p in l.file.syntax().descendants().filter_map(ast::Path::cast) {
            if p.syntax().parent().is_some_and(|x| x.kind() == K::PATH) || p.qualifier().is_none() {
                continue;
            }
            let first = p.first_segment();
            let ours = !p.syntax().text().to_string().starts_with("::")
                && first.and_then(|s| s.kind()).is_some_and(|k| match k {
                    ast::PathSegmentKind::Name(n) => own.unique(n.text()).is_some_and(|i| i.kind != Kind::Other),
                    ast::PathSegmentKind::Type { .. } => false,
                    _ => true,
                });
            if !ours && let Some(n) = p.segment().and_then(|s| s.name_ref()) {
                own.foreign.insert(n.text().to_string());
            }
        }
        own
    }

    fn unique(&self, name: &str) -> Option<&Item> {
        match self.by_name.get(name).map(|v| v.as_slice()) {
            Some([one]) => Some(one),
            _ => None,
        }
    }

    fn unique_assoc(&self, ty: &str, name: &str) -> Option<&Item> {
        match self.assoc.get(&(ty.to_string(), name.to_string())).map(|v| v.as_slice()) {
            Some([one]) => Some(one),
            _ => None,
        }
    }

    /// The item a path names, when the file pins it down.
    fn resolve(&self, p: &ast::Path) -> Option<&Item> {
        let mut segs: Vec<String> = Vec::new();
        for s in p.segments() {
            segs.push(match s.kind()? {
                ast::PathSegmentKind::Name(n) => n.text().to_string(),
                ast::PathSegmentKind::SelfTypeKw => {
                    let imp = p.syntax().ancestors().find_map(ast::Impl::cast)?;
                    impl_self_name(&imp)?
                }
                ast::PathSegmentKind::SelfKw | ast::PathSegmentKind::SuperKw | ast::PathSegmentKind::CrateKw => "::".into(),
                ast::PathSegmentKind::Type { .. } => return None,
            });
        }
        let (last, lead) = segs.split_last()?;
        let module = |s: &String| s == "::" || self.unique(s).is_some_and(|i| i.kind == Kind::Mod);
        if lead.is_empty() {
            return if self.locals.contains(last) { None } else { self.unique(last) };
        }
        if lead.iter().all(module) {
            return self.unique(last);
        }
        let (ty, mods) = lead.split_last()?;
        if !mods.iter().all(module) {
            return None;
        }
        let owner = self.unique(ty)?;
        if !matches!(owner.kind, Kind::Struct | Kind::TupleStruct | Kind::UnitStruct | Kind::Enum | Kind::Trait | Kind::Union) {
            return None;
        }
        self.unique_assoc(ty, last)
    }

    /// The file's struct or enum a checker type names (`Adt("m__P")` for `m::P`).
    fn adt_name(&self, t: Option<&Ty>) -> Option<String> {
        let Some(Ty::Adt(n, _)) = t else { return None };
        let short = n.rsplit("__").next().unwrap_or(n);
        if self.foreign.contains(short) {
            return None;
        }
        let it = self.unique(short)?;
        matches!(it.kind, Kind::Struct | Kind::TupleStruct | Kind::UnitStruct | Kind::Enum | Kind::Union).then(|| short.to_string())
    }

    /// The method a call on a value of type `ty` names: the inherent one, else the one trait
    /// the type implements that declares it.
    fn method(&self, ty: &str, name: &str) -> Option<&Item> {
        if let Some(v) = self.assoc.get(&(ty.to_string(), name.to_string())) {
            return match v.as_slice() {
                [one] if one.kind == Kind::Method => Some(one),
                _ => None,
            };
        }
        let mut found = self
            .impls
            .get(ty)?
            .iter()
            .filter_map(|tr| self.trait_methods.get(&(tr.clone(), name.to_string())));
        let one = found.next()?;
        found.next().is_none().then_some(one)
    }
}

impl Linter<'_> {
    /// `deprecated`: each use of a `#[deprecated]` item (the last segment of the path that
    /// names it), method or field.
    pub(super) fn deprecated(&mut self) {
        if !self.own.any {
            return;
        }
        let mut found: Vec<(Range, String)> = Vec::new();
        let note = |n: &Option<String>| n.as_ref().map(|n| format!(": {n}")).unwrap_or_default();
        for n in self.file.syntax().descendants() {
            if self.skipped(&n) {
                continue;
            }
            if let Some(p) = ast::Path::cast(n.clone()) {
                let Some(parent) = n.parent() else { continue };
                let value = match parent.kind() {
                    K::PATH_EXPR | K::PATH_PAT | K::TUPLE_STRUCT_PAT => true,
                    K::PATH_TYPE | K::USE_TREE | K::RECORD_EXPR | K::RECORD_PAT => false,
                    _ => continue,
                };
                // (The type in `Old::new()`; a variant's path warns for the variant alone.)
                if let Some(q) = p.qualifier()
                    && let Some(t) = self.own.resolve(&q)
                    && matches!(t.kind, Kind::Struct | Kind::TupleStruct | Kind::UnitStruct | Kind::Enum | Kind::Union)
                    && let Some(dep) = &t.deprecated
                    && !self.own.resolve(&p).is_some_and(|i| matches!(i.kind, Kind::UnitVariant | Kind::TupleVariant | Kind::RecordVariant))
                    && let Some(seg) = q.segment().and_then(|s| s.name_ref())
                {
                    found.push((range(seg.syntax()), format!("use of deprecated {} `{}`{}", kind_word(t.kind, false), t.path, note(dep))));
                }
                let Some(it) = self.own.resolve(&p) else { continue };
                let Some(dep) = &it.deprecated else { continue };
                let word = kind_word(it.kind, value);
                let Some(seg) = p.segment().and_then(|s| s.name_ref()).map(|n| range(n.syntax())) else { continue };
                if word.is_empty() {
                    continue;
                }
                found.push((seg, format!("use of deprecated {word} `{}`{}", it.path, note(dep))));
                // A deprecated struct's fields, named in its literal.
                if parent.kind() == K::RECORD_EXPR && it.kind == Kind::Struct {
                    let fields = ast::RecordExpr::cast(parent).and_then(|r| r.record_expr_field_list());
                    for f in fields.into_iter().flat_map(|l| l.fields()) {
                        let Some(name) = f.name_ref() else { continue };
                        found.push((range(f.syntax()), format!("use of deprecated field `{}::{}`{}", it.path, name.text(), note(dep))));
                    }
                }
            } else if let Some(m) = ast::MethodCallExpr::cast(n.clone()) {
                let (Some(recv), Some(name)) = (m.receiver(), m.name_ref()) else { continue };
                let Some(ty) = self.own.adt_name(self.ty(range(recv.syntax()))) else { continue };
                let Some(it) = self.own.method(&ty, name.text().as_ref()) else { continue };
                let Some(dep) = &it.deprecated else { continue };
                found.push((range(name.syntax()), format!("use of deprecated method `{}`{}", it.path, note(dep))));
            } else if let Some(f) = ast::FieldExpr::cast(n.clone()) {
                let (Some(recv), Some(name)) = (f.expr(), f.name_ref()) else { continue };
                let Some(ty) = self.own.adt_name(self.ty(range(recv.syntax()))) else { continue };
                let Some(it) = self.own.unique(&ty) else { continue };
                if !matches!(it.kind, Kind::Struct | Kind::TupleStruct) {
                    continue;
                }
                let Some(dep) = &it.deprecated else { continue };
                found.push((range(f.syntax()), format!("use of deprecated field `{}::{}`{}", it.path, name.text(), note(dep))));
            }
        }
        for ((s, e), msg) in found {
            let d = self.diag(s, e, msg);
            self.emit("deprecated", d);
        }
    }

    /// `unused_must_use` for the program's own items: a dropped value of a `#[must_use]`
    /// struct or enum (`unused `Token` that must be used`), else a dropped call of a
    /// `#[must_use]` function or method (`unused return value of `calc` ..`). With the
    /// attribute's message as the note.
    pub(super) fn own_must_use(&self, e: &ast::Expr) -> Option<(String, Option<String>)> {
        if !self.own.any {
            return None;
        }
        let r = range(e.syntax());
        if let Some(ty) = self.own.adt_name(self.ty(r))
            && let Some(it) = self.own.unique(&ty)
            && let Some(reason) = &it.must_use
        {
            return Some((format!("unused `{}` that must be used", it.short), reason.clone()));
        }
        let it = match e {
            ast::Expr::CallExpr(c) => {
                let ast::Expr::PathExpr(p) = c.expr()? else { return None };
                self.own.resolve(&p.path()?)?
            }
            ast::Expr::MethodCallExpr(m) => {
                let ty = self.own.adt_name(self.ty(range(m.receiver()?.syntax())))?;
                self.own.method(&ty, m.name_ref()?.text().as_ref())?
            }
            _ => return None,
        };
        if !matches!(it.kind, Kind::Fn | Kind::AssocFn | Kind::Method) {
            return None;
        }
        let reason = it.must_use.clone()?;
        Some((format!("unused return value of `{}` that must be used", it.short), reason))
    }
}
