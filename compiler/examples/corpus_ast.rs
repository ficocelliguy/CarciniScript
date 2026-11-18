//! The real-world corpus survey, step 2 (between `tools/corpus-classify.mjs` and
//! `tools/corpus-report.mjs`): read the classifier's JSONL, parse each file with rust-analyzer's
//! parser, and add what compiling alone can't tell:
//!
//! - `feats`: the Rust features each file uses, found syntactically, so a feature counts even in
//!   files where an unresolved name stops the compiler before it gets there.
//! - for each error, `node`: the construct at its position (`kinds`, innermost first, and the
//!   feature it falls in, if any) and `ext`: names in that construct the file doesn't define
//!   (imported from another crate or file, or not found at all), so an error downstream of an
//!   unresolved name can be told from a real gap.
//!
//! Each feature also has a probe, a small program using it; the probes are compiled and the
//! results written to `<out>.probes.json`: a feature whose probe fails is missing.
//!
//! `cargo run --release --example corpus_ast -- <root> <classified.jsonl> <out.jsonl>`
//! `cargo run --release --example corpus_ast -- --probes` (only the probes, printed)
use ra_ap_syntax::{
    Edition, NodeOrToken, SourceFile, SyntaxKind, SyntaxKind::*, SyntaxNode, T, TextRange, TextSize,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::io::{BufRead, Write};

struct Feature {
    id: &'static str,
    label: &'static str,
    /// Not planned: JS values have no memory layout, or the feature is about linking.
    not_planned: bool,
    detect: fn(&SyntaxNode) -> bool,
    probe: &'static str,
}

fn tok(n: &SyntaxNode, k: SyntaxKind) -> bool {
    n.children_with_tokens().any(|c| c.kind() == k)
}
fn child(n: &SyntaxNode, k: SyntaxKind) -> Option<SyntaxNode> {
    n.children().find(|c| c.kind() == k)
}
fn parent_is(n: &SyntaxNode, k: SyntaxKind) -> bool {
    n.parent().is_some_and(|p| p.kind() == k)
}
fn in_trait(n: &SyntaxNode) -> bool {
    n.parent()
        .filter(|p| p.kind() == ASSOC_ITEM_LIST)
        .and_then(|p| p.parent())
        .is_some_and(|p| p.kind() == TRAIT)
}
fn in_impl(n: &SyntaxNode) -> bool {
    n.parent()
        .filter(|p| p.kind() == ASSOC_ITEM_LIST)
        .and_then(|p| p.parent())
        .is_some_and(|p| p.kind() == IMPL)
}
/// The type an `impl .. for X` is for (the last type child of the impl).
fn impl_self_ty(n: &SyntaxNode) -> Option<SyntaxNode> {
    if n.kind() != IMPL || !tok(n, T![for]) {
        return None;
    }
    let mut after_for = false;
    for c in n.children_with_tokens() {
        match c {
            NodeOrToken::Token(t) if t.kind() == T![for] => after_for = true,
            NodeOrToken::Node(c)
                if after_for && c.kind() != WHERE_CLAUSE && c.kind() != ASSOC_ITEM_LIST =>
            {
                return Some(c);
            }
            _ => {}
        }
    }
    None
}
fn first_ident(n: &SyntaxNode) -> String {
    n.descendants_with_tokens()
        .find_map(|e| {
            e.into_token()
                .filter(|t| t.kind() == IDENT)
                .map(|t| t.text().to_string())
        })
        .unwrap_or_default()
}
const PRIMS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "String",
];
const STD_GENERIC: &[&str] = &[
    "Vec",
    "Option",
    "Result",
    "Box",
    "Rc",
    "Arc",
    "HashMap",
    "HashSet",
    "BTreeMap",
    "BTreeSet",
    "VecDeque",
    "BinaryHeap",
    "RefCell",
    "Cell",
];
fn is_screaming(s: &str) -> bool {
    s.len() > 1
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && s.chars().any(|c| c.is_ascii_uppercase())
}
fn path_segments(n: &SyntaxNode) -> Vec<String> {
    n.descendants()
        .filter(|d| d.kind() == PATH_SEGMENT)
        .map(|s| first_ident(&s))
        .collect()
}
fn lit_starts(n: &SyntaxNode, k: SyntaxKind, pre: &str) -> bool {
    n.kind() == LITERAL
        && n.children_with_tokens()
            .any(|c| c.kind() == k && c.as_token().is_some_and(|t| t.text().starts_with(pre)))
}

macro_rules! f {
    ($id:literal, $label:literal, $detect:expr, $probe:literal) => {
        Feature {
            id: $id,
            label: $label,
            not_planned: false,
            detect: $detect,
            probe: $probe,
        }
    };
    (np $id:literal, $label:literal, $detect:expr, $probe:literal) => {
        Feature {
            id: $id,
            label: $label,
            not_planned: true,
            detect: $detect,
            probe: $probe,
        }
    };
}

