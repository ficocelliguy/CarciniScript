//! The bundled preludes (`stdtime`, `stdgaps`) trimmed to the items a program reaches. A
//! prelude's output depends on the program it's compiled with (renamed locals, impls joining
//! the program's `$ext$i32` class, generic bodies checked against the program's impls), so it's
//! checked and emitted with each compile: chrono's 2700 lines cost more than most programs.
//!
//! Reachability is by name: an identifier set seeded from the program's text (and its `mod`
//! files') grows with each item it reaches, until nothing new is reached. The invariant it
//! rests on: a value of a prelude type exists only where reached text names the type (a
//! signature, a field, an associated type), since every type-producing position of a kept item
//! is among its words. So a struct, enum, trait, free function, type alias, const or static is
//! reached when its name is; an `impl` when every prelude type in its header is (`impl
//! Add<ChronoTimeDelta> for ChronoNaiveDate` needs both: `d + delta` needs a delta) and its
//! trait is in use, which for a prelude trait means named or one of its methods called
//! (`d.weekday()` reaches `impl ChronoDatelike for ChronoNaiveDate` without naming the trait;
//! a bound's trait isn't required, so `impl<Tz: ChronoTimeZone> ChronoDateTime<Tz>` doesn't
//! wait on `ChronoTimeZone`). A kept trait impl keeps every method, since operators, `for`,
//! `{}` and `?` call them without naming them; an inherent impl's methods go by name (so
//! `d.year()` keeps `year` and what its body reaches, and `and_hms_opt` with
//! `ChronoNaiveDateTime` stays out). `FromStr` impls carry the whole parser and are used only
//! by `.parse()`, `from_str` or the trait's name. Names the compiler reaches on its own are
//! `IMPLICIT`.
//!
//! The analysis behind hover and completions keeps inherent methods whole: a completion list
//! is every method of the receiver's type.
//!
//! Each prelude is parsed once per thread; a compile scans the program for identifiers and
//! walks the parsed items.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ra_ap_syntax::{
    AstNode, NodeOrToken, SyntaxKind, SyntaxNode,
    ast::{self, HasGenericParams, HasName},
};

/// Names the compiler reaches on its own, without the program's text naming them: the
/// `.entry()` helper, `write!` into a `fmt::Write` / `io::Write` (`__write_str`), the types it
/// makes for a `JoinHandle`'s result or `serde_json::from_str`, `rng.gen::<Adt>()` through
/// `RandStandard`, the `jsx!` attribute marker traits, the `json!` value constructors the
/// `JSON` schema names, and what the runtime calls back into (`e.__m()` on an entry, the
/// context a hand-written future is polled with and the
/// waker it wakes when a polled promise settles).
const IMPLICIT: &[&str] = &[
    "MinMaxResult",
    "FoldWhile",
    "ExactlyOneError",
    "__std_map_entry",
    "__m",
    "__k",
    "StdMapEntry",
    "__sj_write",
    "__sj_null",
    "__sj_bool",
    "__sj_num",
    "__sj_str",
    "__sj_arr",
    "__sj_obj",
    "__SjIndex",
    "Value",
    "Number",
    "__write_str",
    "FmtWrite",
    "IoWrite",
    "IoRead",
    "IoBufRead",
    "IoError",
    "TaskPoll",
    "__task_context",
    "wake_by_ref",
    "TokioJoinError",
    "RandStandard",
    "RandStandardNormal",
    "RandExp1",
    "RandOpen01",
    "RandOpenClosed01",
    "ReactBool",
    "ReactKey",
    "ReactProps",
    "ReactStyle",
    "ReactText",
    "ReactTextOrNum",
    "ReactIntoNode",
    "__push",
    "__tt",
];

/// A method of an `impl`: where it is in the impl's text, its name, and the identifiers it
/// mentions.
struct Method {
    start: usize,
    end: usize,
    name: &'static str,
    words: Vec<&'static str>,
}

