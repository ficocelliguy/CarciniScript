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

/// What the file names itself and what it takes from std, for telling std's `Cell` and
/// iterator types from a struct of the same name.
#[derive(Default)]
struct StdNames {
    /// Structs, enums, unions and type aliases the file declares.
    local: HashSet<String>,
    /// Iterator type names brought in from std by a `use`.
    used_iters: HashSet<String>,
}

thread_local! {
    static STD_NAMES: std::cell::RefCell<StdNames> = std::cell::RefCell::new(StdNames::default());
}

/// std's iterator and adapter types (`vec::IntoIter`, `array::IntoIter`, `vec::Drain`,
/// `string::Drain`, `slice::Iter`, ..).
const STD_ITERS: &[&str] = &[
    "Iter", "IterMut", "IntoIter", "Drain", "Chars", "CharIndices", "Bytes", "Keys", "Values",
    "ValuesMut", "IntoKeys", "IntoValues", "Peekable", "Enumerate", "Rev", "Skip", "Take", "Zip",
    "Map", "Filter", "FilterMap", "Lines", "Split", "SplitWhitespace", "Windows", "Chunks",
    "ChunksExact", "StepBy", "Cycle", "Chain", "Flatten", "FlatMap", "Fuse", "Inspect", "TakeWhile",
    "SkipWhile", "Scan", "Once", "Repeat", "Empty", "Successors", "FromFn", "Cloned", "Copied",
];

/// The first segment of a path that starts in std (`std::`, or one of its modules that hold
/// iterator types).
const STD_ROOTS: &[&str] = &[
    "std", "core", "alloc", "vec", "array", "slice", "string", "str", "iter", "collections",
    "hash_map", "hash_set", "btree_map", "btree_set", "vec_deque", "option", "result",
];

impl StdNames {
    fn of(items: &[ast::Item]) -> Self {
        let mut s = StdNames::default();
        for item in items {
            let name = match item {
                ast::Item::Struct(x) => x.name(),
                ast::Item::Enum(x) => x.name(),
                ast::Item::Union(x) => x.name(),
                ast::Item::TypeAlias(x) => x.name(),
                ast::Item::Use(u) => {
                    let Some(tree) = u.use_tree() else { continue };
                    let mut stack = vec![(tree, false)];
                    while let Some((t, under_std)) = stack.pop() {
                        let first = t
                            .path()
                            .and_then(|p| p.first_segment())
                            .and_then(|s| s.name_ref())
                            .map(|n| n.text().to_string());
                        let std = under_std || first.as_deref().is_some_and(|f| matches!(f, "std" | "core" | "alloc"));
                        if let Some(l) = t.use_tree_list() {
                            stack.extend(l.use_trees().map(|c| (c, std)));
                        } else if std
                            && t.rename().is_none()
                            && let Some(n) = t.path().and_then(|p| p.segment()).and_then(|s| s.name_ref())
                            && STD_ITERS.contains(&n.text().as_ref())
                        {
                            s.used_iters.insert(n.text().to_string());
                        }
                    }
                    None
                }
                _ => None,
            };
            if let Some(n) = name {
                s.local.insert(n.text().to_string());
            }
        }
        s
    }

    /// Whether `path` (whose last segment is `name`) names one of std's iterator types.
    fn is_std_iter(&self, path: &ast::Path, name: &str) -> bool {
        if !STD_ITERS.contains(&name) {
            return false;
        }
        match path.qualifier() {
            Some(q) => q
                .first_segment()
                .and_then(|s| s.name_ref())
                .is_some_and(|n| STD_ROOTS.contains(&n.text().as_ref()) && !self.local.contains(&n.text().to_string())),
            None => self.used_iters.contains(name) && !self.local.contains(name),
        }
    }
}

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
            // (std's iterators are all the runtime's `Iter`, whatever they iterate.)
            if STD_NAMES.with(|s| s.borrow().is_std_iter(&path, &name)) {
                return Some("$Iter".into());
            }
            let name = match name.as_str() {
                "Path" => "PathBuf".to_string(),
                // (A `Cell` is a `RefCell` at run time.)
                "Cell" if STD_NAMES.with(|s| {
                    let s = s.borrow();
                    !s.local.contains("Cell") && !s.local.contains("RefCell")
                }) =>
                {
                    "RefCell".to_string()
                }
                _ => name,
            };
            if args.is_empty() {
                return Some(name);
            }
            // (A map's or set's hasher doesn't exist in JS: `HashMap<K, V, S>` is `HashMap<K, V>`.)
            let args = match hasher_pos(&name) {
                Some(n) if args.len() > n => &args[..n],
                _ => &args[..],
            };
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

/// How many type arguments come before a std map's or set's hasher (`HashMap<K, V, S>`: 2).
fn hasher_pos(name: &str) -> Option<usize> {
    match name {
        "HashMap" | "IndexMap" => Some(2),
        "HashSet" | "IndexSet" => Some(1),
        _ => None,
    }
}