fn features() -> Vec<Feature> {
    vec![
        // Items.
        f!(
            "inline_mod",
            "inline module `mod m { .. }`",
            |n| n.kind() == MODULE && child(n, ITEM_LIST).is_some(),
            "mod m { pub fn f() -> i32 { 1 } } fn main() { println!(\"{}\", m::f()); }"
        ),
        f!(
            "extern_crate",
            "`extern crate x;`",
            |n| n.kind() == EXTERN_CRATE && child(n, RENAME).is_none(),
            "extern crate core; fn main() { println!(\"{}\", core::cmp::max(1, 2)); }"
        ),
        f!(
            "extern_crate_rename",
            "`extern crate x as y;`",
            |n| n.kind() == EXTERN_CRATE && child(n, RENAME).is_some(),
            "extern crate core as c; fn main() { println!(\"{}\", c::cmp::max(1, 2)); }"
        ),
        f!(
            "macro_rules_in_fn",
            "`macro_rules!` inside a function",
            |n| n.kind() == MACRO_RULES && n.ancestors().any(|a| a.kind() == STMT_LIST),
            "fn main() { macro_rules! m { () => { 1 } } println!(\"{}\", m!()); }"
        ),
        f!(
            "static_item",
            "`static X: T = ..`",
            |n| n.kind() == STATIC && !tok(n, T![mut]),
            "static N: i32 = 3; fn main() { println!(\"{}\", N); }"
        ),
        f!(
            "static_mut",
            "`static mut`",
            |n| n.kind() == STATIC && tok(n, T![mut]),
            "static mut N: i32 = 0; fn main() { unsafe { N += 1; let n = N; println!(\"{}\", n); } }"
        ),
        f!(
            "const_underscore",
            "`const _: () = ..`",
            |n| n.kind() == CONST && tok(n, T![_]),
            "const _: () = (); fn main() { println!(\"ok\"); }"
        ),
        f!(
            "const_fn",
            "`const fn`",
            |n| n.kind() == FN && tok(n, T![const]),
            "const fn sq(x: i32) -> i32 { x * x } const N: i32 = sq(3); fn main() { println!(\"{}\", N); }"
        ),
        f!(
            "async_fn",
            "`async fn`",
            |n| n.kind() == FN && tok(n, T![async]) && !in_trait(n),
            "async fn f() -> i32 { 1 } fn main() { let _ = f(); println!(\"ok\"); }"
        ),
        f!(
            "unsafe_fn",
            "`unsafe fn`",
            |n| n.kind() == FN && tok(n, T![unsafe]),
            "unsafe fn f() -> i32 { 1 } fn main() { println!(\"{}\", unsafe { f() }); }"
        ),
        f!(
            "extern_abi_fn",
            "`extern \"C\" fn` (with a body)",
            |n| n.kind() == FN && child(n, ABI).is_some() && child(n, BLOCK_EXPR).is_some(),
            "extern \"C\" fn f() -> i32 { 1 } fn main() { println!(\"{}\", f()); }"
        ),
        f!(
            "unsafe_impl",
            "`unsafe impl`",
            |n| n.kind() == IMPL && tok(n, T![unsafe]),
            "struct S; unsafe impl Send for S {} fn main() { let _s = S; println!(\"ok\"); }"
        ),
        f!(
            "unsafe_trait",
            "`unsafe trait`",
            |n| n.kind() == TRAIT && tok(n, T![unsafe]),
            "unsafe trait T { fn f(&self) -> i32 { 1 } } struct S; unsafe impl T for S {} fn main() { println!(\"{}\", S.f()); }"
        ),
        f!(
            "type_alias_generic",
            "generic type alias `type P<T> = ..`",
            |n| n.kind() == TYPE_ALIAS
                && !in_trait(n)
                && !in_impl(n)
                && child(n, GENERIC_PARAM_LIST).is_some(),
            "type Pair<T> = (T, T); fn main() { let p: Pair<i32> = (1, 2); println!(\"{}\", p.0 + p.1); }"
        ),
        f!(
            "pub_restricted",
            "`pub(crate)` / `pub(super)`",
            |n| n.kind() == VISIBILITY && tok(n, T!['(']),
            "pub(crate) fn f() -> i32 { 1 } fn main() { println!(\"{}\", f()); }"
        ),
        f!(
            "use_glob",
            "glob import `use a::*`",
            |n| n.kind() == USE_TREE && tok(n, T![*]),
            "use std::collections::*; fn main() { let m: HashMap<i32, i32> = HashMap::new(); println!(\"{}\", m.len()); }"
        ),
        f!(
            "use_enum_glob",
            "`use Enum::*` (variants by name)",
            |n| n.kind() == USE_TREE
                && tok(n, T![*])
                && child(n, PATH).is_some_and(|p| path_segments(&p)
                    .last()
                    .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_uppercase()))),
            "#[derive(Debug)] enum E { A, B } use E::*; fn main() { let x = A; match x { A => println!(\"a\"), B => println!(\"b\") } }"
        ),
        f!(
            "use_rename",
            "`use a::B as C`",
            |n| n.kind() == USE_TREE && child(n, RENAME).is_some(),
            "use std::collections::HashMap as Map; fn main() { let m: Map<i32, i32> = Map::new(); println!(\"{}\", m.len()); }"
        ),
        f!(
            "enum_discriminant",
            "enum discriminants `A = 1`",
            |n| n.kind() == VARIANT && tok(n, T![=]),
            "enum C { A = 1, B = 5 } fn main() { println!(\"{} {}\", C::A as i32, C::B as i32); }"
        ),
        f!(
            "repr_attr",
            "`#[repr(..)]`",
            |n| n.kind() == ATTR && n.text().to_string().contains("repr("),
            "#[repr(u8)] #[derive(Clone, Copy)] enum E { A = 1, B = 2 } fn main() { println!(\"{} {}\", E::A as u8, E::B as u8); }"
        ),
        f!(
            "impl_inherent_concrete",
            "inherent impls per instantiation `impl W<i32>` / `impl W<String>`",
            |n| {
                if n.kind() != IMPL || tok(n, T![for]) {
                    return false;
                }
                let params: HashSet<String> = child(n, GENERIC_PARAM_LIST)
                    .map(|g| g.children().map(|p| first_ident(&p)).collect())
                    .unwrap_or_default();
                child(n, PATH_TYPE)
                    .and_then(|t| t.descendants().find(|d| d.kind() == GENERIC_ARG_LIST))
                    .is_some_and(|args| {
                        args.children().any(|a| {
                            a.kind() == CONST_ARG
                                || a.kind() == TYPE_ARG
                                    && !params.contains(a.text().to_string().trim())
                        })
                    })
            },
            "struct W<T>(T); impl W<i32> { fn get(&self) -> i32 { self.0 * 2 } } impl W<String> { fn get(&self) -> usize { self.0.len() } } fn main() { println!(\"{} {}\", W(2).get(), W(String::from(\"ab\")).get()); }"
        ),
        // Traits and impls.
        f!(
            "trait_generic",
            "generic trait `trait Conv<T>`",
            |n| n.kind() == TRAIT && child(n, GENERIC_PARAM_LIST).is_some(),
            "trait Conv<T> { fn conv(&self) -> T; } struct S; impl Conv<i32> for S { fn conv(&self) -> i32 { 1 } } fn main() { println!(\"{}\", S.conv()); }"
        ),
        f!(
            "supertrait",
            "supertraits `trait B: A`",
            |n| n.kind() == TRAIT && child(n, TYPE_BOUND_LIST).is_some(),
            "trait A { fn a(&self) -> i32; } trait B: A { fn b(&self) -> i32 { self.a() + 1 } } struct S; impl A for S { fn a(&self) -> i32 { 1 } } impl B for S {} fn main() { println!(\"{}\", S.b()); }"
        ),
        f!(
            "gat",
            "generic associated type `type It<'a>`",
            |n| n.kind() == TYPE_ALIAS && in_trait(n) && child(n, GENERIC_PARAM_LIST).is_some(),
            "trait L { type It<'a> where Self: 'a; fn first<'a>(&'a self) -> Self::It<'a>; } struct V(Vec<i32>); impl L for V { type It<'a> = &'a i32; fn first<'a>(&'a self) -> &'a i32 { &self.0[0] } } fn main() { let v = V(vec![5]); println!(\"{}\", v.first()); }"
        ),
        f!(
            "assoc_type_bound",
            "associated type bound `I: Iterator<Item: Display>`",
            |n| n.kind() == ASSOC_TYPE_ARG && child(n, TYPE_BOUND_LIST).is_some(),
            "fn f<I: IntoIterator<Item: std::fmt::Display>>(i: I) { for x in i { println!(\"{}\", x); } } fn main() { f(vec![1, 2]); }"
        ),
        f!(
            "assoc_const_trait",
            "associated const in a trait",
            |n| n.kind() == CONST && in_trait(n),
            "trait T { const N: i32; } struct S; impl T for S { const N: i32 = 4; } fn main() { println!(\"{}\", S::N); }"
        ),
        f!(
            "assoc_const_impl",
            "associated const in an impl",
            |n| n.kind() == CONST && in_impl(n),
            "struct S; impl S { const K: i32 = 9; } fn main() { println!(\"{}\", S::K); }"
        ),
        f!(
            "trait_generic_method",
            "generic method in a trait `fn m<U>(..)`",
            |n| n.kind() == FN && in_trait(n) && child(n, GENERIC_PARAM_LIST).is_some(),
            "trait T { fn m<U: std::fmt::Display>(&self, u: U) -> String; } struct S; impl T for S { fn m<U: std::fmt::Display>(&self, u: U) -> String { format!(\"<{}>\", u) } } fn main() { println!(\"{}\", S.m(3)); }"
        ),
        f!(
            "rpitit",
            "`-> impl Trait` in a trait method",
            |n| n.kind() == FN
                && in_trait(n)
                && child(n, RET_TYPE).is_some_and(|r| child(&r, IMPL_TRAIT_TYPE).is_some()),
            "trait T { fn it(&self) -> impl Iterator<Item = i32>; } struct S; impl T for S { fn it(&self) -> impl Iterator<Item = i32> { 0..3 } } fn main() { println!(\"{}\", S.it().count()); }"
        ),
        f!(
            "async_fn_in_trait",
            "`async fn` in a trait",
            |n| n.kind() == FN && tok(n, T![async]) && in_trait(n),
            "trait T { async fn f(&self) -> i32; } struct S; impl T for S { async fn f(&self) -> i32 { 1 } } fn main() { let _ = S.f(); println!(\"ok\"); }"
        ),
        f!(
            "impl_for_ref",
            "`impl Tr for &T`",
            |n| impl_self_ty(n).is_some_and(|t| t.kind() == REF_TYPE),
            "trait T { fn f(&self) -> i32; } impl<'a> T for &'a str { fn f(&self) -> i32 { self.len() as i32 } } fn main() { let s: &str = \"abc\"; println!(\"{}\", s.f()); }"
        ),
        f!(
            "impl_for_tuple",
            "`impl Tr for (A, B)`",
            |n| impl_self_ty(n).is_some_and(|t| t.kind() == TUPLE_TYPE),
            "trait T { fn s(&self) -> i32; } impl T for (i32, i32) { fn s(&self) -> i32 { self.0 + self.1 } } fn main() { let t: (i32, i32) = (1, 2); println!(\"{}\", t.s()); }"
        ),
        f!(
            "impl_for_slice_array",
            "`impl Tr for [T]` / `[T; N]`",
            |n| impl_self_ty(n).is_some_and(|t| matches!(t.kind(), SLICE_TYPE | ARRAY_TYPE)),
            "trait T { fn s(&self) -> i32; } impl T for [i32] { fn s(&self) -> i32 { self.iter().sum() } } fn main() { let v = vec![1, 2]; println!(\"{}\", v[..].s()); }"
        ),
        f!(
            "impl_for_fn_ptr",
            "`impl Tr for fn(..)`",
            |n| impl_self_ty(n).is_some_and(|t| t.kind() == FN_PTR_TYPE),
            "trait T { fn call1(&self) -> i32; } impl T for fn() -> i32 { fn call1(&self) -> i32 { self() } } fn one() -> i32 { 1 } fn main() { let f: fn() -> i32 = one; println!(\"{}\", f.call1()); }"
        ),
        f!(
            "impl_for_primitive",
            "your trait on a number / `str` / `String`",
            |n| {
                impl_self_ty(n).is_some_and(|t| {
                    t.kind() == PATH_TYPE && PRIMS.contains(&first_ident(&t).as_str())
                })
            },
            "trait Dbl { fn dbl(&self) -> Self; } impl Dbl for i32 { fn dbl(&self) -> i32 { self * 2 } } fn main() { println!(\"{}\", 3i32.dbl()); }"
        ),
        f!(
            "impl_for_std_generic",
            "your trait on `Vec<T>` / `Option<T>` / ..",
            |n| impl_self_ty(n).is_some_and(
                |t| t.kind() == PATH_TYPE && STD_GENERIC.contains(&first_ident(&t).as_str())
            ),
            "trait Sz { fn sz(&self) -> usize; } impl Sz for i32 { fn sz(&self) -> usize { 4 } } impl<T: Sz> Sz for Vec<T> { fn sz(&self) -> usize { self.iter().map(|x| x.sz()).sum() } } fn main() { println!(\"{}\", vec![1, 2].sz()); }"
        ),
        f!(
            "blanket_impl",
            "blanket `impl<T: Bound> Tr for T`",
            |n| {
                let Some(t) = impl_self_ty(n) else {
                    return false;
                };
                let Some(g) = child(n, GENERIC_PARAM_LIST) else {
                    return false;
                };
                let name = t.text().to_string();
                t.kind() == PATH_TYPE
                    && g.children()
                        .any(|p| p.kind() == TYPE_PARAM && first_ident(&p) == name)
            },
            "trait D { fn d(&self) -> String; } impl<T: std::fmt::Debug> D for T { fn d(&self) -> String { format!(\"{:?}\", self) } } fn main() { println!(\"{}\", 3.d()); }"
        ),
        f!(
            "qualified_path",
            "`<T as Trait>::item`",
            |n| n.kind() == PATH_SEGMENT && tok(n, T![<]) && tok(n, T![as])
                || n.kind() == TYPE_ANCHOR && tok(n, T![as]),
            "trait T { fn n() -> i32; } struct S; impl T for S { fn n() -> i32 { 7 } } fn main() { println!(\"{}\", <S as T>::n()); }"
        ),
        // Generics and types.
        f!(
            "generic_default",
            "default type parameter `<T = i32>`",
            |n| n.kind() == TYPE_PARAM && tok(n, T![=]),
            "struct W<T = i32>(T); fn main() { let w: W = W(3); println!(\"{}\", w.0); }"
        ),
        f!(
            "const_generic",
            "const generics `<const N: usize>`",
            |n| n.kind() == CONST_PARAM,
            "struct A<const N: usize>([i32; N]); fn main() { let a = A([1, 2, 3]); println!(\"{}\", a.0.len()); }"
        ),
        f!(
            "const_generic_expr",
            "const argument expression `f::<{ N + 1 }>`",
            |n| n.kind() == CONST_ARG && child(n, BLOCK_EXPR).is_some(),
            "fn f<const N: usize>() -> usize { N } fn main() { println!(\"{}\", f::<{ 2 + 1 }>()); }"
        ),
        f!(
            "hrtb",
            "higher-ranked bound `for<'a> Fn(&'a T)`",
            |n| n.kind() == FOR_BINDER,
            "fn ap<F>(f: F) -> usize where F: for<'a> Fn(&'a str) -> usize { f(\"abc\") } fn main() { println!(\"{}\", ap(|s| s.len())); }"
        ),
        f!(
            "maybe_sized",
            "`?Sized`",
            |n| n.kind() == TYPE_BOUND && tok(n, T![?]),
            "fn show<T: ?Sized + std::fmt::Display>(t: &T) -> String { t.to_string() } fn main() { println!(\"{}\", show(\"x\")); }"
        ),
        f!(
            "lifetime_bound_on_type",
            "lifetime bound on a type `T: 'a`",
            |n| n.kind() == TYPE_PARAM
                && child(n, TYPE_BOUND_LIST)
                    .is_some_and(|b| b.descendants().any(|d| d.kind() == LIFETIME)),
            "struct R<'a, T: 'a> { r: &'a T } fn main() { let x = 3; let r = R { r: &x }; println!(\"{}\", r.r); }"
        ),
        f!(
            "impl_trait_arg",
            "`impl Trait` argument",
            |n| n.kind() == IMPL_TRAIT_TYPE && n.ancestors().any(|a| a.kind() == PARAM),
            "fn f(i: impl Iterator<Item = i32>) -> i32 { i.sum() } fn main() { println!(\"{}\", f(0..3)); }"
        ),
        f!(
            "impl_trait_ret",
            "`-> impl Trait`",
            |n| n.kind() == IMPL_TRAIT_TYPE && parent_is(n, RET_TYPE),
            "fn f() -> impl Iterator<Item = i32> { (0..3).map(|x| x * 2) } fn main() { println!(\"{}\", f().sum::<i32>()); }"
        ),
        f!(
            "dyn_multi_bound",
            "`dyn A + Send`",
            |n| n.kind() == DYN_TRAIT_TYPE
                && child(n, TYPE_BOUND_LIST).is_some_and(|b| b
                    .children()
                    .filter(|c| c.kind() == TYPE_BOUND && child(c, LIFETIME).is_none())
                    .count()
                    > 1),
            "fn main() { let b: Box<dyn Fn() -> i32 + Send> = Box::new(|| 3); println!(\"{}\", b()); }"
        ),
        f!(
            "dyn_assoc_type",
            "`dyn Iterator<Item = T>`",
            |n| n.kind() == DYN_TRAIT_TYPE && n.descendants().any(|d| d.kind() == ASSOC_TYPE_ARG),
            "fn main() { let it: Box<dyn Iterator<Item = i32>> = Box::new(0..3); println!(\"{}\", it.sum::<i32>()); }"
        ),
        f!(
            "fn_ptr_type",
            "function pointer type `fn(i32) -> i32`",
            |n| n.kind() == FN_PTR_TYPE,
            "fn d(x: i32) -> i32 { x * 2 } fn main() { let f: fn(i32) -> i32 = d; println!(\"{}\", f(4)); }"
        ),
        f!(
            "never_type",
            "never type `-> !`",
            |n| n.kind() == NEVER_TYPE,
            "fn die() -> ! { panic!(\"x\") } fn main() { if false { die(); } println!(\"ok\"); }"
        ),
        f!(
            "macro_type",
            "macro in type position",
            |n| n.kind() == MACRO_TYPE,
            "macro_rules! int { () => { i32 } } fn main() { let x: int!() = 3; println!(\"{}\", x); }"
        ),
        // Statements and expressions.
        f!(
            "let_else",
            "`let .. else`",
            |n| n.kind() == LET_ELSE,
            "fn f(o: Option<i32>) -> i32 { let Some(x) = o else { return 0; }; x } fn main() { println!(\"{}\", f(Some(2))); }"
        ),
        f!(
            "while_let",
            "`while let`",
            |n| n.kind() == WHILE_EXPR && child(n, LET_EXPR).is_some(),
            "fn main() { let mut v = vec![1, 2]; while let Some(x) = v.pop() { println!(\"{}\", x); } }"
        ),
        f!(
            "labeled_loop",
            "labeled loops `'outer: for`",
            |n| matches!(n.kind(), FOR_EXPR | WHILE_EXPR | LOOP_EXPR) && child(n, LABEL).is_some(),
            "fn main() { 'o: for i in 0..3 { for j in 0..3 { if j == 1 { continue 'o; } if i == 2 { break 'o; } println!(\"{} {}\", i, j); } } }"
        ),
        f!(
            "labeled_block",
            "labeled block `'b: { break 'b x }`",
            |n| n.kind() == BLOCK_EXPR && child(n, LABEL).is_some(),
            "fn main() { let x = 'b: { if true { break 'b 1; } 2 }; println!(\"{}\", x); }"
        ),
        f!(
            "loop_break_value",
            "`break value` out of a `loop`",
            |n| n.kind() == BREAK_EXPR
                && n.children().any(|c| c.kind() != LIFETIME)
                && n.ancestors()
                    .skip(1)
                    .find(|a| matches!(
                        a.kind(),
                        LOOP_EXPR | FOR_EXPR | WHILE_EXPR | CLOSURE_EXPR | FN
                    ) || a.kind() == BLOCK_EXPR && child(a, LABEL).is_some())
                    .is_some_and(|a| a.kind() == LOOP_EXPR),
            "fn main() { let mut i = 0; let x = loop { i += 1; if i == 3 { break i * 2; } }; println!(\"{}\", x); }"
        ),
        f!(
            "async_block",
            "`async { .. }` block",
            |n| n.kind() == BLOCK_EXPR && tok(n, T![async]),
            "fn main() { let _f = async { 1 }; println!(\"ok\"); }"
        ),
        f!(
            "async_closure",
            "`async |x| ..` closure",
            |n| n.kind() == CLOSURE_EXPR && tok(n, T![async]),
            "fn main() { let _c = async |x: i32| x + 1; println!(\"ok\"); }"
        ),
        f!(
            "await_expr",
            "`.await`",
            |n| n.kind() == AWAIT_EXPR,
            "async fn one() -> i32 { 1 } async fn two() -> i32 { one().await + 1 } fn main() { let _ = two(); println!(\"ok\"); }"
        ),
        f!(
            "unsafe_block",
            "`unsafe { .. }` block",
            |n| n.kind() == BLOCK_EXPR && tok(n, T![unsafe]),
            "unsafe fn f() -> i32 { 2 } fn main() { let x = unsafe { f() }; println!(\"{}\", x); }"
        ),
        f!(
            "const_block",
            "inline `const { .. }` block",
            |n| n.kind() == BLOCK_EXPR && tok(n, T![const]),
            "fn main() { let x = const { 1 + 2 }; println!(\"{}\", x); }"
        ),
        f!(
            "try_operator",
            "`?` operator",
            |n| n.kind() == TRY_EXPR,
            "fn p(s: &str) -> Result<i32, std::num::ParseIntError> { let n: i32 = s.parse()?; Ok(n + 1) } fn main() { println!(\"{:?}\", p(\"4\")); }"
        ),
        f!(
            "try_in_closure",
            "`?` inside a closure",
            |n| n.kind() == TRY_EXPR
                && n.ancestors()
                    .find(|a| matches!(a.kind(), CLOSURE_EXPR | FN))
                    .is_some_and(|a| a.kind() == CLOSURE_EXPR),
            "fn main() { let f = |s: &str| -> Result<i32, std::num::ParseIntError> { Ok(s.parse::<i32>()? * 2) }; println!(\"{:?}\", f(\"3\")); }"
        ),
        f!(
            "move_closure",
            "`move` closure",
            |n| n.kind() == CLOSURE_EXPR && tok(n, T![move]),
            "fn mk(n: i32) -> impl Fn(i32) -> i32 { move |x| x + n } fn main() { println!(\"{}\", mk(2)(3)); }"
        ),
        f!(
            "mut_closure_capture",
            "closure mutating a captured variable",
            |n| n.kind() == LET_STMT
                && child(n, IDENT_PAT).is_some_and(|p| tok(&p, T![mut]))
                && child(n, CLOSURE_EXPR).is_some(),
            "fn main() { let mut n = 0; let mut inc = || n += 1; inc(); inc(); println!(\"{}\", n); }"
        ),
        f!(
            "struct_update",
            "struct update `S { x, ..base }`",
            |n| n.kind() == RECORD_EXPR_FIELD_LIST && tok(n, T![..]),
            "#[derive(Clone, Debug)] struct P { x: i32, y: i32 } fn main() { let a = P { x: 1, y: 2 }; let b = P { x: 5, ..a.clone() }; println!(\"{:?}\", b); }"
        ),
        f!(
            "ctor_as_fn",
            "tuple struct / variant as a function `.map(Some)`",
            |n| n.kind() == PATH_EXPR && parent_is(n, ARG_LIST) && {
                let s = path_segments(n);
                s.len() == 1
                    && s[0].starts_with(|c: char| c.is_ascii_uppercase())
                    && !is_screaming(&s[0])
            },
            "struct W(i32); fn main() { let v: Vec<W> = vec![1, 2].into_iter().map(W).collect(); println!(\"{}\", v[1].0); }"
        ),
        f!(
            "method_as_fn",
            "method path as a function `.map(str::len)`",
            |n| n.kind() == PATH_EXPR && parent_is(n, ARG_LIST) && {
                let s = path_segments(n);
                s.len() >= 2
                    && s.last()
                        .is_some_and(|l| l.starts_with(|c: char| c.is_ascii_lowercase()))
            },
            "fn main() { let v: Vec<usize> = vec![\"a\", \"bb\"].into_iter().map(str::len).collect(); println!(\"{:?}\", v); }"
        ),
        f!(
            "format_inline_args",
            "inline format arguments `\"{x}\"`",
            |n| n.kind() == MACRO_CALL && {
                let name = n
                    .children()
                    .find(|c| c.kind() == PATH)
                    .map(|p| p.text().to_string())
                    .unwrap_or_default();
                matches!(
                    name.rsplit("::").next(),
                    Some(
                        "println"
                            | "print"
                            | "format"
                            | "write"
                            | "writeln"
                            | "panic"
                            | "eprintln"
                            | "eprint"
                            | "assert"
                            | "assert_eq"
                    )
                ) && n.descendants_with_tokens().any(|t| {
                    t.kind() == STRING && t.as_token().is_some_and(|t| has_inline_arg(t.text()))
                })
            },
            "fn main() { let x = 3; let w = 4; println!(\"{x} {x:>w$} {}\", x + 1); }"
        ),
        // Patterns.
        f!(
            "range_pat",
            "range pattern `'a'..='z'`",
            |n| n.kind() == RANGE_PAT && tok(n, T![..=]),
            "fn main() { let c = 'q'; match c { 'a'..='z' => println!(\"lower\"), _ => println!(\"other\") } }"
        ),
        f!(
            "range_pat_exclusive",
            "exclusive / half-open range pattern `0..10`, `10..`",
            |n| n.kind() == RANGE_PAT && !tok(n, T![..=]),
            "fn main() { match 15 { 0..10 => println!(\"small\"), 10.. => println!(\"big\"), _ => println!(\"neg\") } }"
        ),
        f!(
            "slice_pat",
            "slice pattern `[first, .., last]`",
            |n| n.kind() == SLICE_PAT,
            "fn main() { let v = [1, 2, 3]; match v { [first, .., last] => println!(\"{} {}\", first, last) } }"
        ),
        f!(
            "slice_rest_binding",
            "`rest @ ..` in a slice pattern",
            |n| n.kind() == IDENT_PAT && tok(n, T![@]) && child(n, REST_PAT).is_some(),
            "fn f(v: &[i32]) -> i32 { match v { [h, rest @ ..] => h + f(rest), [] => 0 } } fn main() { println!(\"{}\", f(&[1, 2, 3])); }"
        ),
        f!(
            "at_binding",
            "`name @ pattern`",
            |n| n.kind() == IDENT_PAT && tok(n, T![@]) && child(n, REST_PAT).is_none(),
            "fn main() { match 7 { n @ 1..=9 => println!(\"{}\", n), _ => {} } }"
        ),
        f!(
            "nested_or_pat",
            "nested or-pattern `Some(1 | 2)`",
            |n| n.kind() == OR_PAT
                && !parent_is(n, MATCH_ARM)
                && !parent_is(n, LET_EXPR)
                && !parent_is(n, LET_STMT)
                && !parent_is(n, PARAM),
            "fn main() { let v = Some(2); if let Some(1 | 2) = v { println!(\"yes\"); } }"
        ),
        f!(
            "ref_pat",
            "`&x` pattern",
            |n| n.kind() == REF_PAT,
            "fn main() { let v = vec![1, 2]; let s: i32 = v.iter().map(|&x| x * 2).sum(); println!(\"{}\", s); }"
        ),
        f!(
            "ref_binding",
            "`ref` / `ref mut` binding",
            |n| n.kind() == IDENT_PAT && tok(n, T![ref]),
            "fn main() { let t = (String::from(\"a\"), 1); let (ref s, n) = t; println!(\"{} {}\", s, n); }"
        ),
        f!(
            "const_pat",
            "named constant as a pattern",
            |n| n.kind() == PATH_PAT && {
                let s = path_segments(n);
                s.len() == 1 && is_screaming(&s[0])
            },
            "const LIMIT: i32 = 3; fn main() { match 3 { LIMIT => println!(\"limit\"), _ => println!(\"no\") } }"
        ),
        f!(
            "assoc_const_pat",
            "associated const as a pattern `Self::MAX`",
            |n| n.kind() == PATH_PAT && {
                let s = path_segments(n);
                s.len() >= 2 && s.last().is_some_and(|l| is_screaming(l))
            },
            "struct S; impl S { const Z: i32 = 0; } fn main() { match 0 { S::Z => println!(\"z\"), _ => println!(\"nz\") } }"
        ),
        f!(
            "record_pat_rest",
            "`S { x, .. }` pattern",
            |n| n.kind() == RECORD_PAT_FIELD_LIST && n.children().any(|c| c.kind() == REST_PAT),
            "struct P { x: i32, y: i32 } fn main() { let p = P { x: 1, y: 2 }; let P { x, .. } = p; println!(\"{} {}\", x, p.y); }"
        ),
        f!(
            "tuple_rest_pat",
            "`(a, .., z)` / `T(a, ..)` patterns",
            |n| matches!(n.kind(), TUPLE_PAT | TUPLE_STRUCT_PAT)
                && n.children().any(|c| c.kind() == REST_PAT),
            "struct T(i32, i32, i32); fn main() { let T(a, ..) = T(1, 2, 3); let (b, .., z) = (1, 2, 3, 4); println!(\"{} {} {}\", a, b, z); }"
        ),
        f!(
            "macro_pat",
            "macro in pattern position",
            |n| n.kind() == MACRO_PAT,
            "macro_rules! one { () => { 1 } } fn main() { match 1 { one!() => println!(\"one\"), _ => {} } }"
        ),
        // Literals.
        f!(
            "raw_identifier",
            "raw identifier `r#type`",
            |n| n
                .children_with_tokens()
                .any(|t| t.kind() == IDENT
                    && t.as_token().is_some_and(|t| t.text().starts_with("r#"))),
            "fn main() { let r#type = 3; println!(\"{}\", r#type); }"
        ),
        f!(
            "byte_string",
            "byte string `b\"..\"`",
            |n| lit_starts(n, BYTE_STRING, "b"),
            "fn main() { let b = b\"hi\"; println!(\"{}\", b.len()); }"
        ),
        f!(
            "byte_char",
            "byte literal `b'a'`",
            |n| lit_starts(n, BYTE, "b"),
            "fn main() { let b = b'a'; println!(\"{}\", b); }"
        ),
        f!(
            "c_string",
            "C string `c\"..\"`",
            |n| lit_starts(n, C_STRING, "c"),
            "fn main() { let c = c\"hi\"; println!(\"{}\", c.to_bytes().len()); }"
        ),
        f!(
            "raw_string",
            "raw string `r#\"..\"#`",
            |n| lit_starts(n, STRING, "r"),
            "fn main() { let s = r#\"a \"q\" b\"#; println!(\"{}\", s); }"
        ),
        // Not planned.
        f!(np "union", "`union`", |n| n.kind() == UNION,
            "union U { a: u32, b: f32 } fn main() { let u = U { a: 1 }; println!(\"{}\", unsafe { u.a }); }"),
        f!(np "extern_block", "`extern { .. }` block (FFI)", |n| n.kind() == EXTERN_BLOCK,
            "unsafe extern \"C\" { fn abs(x: i32) -> i32; } fn main() { println!(\"{}\", unsafe { abs(-3) }); }"),
        f!(np "raw_pointer", "raw pointer types `*const T`", |n| n.kind() == PTR_TYPE,
            "fn main() { let x = 3; let p: *const i32 = &x; println!(\"{}\", unsafe { *p }); }"),
        f!(np "raw_ref", "`&raw const x`", |n| n.kind() == REF_EXPR && tok(n, T![raw]),
            "fn main() { let x = 3; let p = &raw const x; println!(\"{}\", unsafe { *p }); }"),
    ]
}

