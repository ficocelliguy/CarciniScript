//! Impls of one trait (or inherent impls) for self types that are one type in JS: references
//! (`X` and `&X`, `&T` and `&mut T`), an array and its slice (`[T; N]` and `[T]`, `[u8; 1]` and
//! `[u8; 2]`), boxes and shared pointers (`Box<T>`, `Rc<T>`, `Arc<T>`), `dyn Tr` and `dyn Tr +
//! Send`, `Path` and `PathBuf`. rustc keeps them apart; here they would be two impls for one
//! class. When the later impls do just what an earlier one does (each method with the same body,
//! views of `self` such as `&**self` or `self as &[_]` read as `self`) or hand each call on to it
//! (`self[..].f(x)`), the earlier one serves for all of them and the others are left out (see
//! `adt::all_items`). Impls that behave differently stay, and the checker reports the conflict.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, NodeOrToken, SyntaxKind, SyntaxNode, ast, ast::HasGenericArgs, ast::HasGenericParams,
    ast::HasName,
};

/// The impl's type and const parameter names.
fn impl_params(imp: &ast::Impl) -> HashSet<String> {
    imp.generic_param_list()
        .into_iter()
        .flat_map(|l| l.type_or_const_params())
        .filter_map(|p| match p {
            ast::TypeOrConstParam::Type(t) => t.name(),
            ast::TypeOrConstParam::Const(c) => c.name(),
        })
        .map(|n| n.text().to_string())
        .collect()
}

/// `t` as the type it is in JS, written out: references, boxes and an array's length left out,
/// the impl's parameters (`ps`) as `$P`, lifetimes and auto traits dropped.
fn erased_key(t: &ast::Type, ps: &HashSet<String>) -> Option<String> {
    Some(match t {
        ast::Type::RefType(r) => erased_key(&r.ty()?, ps)?,
        ast::Type::ParenType(p) => erased_key(&p.ty()?, ps)?,
        ast::Type::ArrayType(a) => format!("[{}]", erased_key(&a.ty()?, ps)?),
        ast::Type::SliceType(s) => format!("[{}]", erased_key(&s.ty()?, ps)?),
        ast::Type::TupleType(t) => {
            let es: Vec<String> = t.fields().map(|f| erased_key(&f, ps)).collect::<Option<_>>()?;
            format!("({})", es.join(","))
        }
        ast::Type::DynTraitType(d) => {
            let mut bs = bound_keys(d.type_bound_list()?, ps)?;
            bs.sort();
            format!("dyn {}", bs.join("+"))
        }
        ast::Type::PathType(p) => {
            let path = p.path()?;
            let seg = path.segment()?;
            let name = seg.name_ref()?.text().to_string();
            let args: Vec<ast::GenericArg> = seg
                .generic_arg_list()
                .into_iter()
                .flat_map(|l| l.generic_args())
                .filter(|g| !matches!(g, ast::GenericArg::LifetimeArg(_)))
                .collect();
            if path.qualifier().is_none() && args.is_empty() && ps.contains(&name) {
                return Some("$P".into());
            }
            if let ("Box" | "Rc" | "Arc", [ast::GenericArg::TypeArg(a)]) = (name.as_str(), args.as_slice()) {
                return erased_key(&a.ty()?, ps);
            }
            let name = match name.as_str() {
                "Path" => "PathBuf".to_string(),
                _ => name,
            };
            if args.is_empty() {
                return Some(name);
            }
            let ks: Vec<String> = args
                .iter()
                .map(|g| match g {
                    ast::GenericArg::TypeArg(a) => erased_key(&a.ty()?, ps),
                    g => Some(tokens(g.syntax(), ps)),
                })
                .collect::<Option<_>>()?;
            format!("{name}<{}>", ks.join(","))
        }
        t => tokens(t.syntax(), ps),
    })
}

/// A `dyn` / `impl` type's bounds, without lifetimes and the auto traits (`Send`, `Sync`,
/// `Unpin`), which change nothing in JS.
fn bound_keys(l: ast::TypeBoundList, ps: &HashSet<String>) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for b in l.bounds() {
        let Some(t) = b.ty() else { continue };
        if let ast::Type::PathType(p) = &t
            && p.path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_some_and(|n| matches!(n.text().as_ref(), "Send" | "Sync" | "Unpin"))
        {
            continue;
        }
        out.push(erased_key(&t, ps)?);
    }
    Some(out)
}