/// In an impl for a map or set, a call that builds one with a hasher, or by the type's path
/// (`Self::with_hasher(Default::default())`, `HashMap::new()`, `HashMap::with_capacity_and_hasher(n,
/// s)`), as the shape without the hasher (`Self::new()`, `Self::with_capacity(n)`): hashers don't
/// exist in JS.
fn hasher_free_call(c: &ast::CallExpr, ps: &HashSet<String>) -> Option<String> {
    use ra_ap_syntax::ast::HasArgList;
    let ast::Expr::PathExpr(p) = c.expr()? else {
        return None;
    };
    let path = p.path()?;
    let f = path.segment()?.name_ref()?.text().to_string();
    let owner = path.qualifier()?.segment()?.syntax().text().to_string();
    let owner = owner.split('<').next().unwrap_or_default().trim().to_string();
    if !matches!(owner.as_str(), "Self" | "HashMap" | "HashSet" | "IndexMap" | "IndexSet") {
        return None;
    }
    let args: Vec<ast::Expr> = c.arg_list()?.args().collect();
    let arg = |e: &ast::Expr| tokens(e.syntax(), ps);
    Some(match (f.as_str(), args.as_slice()) {
        ("new" | "default", []) | ("with_hasher", [_]) => "Self :: new ( ) ".to_string(),
        ("with_capacity", [n]) | ("with_capacity_and_hasher", [n, _]) => {
            format!("Self :: with_capacity ( {}) ", arg(n))
        }
        _ => return None,
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
/// (`hash`: in an impl for a map or set, calls that build one compare without the hasher, see
/// `hasher_free_call`.)
fn shape(f: &ast::Fn, ps: &HashSet<String>, hash: bool) -> Option<String> {
    let mut locals: HashMap<String, usize> = HashMap::new();
    for ip in f.syntax().descendants().filter_map(ast::IdentPat::cast) {
        if let Some(n) = ip.name() {
            let k = locals.len();
            locals.entry(n.text().to_string()).or_insert(k);
        }
    }
    fn walk(n: &SyntaxNode, ps: &HashSet<String>, locals: &HashMap<String, usize>, hash: bool, out: &mut String) {
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
                    let hasher_free = if hash { ast::CallExpr::cast(c.clone()).and_then(|x| hasher_free_call(&x, ps)) } else { None };
                    if is_self {
                        out.push_str("self ");
                    } else if let Some(s) = hasher_free {
                        out.push_str(&s);
                    } else {
                        walk(&c, ps, locals, hash, out);
                    }
                }
            }
        }
    }
    let mut out = String::new();
    walk(f.body()?.syntax(), ps, &locals, hash, &mut out);
    Some(out)
}

struct Entry {
    idx: usize,
    ps: HashSet<String>,
    fns: Vec<ast::Fn>,
    consts: Vec<ast::Const>,
    macros: bool,
    /// An impl for a map or set (see `shape`).
    hash: bool,
}

impl Entry {
    fn new(idx: usize, imp: &ast::Impl) -> Self {
        let mut e = Entry {
            idx,
            ps: impl_params(imp),
            fns: Vec::new(),
            consts: Vec::new(),
            macros: false,
            hash: false,
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
                    .is_some_and(|g| shape(g, &kept.ps, self.hash).is_some_and(|s| Some(s) == shape(f, &self.ps, self.hash)))
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
    STD_NAMES.with(|s| *s.borrow_mut() = StdNames::of(&items));
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
    for ((_, st), idxs) in groups {
        let hash = ["HashMap<", "HashSet<", "IndexMap<", "IndexSet<"].iter().any(|h| st.starts_with(h));
        if idxs.len() < 2 {
            continue;
        }
        let mut entries: Vec<Entry> = idxs
            .iter()
            .filter_map(|&i| match &items[i] {
                ast::Item::Impl(imp) => Some(Entry { hash, ..Entry::new(i, imp) }),
                _ => None,
            })
            .collect();
        // (An impl that hands calls on goes after the ones it hands them on to: `impl Write for
        // Pipe` whose `flush` is `<&Pipe as Write>::flush(..)` after `impl Write for &Pipe`.)
        // (And a map's impl with a hasher after the one without: the one kept is std's default.)
        let with_hasher = |i: usize| match &items[i] {
            ast::Item::Impl(imp) => match imp.self_ty() {
                Some(ast::Type::PathType(p)) => p
                    .path()
                    .and_then(|p| p.segment())
                    .is_some_and(|s| {
                        let n = s.name_ref().map(|n| n.text().to_string()).unwrap_or_default();
                        let count = s
                            .generic_arg_list()
                            .into_iter()
                            .flat_map(|l| l.generic_args())
                            .filter(|g| !matches!(g, ast::GenericArg::LifetimeArg(_)))
                            .count();
                        hasher_pos(&n).is_some_and(|k| count > k)
                    }),
                _ => false,
            },
            _ => false,
        };
        entries.sort_by_key(|e| (with_hasher(e.idx), e.fns.iter().filter(|f| crate::adt::forwards(f)).count(), e.idx));
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

    #[test]
    fn drops_std_cell_and_iter_twins_only() {
        let parse = |src: &str| {
            let file = ra_ap_syntax::SourceFile::parse(src, ra_ap_syntax::Edition::Edition2021).tree();
            crate::adt::all_items(&file).len()
        };
        let std_cells = "use std::cell::{Cell, RefCell};\npub trait T { fn f(); }\nimpl T for Cell<u8> { fn f() {} }\nimpl T for RefCell<u8> { fn f() {} }\n";
        assert_eq!(parse(std_cells), 3);
        // (A struct of the file named `Cell` is its own class.)
        let own_cell = "use std::cell::RefCell;\npub struct Cell<X>(X);\npub trait T { fn f(); }\nimpl T for Cell<u8> { fn f() {} }\nimpl T for RefCell<u8> { fn f() {} }\n";
        assert_eq!(parse(own_cell), 5);
        let iters = "pub trait T { fn f(&self); }\nimpl<A> T for std::vec::IntoIter<A> { fn f(&self) {} }\nimpl<'a> T for std::string::Drain<'a> { fn f(&self) {} }\n";
        assert_eq!(parse(iters), 2);
        let own_iter = "pub struct IntoIter;\npub trait T { fn f(&self); }\nimpl T for IntoIter { fn f(&self) {} }\nimpl<A> T for std::vec::IntoIter<A> { fn f(&self) {} }\n";
        assert_eq!(parse(own_iter), 4);
    }
}