enum Kind {
    /// A struct, enum, union, trait, free function, type alias, const or static, with the
    /// identifiers it mentions.
    Named {
        name: &'static str,
        words: Vec<&'static str>,
        /// A trait's methods (default bodies included): a call of one reaches the trait's
        /// impls without naming the trait.
        trait_methods: Vec<&'static str>,
    },
    Impl {
        /// The identifiers of the self type and the trait (with its arguments), less the
        /// impl's own type parameters: `impl<Tz: ChronoTimeZone> Add<ChronoTimeDelta> for
        /// ChronoDateTime<Tz>` has `Add`, `ChronoTimeDelta`, `ChronoDateTime`. The impl is
        /// reached when each of these that's a prelude item is (a bound's trait isn't
        /// required: `ChronoTimeZone` is reached through the impls that join it).
        header_names: Vec<&'static str>,
        /// The trait's base name, for a trait impl.
        trait_name: Option<&'static str>,
        /// The identifiers in the header and the non-method members: what keeping the impl
        /// reaches.
        words: Vec<&'static str>,
        methods: Vec<Method>,
        /// The body holds nothing but the methods: with all of them dropped, so is the impl.
        only_methods: bool,
    },
}

struct Item {
    text: &'static str,
    kind: Kind,
}

enum Part {
    /// Text between items (comments, blank lines, the odd `static`): kept as is.
    Text(&'static str),
    Item(usize),
}

struct Parsed {
    parts: Vec<Part>,
    items: Vec<Item>,
}

thread_local! {
    static PARSED: RefCell<HashMap<usize, Rc<Parsed>>> = RefCell::new(HashMap::new());
}

/// The identifiers of `s`, as slices of it.
fn words_of<'a>(s: &'a str, out: &mut Vec<&'a str>) {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut from = 0;
    while from < s.len() {
        let Some(k) = s[from..].find(ident) else {
            break;
        };
        let start = from + k;
        let len = s[start..]
            .find(|c: char| !ident(c))
            .unwrap_or(s.len() - start);
        if !s.as_bytes()[start].is_ascii_digit() {
            out.push(&s[start..start + len]);
        }
        from = start + len;
    }
}