/// A `{name}` / `{name:..}` argument in a format string (not `{}`, `{0}`, `{:?}` or `{{`).
fn has_inline_arg(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'{' {
            if b.get(i + 1) == Some(&b'{') {
                i += 2;
                continue;
            }
            if b.get(i + 1)
                .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Names the file itself defines or imports from std: items, variants, fields' types aside,
/// generic parameters, and `use std::..` leaves. A capitalized name outside these (or a name a
/// non-std `use` brings in) is from another crate or file.
struct Scope {
    local: HashSet<String>,
    foreign: HashSet<String>,
}
const STD_NAMES: &[&str] = &[
    "Self",
    "Some",
    "None",
    "Ok",
    "Err",
    "Option",
    "Result",
    "Vec",
    "String",
    "Box",
    "Default",
    "Clone",
    "Copy",
    "Debug",
    "Display",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Iterator",
    "IntoIterator",
    "From",
    "Into",
    "TryFrom",
    "TryInto",
    "AsRef",
    "AsMut",
    "Fn",
    "FnMut",
    "FnOnce",
    "Drop",
    "Send",
    "Sync",
    "Sized",
    "ToString",
    "ToOwned",
    "Extend",
    "FromIterator",
    "DoubleEndedIterator",
    "ExactSizeIterator",
];
fn scope_of(file: &SyntaxNode) -> Scope {
    let mut local: HashSet<String> = STD_NAMES
        .iter()
        .chain(PRIMS)
        .map(|s| s.to_string())
        .collect();
    let mut foreign = HashSet::new();
    for n in file.descendants() {
        match n.kind() {
            FN | STRUCT | ENUM | UNION | TRAIT | CONST | STATIC | TYPE_ALIAS | MODULE
            | MACRO_RULES | VARIANT | TYPE_PARAM | CONST_PARAM | RECORD_FIELD => {
                if let Some(name) = child(&n, NAME) {
                    local.insert(name.text().to_string());
                }
            }
            USE => {
                let text = n.text().to_string();
                let path = text
                    .trim_start_matches("pub")
                    .trim_start()
                    .trim_start_matches(|c| c != 'u');
                let path = path
                    .trim_start_matches("use")
                    .trim()
                    .trim_start_matches("::");
                let is_std = ["std", "core", "alloc"].iter().any(|r| {
                    path.starts_with(&format!("{r}::")) || path.starts_with(&format!("{r}{{"))
                });
                for leaf in n
                    .descendants()
                    .filter(|d| d.kind() == USE_TREE && child(d, USE_TREE_LIST).is_none())
                {
                    let name = match child(&leaf, RENAME) {
                        Some(r) => first_ident(&r),
                        None => child(&leaf, PATH)
                            .map(|p| path_segments(&p).last().cloned().unwrap_or_default())
                            .unwrap_or_default(),
                    };
                    if name.is_empty() || name == "self" {
                        continue;
                    }
                    if is_std {
                        local.insert(name);
                    } else {
                        foreign.insert(name);
                    }
                }
            }
            _ => {}
        }
    }
    Scope { local, foreign }
}
/// Names in `n` from another crate or file (first path segments only).
fn ext_names(n: &SyntaxNode, scope: &Scope) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in n
        .descendants()
        .filter(|d| d.kind() == PATH && !parent_is(d, PATH))
    {
        let segs = path_segments(&p);
        let Some(first) = segs.first() else { continue };
        // `crate::x` / `super::x` are in other files; `path_segments` skips keyword segments, so
        // look at the text.
        let text = p.text().to_string();
        if text.starts_with("crate::") || text.starts_with("super::") {
            out.push(text);
            continue;
        }
        if text.starts_with("self::") || matches!(first.as_str(), "" | "std" | "core" | "alloc") {
            continue;
        }
        let foreign = scope.foreign.contains(first)
            || first.starts_with(|c: char| c.is_ascii_uppercase()) && !scope.local.contains(first);
        if foreign && !out.contains(first) {
            out.push(first.clone());
        }
    }
    out.truncate(4);
    out
}

/// Byte offsets of the start of each line, to turn a classifier's (line, column) into an offset
/// when it has no byte offsets.
fn offset_of(src: &str, line: u64) -> usize {
    src.split_inclusive('\n')
        .take(line.saturating_sub(1) as usize)
        .map(str::len)
        .sum()
}

/// The cargo registry's source directory (`$CARGO_HOME/registry/src/index.*`), as the classifier
/// finds it.
fn registry_root() -> String {
    let home = std::env::var("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let h = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            std::path::Path::new(&h).join(".cargo")
        });
    let src = home.join("registry").join("src");
    let dirs: Vec<_> = std::fs::read_dir(&src)
        .map(|d| {
            d.flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("index."))
                .collect()
        })
        .unwrap_or_default();
    match &dirs[..] {
        [d] => d.path().to_string_lossy().into_owned(),
        _ => panic!(
            "pass the root: found {} registry indexes in {}",
            dirs.len(),
            src.display()
        ),
    }
}