/// `n`'s tokens without trivia and lifetimes, the impl's parameters as `$P`.
fn tokens(n: &SyntaxNode, ps: &HashSet<String>) -> String {
    let mut out = String::new();
    for t in n.descendants_with_tokens().filter_map(|e| e.into_token()) {
        if t.kind().is_trivia() || t.kind() == SyntaxKind::LIFETIME_IDENT {
            continue;
        }
        if t.kind() == SyntaxKind::IDENT && ps.contains(t.text()) {
            out.push_str("$P ");
        } else {
            out.push_str(t.text());
            out.push(' ');
        }
    }
    out
}

/// `f`'s body as its tokens, each view of `self` read as `self` (`adt::strip_views`) and the
/// impl's parameters as `$P`.
/// (Its own bindings, parameters first, by position: `v__h1` and `v__h3`, a macro's hygienic
/// names for one `v`, are the same.)
fn shape(f: &ast::Fn, ps: &HashSet<String>) -> Option<String> {
    let mut locals: HashMap<String, usize> = HashMap::new();
    for ip in f.syntax().descendants().filter_map(ast::IdentPat::cast) {
        if let Some(n) = ip.name() {
            let k = locals.len();
            locals.entry(n.text().to_string()).or_insert(k);
        }
    }
    fn walk(n: &SyntaxNode, ps: &HashSet<String>, locals: &HashMap<String, usize>, out: &mut String) {
        for el in n.children_with_tokens() {
            match el {
                NodeOrToken::Token(t) => {
                    if t.kind().is_trivia() || t.kind() == SyntaxKind::LIFETIME_IDENT {
                        continue;
                    }
                    if t.kind() == SyntaxKind::IDENT && ps.contains(t.text()) {
                        out.push_str("$P ");
                    } else if t.kind() == SyntaxKind::IDENT
                        && let Some(i) = locals.get(t.text())
                    {
                        out.push_str(&format!("$L{i} "));
                    } else {
                        out.push_str(t.text());
                        out.push(' ');
                    }
                }
                NodeOrToken::Node(c) => {
                    let is_self = ast::Expr::cast(c.clone())
                        .and_then(crate::adt::strip_views)
                        .is_some_and(|e| matches!(&e, ast::Expr::PathExpr(p) if p.syntax().text() == "self"));
                    if is_self {
                        out.push_str("self ");
                    } else {
                        walk(&c, ps, locals, out);
                    }
                }
            }
        }
    }
    let mut out = String::new();
    walk(f.body()?.syntax(), ps, &locals, &mut out);
    Some(out)
}

struct Entry {
    idx: usize,
    ps: HashSet<String>,
    fns: Vec<ast::Fn>,
    consts: Vec<ast::Const>,
    macros: bool,
}

impl Entry {
    fn new(idx: usize, imp: &ast::Impl) -> Self {
        let mut e = Entry {
            idx,
            ps: impl_params(imp),
            fns: Vec::new(),
            consts: Vec::new(),
            macros: false,
        };
        for item in imp.assoc_item_list().into_iter().flat_map(|l| l.assoc_items()) {
            match item {
                ast::AssocItem::Fn(f) => e.fns.push(f),
                ast::AssocItem::Const(k) => e.consts.push(k),
                ast::AssocItem::TypeAlias(_) => {}
                ast::AssocItem::MacroCall(_) => e.macros = true,
            }
        }
        e
    }

    fn fn_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.fns.iter().filter_map(|f| Some(f.name()?.text().to_string())).collect();
        v.sort();
        v
    }

    fn const_names(&self) -> Vec<String> {
        let mut v: Vec<String> =
            self.consts.iter().filter_map(|k| Some(k.name()?.text().to_string())).collect();
        v.sort();
        v
    }

    /// Every method hands the call on to a view of `self` (`(**self).f(x)`, `self[..].f(x)`).
    fn all_forward(&self) -> bool {
        !self.macros && !self.fns.is_empty() && self.fns.iter().all(crate::adt::forwards)
    }

    /// Whether this impl does what `kept` does: the same methods and consts, each method with
    /// `kept`'s body or handing the call on.
    fn same_as(&self, kept: &Entry) -> bool {
        if self.macros || kept.macros || self.fn_names() != kept.fn_names() || self.const_names() != kept.const_names() {
            return false;
        }
        let fn_ok = self.fns.iter().all(|f| {
            crate::adt::forwards(f)
                || kept
                    .fns
                    .iter()
                    .find(|g| g.name().map(|n| n.text().to_string()) == f.name().map(|n| n.text().to_string()))
                    .is_some_and(|g| shape(g, &kept.ps).is_some_and(|s| Some(s) == shape(f, &self.ps)))
        });
        let const_ok = self.consts.iter().all(|k| {
            kept.consts
                .iter()
                .find(|c| c.name().map(|n| n.text().to_string()) == k.name().map(|n| n.text().to_string()))
                .is_some_and(|c| {
                    let v = |c: &ast::Const, ps| c.body().map(|b| tokens(b.syntax(), ps));
                    v(c, &kept.ps).is_some() && v(c, &kept.ps) == v(k, &self.ps)
                })
        });
        fn_ok && const_ok
    }
}