/// The identifiers `node` mentions outside the `skip` ranges (relative to `base`, the start of
/// `text` in the file): its identifier tokens, and the words of its string literals (a snippet
/// of JS may name a helper).
fn node_words(
    node: &SyntaxNode,
    text: &'static str,
    base: usize,
    skip: &[(usize, usize)],
) -> Vec<&'static str> {
    let mut out = Vec::new();
    for t in node.descendants_with_tokens() {
        let NodeOrToken::Token(t) = t else { continue };
        let r = t.text_range();
        let (a, b) = (usize::from(r.start()) - base, usize::from(r.end()) - base);
        if skip.iter().any(|&(s, e)| a >= s && b <= e) {
            continue;
        }
        match t.kind() {
            SyntaxKind::IDENT => out.push(&text[a..b]),
            SyntaxKind::STRING | SyntaxKind::BYTE_STRING | SyntaxKind::C_STRING => {
                words_of(&text[a..b], &mut out);
            }
            _ => {}
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The base name of a type: `ChronoDateTime<Tz>` is `ChronoDateTime`, `&mut T` is `T`.
fn base_name(ty: &ast::Type, text: &'static str) -> Option<&'static str> {
    match ty {
        ast::Type::PathType(p) => {
            let r = p.path()?.segment()?.name_ref()?.syntax().text_range();
            Some(&text[usize::from(r.start())..usize::from(r.end())])
        }
        ast::Type::RefType(r) => base_name(&r.ty()?, text),
        ast::Type::ParenType(p) => base_name(&p.ty()?, text),
        _ => None,
    }
}

/// `text`'s items, once per thread.
fn parse(text: &'static str) -> Rc<Parsed> {
    let key = text.as_ptr() as usize;
    if let Some(p) = PARSED.with(|c| c.borrow().get(&key).cloned()) {
        return p;
    }
    let file = crate::macros::parse_tree(text);
    let mut parts = Vec::new();
    let mut items = Vec::new();
    let slice = |r: ra_ap_syntax::TextRange| &text[usize::from(r.start())..usize::from(r.end())];
    let name_of =
        |n: Option<ast::Name>| -> Option<&'static str> { Some(slice(n?.syntax().text_range())) };
    for child in file.syntax().children_with_tokens() {
        let node = match child {
            NodeOrToken::Token(t) => {
                parts.push(Part::Text(slice(t.text_range())));
                continue;
            }
            NodeOrToken::Node(n) => n,
        };
        let base = usize::from(node.text_range().start());
        let item_text = slice(node.text_range());
        let kind = match ast::Item::cast(node.clone()) {
            Some(ast::Item::Impl(imp)) => {
                let params: Vec<&str> = imp
                    .generic_param_list()
                    .map(|l| {
                        l.generic_params()
                            .filter_map(|p| match p {
                                ast::GenericParam::TypeParam(t) => name_of(t.name()),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut header_names = Vec::new();
                if let Some(t) = imp.self_ty() {
                    header_names.extend(node_words(t.syntax(), item_text, base, &[]));
                }
                if let Some(t) = imp.trait_() {
                    header_names.extend(node_words(t.syntax(), item_text, base, &[]));
                }
                header_names.retain(|n| !params.contains(n));
                header_names.sort_unstable();
                header_names.dedup();
                let trait_name = imp.trait_().and_then(|t| base_name(&t, text));
                let mut methods = Vec::new();
                let mut only_methods = true;
                if let Some(list) = imp.assoc_item_list() {
                    for m in list.assoc_items() {
                        let ast::AssocItem::Fn(f) = m else {
                            only_methods = false;
                            continue;
                        };
                        let Some(name) = name_of(f.name()) else {
                            continue;
                        };
                        let mr = f.syntax().text_range();
                        methods.push(Method {
                            start: usize::from(mr.start()) - base,
                            end: usize::from(mr.end()) - base,
                            name,
                            words: node_words(f.syntax(), item_text, base, &[]),
                        });
                    }
                }
                let skip: Vec<(usize, usize)> = methods.iter().map(|m| (m.start, m.end)).collect();
                Kind::Impl {
                    header_names,
                    trait_name,
                    words: node_words(&node, item_text, base, &skip),
                    methods,
                    only_methods,
                }
            }
            Some(item) => {
                let name = match &item {
                    ast::Item::Fn(f) => name_of(f.name()),
                    ast::Item::Struct(s) => name_of(s.name()),
                    ast::Item::Enum(e) => name_of(e.name()),
                    ast::Item::Union(u) => name_of(u.name()),
                    ast::Item::Trait(t) => name_of(t.name()),
                    ast::Item::TypeAlias(t) => name_of(t.name()),
                    ast::Item::Const(c) => name_of(c.name()),
                    ast::Item::Static(s) => name_of(s.name()),
                    _ => None,
                };
                let trait_methods = match &item {
                    ast::Item::Trait(t) => t
                        .assoc_item_list()
                        .map(|l| {
                            l.assoc_items()
                                .filter_map(|m| match m {
                                    ast::AssocItem::Fn(f) => name_of(f.name()),
                                    _ => None,
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                match name {
                    Some(name) => Kind::Named {
                        name,
                        words: node_words(&node, item_text, base, &[]),
                        trait_methods,
                    },
                    None => {
                        parts.push(Part::Text(item_text));
                        continue;
                    }
                }
            }
            None => {
                parts.push(Part::Text(item_text));
                continue;
            }
        };
        parts.push(Part::Item(items.len()));
        items.push(Item {
            text: item_text,
            kind,
        });
    }
    let parsed = Rc::new(Parsed { parts, items });
    PARSED.with(|c| c.borrow_mut().insert(key, parsed.clone()));
    parsed
}

/// The text of every `mod name;` file `src` declares, recursively (a module's exports can
/// name a prelude type the root never spells out).
pub(crate) fn module_texts(
    src: &str,
    filename: &str,
    resolver: crate::typeck::Resolver,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut todo = vec![(src.to_string(), filename.to_string())];
    while let Some((text, file)) = todo.pop() {
        if !text.contains("mod ") {
            continue;
        }
        for line in text.lines() {
            let l = line.trim_start();
            // (`pub mod x;`, `pub(crate) mod x;`)
            let l = match l.strip_prefix("pub") {
                Some(r) => r
                    .trim_start_matches(|c: char| c == '(' || c == ')' || c.is_alphanumeric())
                    .trim_start(),
                None => l,
            };
            let Some(rest) = l.strip_prefix("mod ") else {
                continue;
            };
            let Some(name) = rest.trim_end().strip_suffix(';') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let explicit = crate::typeck::path_attr_in(&text, name);
            let Some((path, mtext, _)) =
                crate::typeck::find_module(&file, name, explicit.as_deref(), resolver)
            else {
                continue;
            };
            if seen.insert(path.clone()) {
                todo.push((mtext.clone(), path));
                out.push(mtext);
            }
        }
    }
    out
}

/// Whether an impl of `trait_name` can be used by what's reached. Most traits are reached
/// without a name (`+`, `for`, `{}`, `?`, `.into()`), so any impl of theirs counts; a
/// `FromStr` impl is used only by `.parse()`, `T::from_str` or the trait's name, and it
/// carries the whole parser.
fn trait_in_use(trait_name: Option<&str>, reached: &HashSet<&str>) -> bool {
    match trait_name {
        Some("FromStr") => ["parse", "from_str", "FromStr"]
            .iter()
            .any(|w| reached.contains(w)),
        _ => true,
    }
}

/// `preludes`, each trimmed to what `program` (and `modules`, the texts of its `mod` files)
/// reaches, joined; each is preceded by a newline. `whole_impls` keeps every method of a
/// reached type (for the analysis behind completions).
pub(crate) fn trimmed(
    program: &str,
    modules: &[String],
    preludes: &[&'static str],
    whole_impls: bool,
) -> String {
    let parsed: Vec<Rc<Parsed>> = preludes.iter().map(|p| parse(p)).collect();
    let mut seed = Vec::new();
    words_of(program, &mut seed);
    for m in modules {
        words_of(m, &mut seed);
    }
    let mut reached: HashSet<&str> = seed.into_iter().collect();
    reached.extend(IMPLICIT.iter().copied());
    let declared: HashSet<&'static str> = parsed
        .iter()
        .flat_map(|p| p.items.iter())
        .filter_map(|i| match &i.kind {
            Kind::Named { name, .. } => Some(*name),
            _ => None,
        })
        .collect();
    let trait_methods: HashMap<&'static str, &[&'static str]> = parsed
        .iter()
        .flat_map(|p| p.items.iter())
        .filter_map(|i| match &i.kind {
            Kind::Named {
                name,
                trait_methods,
                ..
            } if !trait_methods.is_empty() => Some((*name, trait_methods.as_slice())),
            _ => None,
        })
        .collect();
    // kept[p][i]: the item is reached; kept_methods[p][i][m]: so is its m-th method.
    let mut kept: Vec<Vec<bool>> = parsed.iter().map(|p| vec![false; p.items.len()]).collect();
    let mut kept_methods: Vec<Vec<Vec<bool>>> = parsed
        .iter()
        .map(|p| {
            p.items
                .iter()
                .map(|i| match &i.kind {
                    Kind::Impl { methods, .. } => vec![false; methods.len()],
                    _ => Vec::new(),
                })
                .collect()
        })
        .collect();
    loop {
        let mut changed = false;
        for (pi, p) in parsed.iter().enumerate() {
            for (ii, item) in p.items.iter().enumerate() {
                match &item.kind {
                    Kind::Named { name, words, .. } => {
                        if !kept[pi][ii] && reached.contains(name) {
                            kept[pi][ii] = true;
                            changed = true;
                            reached.extend(words.iter().copied());
                        }
                    }
                    Kind::Impl {
                        header_names,
                        trait_name,
                        words,
                        methods,
                        ..
                    } => {
                        if !kept[pi][ii] {
                            // The types in the header are required. A prelude trait's
                            // methods are called without its name, so the trait is in use
                            // when it's named or one of its methods (or this impl's) is.
                            let types_reached = header_names
                                .iter()
                                .filter(|n| declared.contains(*n) && Some(**n) != *trait_name)
                                .all(|n| reached.contains(n));
                            let trait_used = match trait_name {
                                Some(t) if declared.contains(t) => {
                                    reached.contains(t)
                                        || trait_methods.get(t).is_some_and(|ms| {
                                            ms.iter().any(|m| reached.contains(m))
                                        })
                                        || methods.iter().any(|m| reached.contains(m.name))
                                }
                                t => trait_in_use(*t, &reached),
                            };
                            let ok = types_reached && trait_used;
                            if !ok {
                                continue;
                            }
                            kept[pi][ii] = true;
                            changed = true;
                            reached.extend(words.iter().copied());
                        }
                        for (mi, m) in methods.iter().enumerate() {
                            if !kept_methods[pi][ii][mi]
                                && (trait_name.is_some() || whole_impls || reached.contains(m.name))
                            {
                                kept_methods[pi][ii][mi] = true;
                                changed = true;
                                reached.extend(m.words.iter().copied());
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut out = String::new();
    for (pi, p) in parsed.iter().enumerate() {
        out.push('\n');
        for part in &p.parts {
            let ii = match part {
                Part::Text(t) => {
                    out.push_str(t);
                    continue;
                }
                Part::Item(ii) => *ii,
            };
            if !kept[pi][ii] {
                continue;
            }
            let item = &p.items[ii];
            let Kind::Impl {
                methods,
                only_methods,
                ..
            } = &item.kind
            else {
                out.push_str(item.text);
                continue;
            };
            let dropped: Vec<&Method> = methods
                .iter()
                .enumerate()
                .filter(|(mi, _)| !kept_methods[pi][ii][*mi])
                .map(|(_, m)| m)
                .collect();
            if *only_methods && !methods.is_empty() && dropped.len() == methods.len() {
                continue;
            }
            let mut at = 0;
            for m in dropped {
                out.push_str(&item.text[at..m.start]);
                at = m.end;
            }
            out.push_str(&item.text[at..]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "// (test prelude)\n\
        struct A { x: i32 }\n\
        struct B { a: A }\n\
        struct C;\n\
        impl A {\n    fn new() -> A { A { x: 1 } }\n    fn to_b(&self) -> B { B { a: A::new() } }\n}\n\
        impl std::fmt::Display for A {\n    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, \"{}\", self.x) }\n}\n\
        impl C {\n    fn c() {}\n}\n\
        trait Tr { fn tr(&self) -> i32; }\n\
        impl Tr for i32 { fn tr(&self) -> i32 { *self } }\n\
        fn helper() -> C { C }\n";

    #[test]
    fn keeps_what_is_named_and_what_that_reaches() {
        let out = trimmed(
            "fn main() { let a = A::new(); println!(\"{a}\"); }",
            &[],
            &[P],
            false,
        );
        assert!(out.contains("struct A"));
        assert!(out.contains("fn new()"));
        assert!(!out.contains("fn to_b"));
        assert!(!out.contains("struct B"));
        assert!(out.contains("Display for A"));
        assert!(!out.contains("struct C"));
        assert!(!out.contains("impl C"));
        assert!(!out.contains("trait Tr"));
        assert!(!out.contains("fn helper"));
        assert!(out.starts_with("\n// (test prelude)"));
    }

    #[test]
    fn methods_reach_their_types_and_traits_reach_std_impls() {
        let out = trimmed(
            "fn f() { A::new().to_b(); 3.tr(); use Tr; }",
            &[],
            &[P],
            false,
        );
        assert!(out.contains("struct B"));
        assert!(out.contains("fn to_b"));
        assert!(out.contains("trait Tr"));
        assert!(out.contains("impl Tr for i32"));
    }

    #[test]
    fn whole_impls_keep_every_method() {
        let out = trimmed("fn f() { let a = A::new(); }", &[], &[P], true);
        assert!(out.contains("fn to_b"));
        assert!(out.contains("struct B"));
    }
}