fn run_probes(feats: &[Feature]) -> Vec<Value> {
    feats
        .iter()
        .map(|f| {
            let out = std::panic::catch_unwind(|| carciniscript::compile(f.probe, "probe.rs"));
            let (ok, err) = match out {
                Ok(o) => {
                    let e = o.diagnostics.iter().find(|d| d.severity == "error").map(|d| d.message.clone());
                    (o.code.is_some() && e.is_none(), e)
                }
                Err(_) => (false, Some("compiler panic".to_string())),
            };
            json!({ "id": f.id, "label": f.label, "not_planned": f.not_planned, "ok": ok, "error": err })
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let feats = features();
    // `--write-probes <dir>`: each probe as `<dir>/<id>.rs`, to check them with rustc.
    if args.first().map(String::as_str) == Some("--write-probes") {
        for f in &feats {
            std::fs::write(
                std::path::Path::new(&args[1]).join(format!("{}.rs", f.id)),
                f.probe,
            )
            .unwrap();
        }
        return;
    }
    std::panic::set_hook(Box::new(|_| {}));
    let probes = run_probes(&feats);
    if args.first().map(String::as_str) == Some("--probes") {
        for p in &probes {
            println!(
                "{} {:<24} {}",
                if p["ok"] == true { "ok  " } else { "MISS" },
                p["id"].as_str().unwrap(),
                p["error"].as_str().unwrap_or("")
            );
        }
        return;
    }
    // With no arguments: the cargo registry, and the classifier's default output in the temp dir.
    let tmp = std::env::temp_dir();
    let (root, input, output) = match &args[..] {
        [root, input, output] => (root.clone(), input.clone(), output.clone()),
        [] => (
            registry_root(),
            tmp.join("carciniscript-corpus.jsonl")
                .to_string_lossy()
                .into_owned(),
            tmp.join("carciniscript-corpus.ast.jsonl")
                .to_string_lossy()
                .into_owned(),
        ),
        _ => {
            eprintln!(
                "usage: corpus_ast [<root> <classified.jsonl> <out.jsonl>] | --probes | --write-probes <dir>"
            );
            std::process::exit(2);
        }
    };
    let (root, input, output) = (&root, &input, &output);
    println!("adding syntax to {input} -> {output}");
    std::fs::write(
        format!("{output}.probes.json"),
        serde_json::to_string_pretty(&probes).unwrap(),
    )
    .unwrap();
    let mut out = std::io::BufWriter::new(std::fs::File::create(output).unwrap());
    let reader = std::io::BufReader::new(std::fs::File::open(input).unwrap());
    for line in reader.lines() {
        let line = line.unwrap();
        if line.is_empty() {
            continue;
        }
        let mut rec: Value = serde_json::from_str(&line).unwrap();
        let path = std::path::Path::new(root).join(rec["f"].as_str().unwrap());
        let Ok(src) = std::fs::read_to_string(&path) else {
            writeln!(out, "{rec}").unwrap();
            continue;
        };
        let file = SourceFile::parse(&src, Edition::Edition2021).syntax_node();
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        for n in file.descendants() {
            for f in &feats {
                if (f.detect)(&n) {
                    *counts.entry(f.id).or_default() += 1;
                }
            }
        }
        rec["feats"] = json!(counts);
        let scope = scope_of(&file);
        if let Some(errs) = rec["errs"].as_array_mut() {
            for e in errs.iter_mut() {
                let start = e["start"]
                    .as_u64()
                    .map(|s| s as usize)
                    .unwrap_or_else(|| offset_of(&src, e["line"].as_u64().unwrap_or(1)));
                let end = e["end"]
                    .as_u64()
                    .map(|s| s as usize)
                    .unwrap_or(start)
                    .max(start);
                if end > src.len() || !src.is_char_boundary(start) || !src.is_char_boundary(end) {
                    continue;
                }
                let range =
                    TextRange::new(TextSize::from(start as u32), TextSize::from(end as u32));
                let elem = file.covering_element(range);
                let node = match elem {
                    NodeOrToken::Node(n) => n,
                    NodeOrToken::Token(t) => t.parent().unwrap(),
                };
                let kinds: Vec<String> = node
                    .ancestors()
                    .take(4)
                    .map(|a| format!("{:?}", a.kind()))
                    .collect();
                // The construct the error is on: the covering node, or the one a name, path or
                // literal it's on belongs to. Only that one is checked against the features, so
                // an error on `x.f()` isn't put down to the `?` after it or the `impl` around it.
                let mut at = node.clone();
                while matches!(
                    at.kind(),
                    NAME | NAME_REF
                        | PATH
                        | PATH_SEGMENT
                        | LITERAL
                        | TOKEN_TREE
                        | LIFETIME
                        | LABEL
                        | GENERIC_ARG_LIST
                        | GENERIC_PARAM_LIST
                        | RET_TYPE
                        | PARAM_LIST
                ) && let Some(p) = at.parent()
                {
                    at = p;
                }
                let feat = feats.iter().find(|f| (f.detect)(&at)).map(|f| f.id);
                // Whether the item it's in is under a `#[cfg]` (the same item may be defined
                // again under another one).
                let item = node.ancestors().find(|a| {
                    matches!(
                        a.kind(),
                        FN | STRUCT
                            | ENUM
                            | UNION
                            | IMPL
                            | TRAIT
                            | CONST
                            | STATIC
                            | TYPE_ALIAS
                            | MODULE
                            | USE
                            | MACRO_RULES
                    )
                });
                let cfg = item.is_some_and(|i| {
                    i.children()
                        .any(|c| c.kind() == ATTR && c.text().to_string().contains("cfg"))
                });
                let text: String = node.text().to_string().chars().take(80).collect();
                e["node"] = json!({ "kinds": kinds, "feat": feat, "cfg": cfg, "text": text.split_whitespace().collect::<Vec<_>>().join(" ") });
                e["ext"] = json!(ext_names(&node, &scope));
                // Names from elsewhere in the error's context: its statement, the signature of the
                // function it's in, and the header of the impl that is in (`self`'s type).
                let mut ctx: Vec<String> = Vec::new();
                let stmt = node.ancestors().find(|a| {
                    matches!(a.kind(), EXPR_STMT | LET_STMT)
                        || a.parent().is_some_and(|p| p.kind() == STMT_LIST)
                });
                let func = node
                    .ancestors()
                    .find(|a| matches!(a.kind(), FN | CLOSURE_EXPR));
                let imp = node.ancestors().find(|a| a.kind() == IMPL);
                for part in [
                    stmt,
                    func.as_ref().and_then(|f| child(f, PARAM_LIST)),
                    func.as_ref().and_then(|f| child(f, RET_TYPE)),
                ] {
                    if let Some(p) = part {
                        ctx.extend(ext_names(&p, &scope));
                    }
                }
                if let Some(imp) = imp {
                    for c in imp.children().filter(|c| {
                        matches!(
                            c.kind(),
                            PATH_TYPE | REF_TYPE | TUPLE_TYPE | ARRAY_TYPE | SLICE_TYPE
                        )
                    }) {
                        ctx.extend(ext_names(&c, &scope));
                    }
                }
                ctx.dedup();
                ctx.truncate(4);
                e["ctx_ext"] = json!(ctx);
            }
        }
        writeln!(out, "{rec}").unwrap();
    }
}