/// `items` without the impls that only repeat another impl of their trait for a self type that
/// is the same type in JS.
pub fn drop_erased_twins(items: Vec<ast::Item>) -> Vec<ast::Item> {
    // (trait, self type in JS) -> the impls, in order.
    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        let ast::Item::Impl(imp) = item else { continue };
        // (`impl<T: Tr> Tr for &T` / `&mut T` / `Box<T>`: a reference passed for a type
        // parameter carries which one it is, see `typeck_twins`.)
        if imp.excl_token().is_some() || crate::adt::wrapping_impl(imp) {
            continue;
        }
        let ps = impl_params(imp);
        let tr = match imp.trait_() {
            Some(t) => match erased_key(&t, &ps) {
                Some(k) => k,
                None => continue,
            },
            None => String::new(),
        };
        let Some(st) = imp.self_ty().and_then(|t| erased_key(&t, &ps)) else {
            continue;
        };
        groups.entry((tr, st)).or_default().push(i);
    }
    let mut drop: HashSet<usize> = HashSet::new();
    for (_, idxs) in groups {
        if idxs.len() < 2 {
            continue;
        }
        let mut entries: Vec<Entry> = idxs
            .iter()
            .filter_map(|&i| match &items[i] {
                ast::Item::Impl(imp) => Some(Entry::new(i, imp)),
                _ => None,
            })
            .collect();
        // (An impl that hands calls on goes after the ones it hands them on to: `impl Write for
        // Pipe` whose `flush` is `<&Pipe as Write>::flush(..)` after `impl Write for &Pipe`.)
        entries.sort_by_key(|e| (e.fns.iter().filter(|f| crate::adt::forwards(f)).count(), e.idx));
        let mut kept: Vec<Entry> = Vec::new();
        for e in entries {
            // (Inherent impls are each their own unless they repeat one another.)
            if kept.iter().any(|k| e.same_as(k)) {
                drop.insert(e.idx);
            } else {
                kept.push(e);
            }
        }
    }
    // An array's impl that hands each call on to its slice (`impl<T> Fill for [T; 32] where [T]:
    // Fill { .. self[..].try_fill(rng) }`) next to the trait's impls for slices: an array is its
    // slice in JS, so those serve.
    let peeled = |imp: &ast::Impl| {
        let mut t = imp.self_ty();
        while let Some(ast::Type::RefType(r)) = &t {
            t = r.ty();
        }
        t
    };
    let trait_key = |imp: &ast::Impl| imp.trait_().and_then(|t| erased_key(&t, &impl_params(imp)));
    let slice_traits: HashSet<String> = items
        .iter()
        .filter_map(|it| match it {
            ast::Item::Impl(imp) if matches!(peeled(imp), Some(ast::Type::SliceType(_))) => trait_key(imp),
            _ => None,
        })
        .collect();
    for (i, it) in items.iter().enumerate() {
        if let ast::Item::Impl(imp) = it
            && matches!(peeled(imp), Some(ast::Type::ArrayType(_)))
            && trait_key(imp).is_some_and(|k| slice_traits.contains(&k))
            && Entry::new(i, imp).all_forward()
        {
            drop.insert(i);
        }
    }
    if drop.is_empty() {
        return items;
    }
    items
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop.contains(i))
        .map(|(_, it)| it)
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn drops_slice_array_marker() {
        let src = "pub trait Marker {}\nimpl Marker for [u8] {}\nimpl<const N: usize> Marker for [u8; N] {}\n";
        let file = ra_ap_syntax::SourceFile::parse(src, ra_ap_syntax::Edition::Edition2021).tree();
        let items = crate::adt::all_items(&file);
        assert_eq!(items.len(), 2);
    }
}
