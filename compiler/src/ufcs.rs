//! Path-call syntax for methods of built-in types, as a pre-pass over the source tokens (so it
//! reaches macro arguments too): `u64::pow(2, 40)` is written as `(2u64).pow(40)`, `str::len(&s)`
//! as `(s).len()`, and a method named as a value (`.fold(0, i32::max)`, `.map(str::trim)`)
//! becomes a closure, `(|a, b| a.max(b))`. The same for some methods of `Vec`, `Option` and std
//! traits (`.map(Vec::len)`, `.map(Into::into)`, `std::error::Error::source(&e)`). An `OffsetMap`
//! keeps positions pointing at the original text.

use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxToken,
    ast::{self, HasName},
};

use crate::mbe::OffsetMap;

const PRIMS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "char", "bool", "str", "String",
];

/// Other types and std traits whose methods are named by path (`Vec::len`,
/// `std::error::Error::source`); their receivers' types aren't written into the closures.
const OTHERS: &[&str] = &[
    "Vec", "Option", "Result", "Into", "From", "Clone", "ToString", "Error",
];

/// The `std::ops` / `std::cmp` traits whose methods are an operator: `Add::add(a, b)`,
/// `<V as Add>::add(a, b)` and `i32::add(1, 2)` are `((a) + (b))`, and `.fold(0, Add::add)` takes
/// `(|a, b| a + b)`. With the operator, and whether it's unary.
const OP_TRAITS: &[(&str, &str, &str, bool)] = &[
    ("Add", "add", "+", false),
    ("Sub", "sub", "-", false),
    ("Mul", "mul", "*", false),
    ("Div", "div", "/", false),
    ("Rem", "rem", "%", false),
    ("BitAnd", "bitand", "&", false),
    ("BitOr", "bitor", "|", false),
    ("BitXor", "bitxor", "^", false),
    ("Shl", "shl", "<<", false),
    ("Shr", "shr", ">>", false),
    ("Neg", "neg", "-", true),
    ("Not", "not", "!", true),
    ("PartialEq", "eq", "==", false),
    ("PartialEq", "ne", "!=", false),
    ("PartialOrd", "lt", "<", false),
    ("PartialOrd", "le", "<=", false),
    ("PartialOrd", "gt", ">", false),
    ("PartialOrd", "ge", ">=", false),
];

/// The methods (taking `self`) these types have, with how many more arguments they take.
fn arity(prim: &str, method: &str) -> Option<usize> {
    match (prim, method) {
        (
            "Vec",
            "len" | "is_empty" | "iter" | "first" | "last" | "to_vec" | "into_iter" | "clone",
        ) => return Some(0),
        ("Vec", "contains") => return Some(1),
        ("Option", "is_some" | "is_none" | "unwrap" | "unwrap_or_default" | "flatten") => {
            return Some(0);
        }
        ("Option", "expect" | "unwrap_or" | "ok_or") => return Some(1),
        (
            "Result",
            "is_ok" | "is_err" | "ok" | "err" | "unwrap" | "unwrap_err" | "unwrap_or_default",
        ) => return Some(0),
        ("Result", "expect" | "expect_err" | "unwrap_or") => return Some(1),
        // (`From::from(x)` is `x.into()`, see `emitted`.)
        ("From", "from") => return Some(0),
        ("Into", "into") | ("Clone", "clone") | ("ToString", "to_string") => return Some(0),
        ("Error", "source" | "description") => return Some(0),
        ("Vec" | "Option" | "Result" | "Into" | "From" | "Clone" | "ToString" | "Error", _) => {
            return None;
        }
        // `u64::from_str(s)` is `s.parse::<u64>()`, see `emitted`.
        (p, "from_str") if p != "str" => return Some(0),
        _ => {}
    }
    let numeric = !matches!(prim, "char" | "bool" | "str" | "String");
    let float = matches!(prim, "f32" | "f64");
    if numeric {
        if method.starts_with("checked_")
            || method.starts_with("wrapping_")
            || method.starts_with("saturating_")
        {
            let unary = method.ends_with("_neg")
                || method.ends_with("_abs")
                || method == "checked_next_power_of_two";
            return Some(if unary { 0 } else { 1 });
        }
        return Some(match method {
            "abs" | "signum" | "to_string" | "count_ones" | "count_zeros" | "leading_zeros"
            | "trailing_zeros" | "is_power_of_two" | "ilog2" | "ilog10" | "isqrt"
            | "is_positive" | "is_negative" | "next_power_of_two" => 0,
            "pow" | "min" | "max" | "rem_euclid" | "div_euclid" | "abs_diff" | "cmp"
            | "partial_cmp" => 1,
            "clamp" => 2,
            _ if float => match method {
                "sqrt" | "cbrt" | "floor" | "ceil" | "round" | "trunc" | "fract" | "exp"
                | "exp2" | "exp_m1" | "ln" | "ln_1p" | "log10" | "log2" | "sin" | "cos" | "tan"
                | "asin" | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh"
                | "atanh" | "recip" | "to_radians" | "to_degrees" | "is_nan" | "is_finite"
                | "is_infinite" | "is_sign_negative" | "is_sign_positive" | "to_bits" => 0,
                "powi" | "powf" | "atan2" | "hypot" | "copysign" | "log" | "total_cmp" => 1,
                "mul_add" => 2,
                _ => return None,
            },
            _ => return None,
        });
    }
    Some(match (prim, method) {
        ("char", m) => match m {
            "to_digit" => 1,
            m if m.starts_with("is_") || m.starts_with("to_") || m == "len_utf8" => 0,
            _ => return None,
        },
        ("bool", "then_some") => 1,
        ("bool", "then") => 1,
        ("str" | "String", m) => match m {
            "len" | "is_empty" | "trim" | "trim_start" | "trim_end" | "to_uppercase"
            | "to_lowercase" | "to_ascii_uppercase" | "to_ascii_lowercase" | "chars" | "bytes"
            | "lines" | "split_whitespace" | "to_string" | "to_owned" | "as_str"
            | "char_indices" => 0,
            "contains"
            | "starts_with"
            | "ends_with"
            | "split"
            | "find"
            | "repeat"
            | "push_str"
            | "push"
            | "strip_prefix"
            | "strip_suffix"
            | "eq_ignore_ascii_case" => 1,
            "replace" => 2,
            _ => return None,
        },
        _ => return None,
    })
}

/// The method call written for `prim::method`: `u64::from_str(s)` is `s.parse::<u64>()` and
/// `From::from(x)` is `x.into()`.
fn emitted(prim: &str, method: &str) -> String {
    match (prim, method) {
        ("From", "from") => "into".to_string(),
        (p, "from_str") => format!("parse::<{p}>"),
        _ => method.to_string(),
    }
}

/// An operator trait's method named by path at `toks[i]`: `Add::add`, `std::ops::Add::add`,
/// `<V as Add>::add`, `<i32 as std::ops::Add<i32>>::add` or `i32::add`. The index of the path's
/// first token and of the method's, the operator, whether it's unary, and the primitive type that
/// names it (for literal suffixes). Not a trait of the file's own (`traits`). Also `V::add` for a
/// type of the file's with that operator (`op_types`, (type, trait) pairs).
fn op_path(
    toks: &[SyntaxToken],
    i: usize,
    traits: &[String],
    op_types: &[(String, String)],
) -> Option<(usize, usize, &'static str, bool, Option<String>)> {
    let at = |k: usize| {
        toks.get(k)
            .map(|t| t.text().to_string())
            .unwrap_or_default()
    };
    let ident = |k: usize| toks.get(k).is_some_and(|t| t.kind() == SyntaxKind::IDENT);
    // (`::` is one token in a path, two `:` in a macro's tokens.)
    let sep = |k: usize| -> Option<usize> {
        if at(k) == "::" {
            Some(k + 1)
        } else if at(k) == ":" && at(k + 1) == ":" {
            Some(k + 2)
        } else {
            None
        }
    };
    let sep_before = |k: usize| -> Option<usize> {
        if k >= 1 && at(k - 1) == "::" {
            Some(k - 1)
        } else if k >= 2 && at(k - 1) == ":" && at(k - 2) == ":" {
            Some(k - 2)
        } else {
            None
        }
    };
    let op = |tr: &str, m: &str| {
        OP_TRAITS
            .iter()
            .find(|(t, n, _, _)| (tr.is_empty() || *t == tr) && *n == m)
            .filter(|(t, ..)| !traits.iter().any(|u| u == t))
            .map(|(_, _, o, u)| (*o, *u))
    };
    if at(i) == "<" {
        // `<X as [path::]Trait[<..>]>::method`
        let mut depth = 0i32;
        let mut k = i;
        let mut as_at = None;
        loop {
            let t = at(k);
            if t.is_empty() || matches!(t.as_str(), ";" | "{" | "}" | "(" | ")") {
                return None;
            }
            if t == "as" && depth == 1 {
                as_at = Some(k);
            }
            depth += t.chars().filter(|c| *c == '<').count() as i32;
            depth -= t.chars().filter(|c| *c == '>').count() as i32;
            if depth <= 0 {
                break;
            }
            k += 1;
        }
        let (close, as_at) = (k, as_at?);
        // The trait's name: the last segment before its generic arguments.
        let mut tr = as_at + 1;
        let mut j = as_at + 1;
        while j < close && at(j) != "<" {
            if ident(j) {
                tr = j;
            }
            j += 1;
        }
        let m = sep(close + 1)?;
        if !ident(m) {
            return None;
        }
        let (o, u) = op(&at(tr), &at(m))?;
        let prim = (as_at == i + 2 && PRIMS.contains(&at(i + 1).as_str())).then(|| at(i + 1));
        return Some((i, m, o, u, prim));
    }
    if !ident(i) {
        return None;
    }
    let name = at(i);
    let m = sep(i + 1)?;
    if !ident(m) {
        return None;
    }
    let prim = PRIMS.contains(&name.as_str()) && !matches!(name.as_str(), "str" | "String");
    let (o, u) = if prim {
        op("", &at(m))?
    } else if let Some((_, tr)) = op_types.iter().find(|(t, tr)| {
        *t == name && OP_TRAITS.iter().any(|(x, n, ..)| x == tr && *n == at(m))
    }) {
        op(tr, &at(m))?
    } else {
        op(&name, &at(m))?
    };
    // `std::ops::` before the trait.
    let mut first = i;
    if !prim {
        while let Some(s) = sep_before(first)
            && s >= 1
            && matches!(at(s - 1).as_str(), "std" | "core" | "ops" | "cmp")
        {
            first = s - 1;
        }
    }
    if sep_before(first).is_some()
        || (first >= 1 && matches!(at(first - 1).as_str(), "." | "<" | "as" | "impl" | "for"))
    {
        return None;
    }
    Some((first, m, o, u, prim.then_some(name)))
}

/// The (type, trait) pairs of the file's `impl Add for V`s (`std::ops::Add<V>`, `impl<T> Add for
/// V<T>`), where `V` has no inherent method of the trait's (which `V::add` would call).
fn op_impl_types(file: &ast::SourceFile, traits: &[String]) -> Vec<(String, String)> {
    let last = |p: Option<ast::Path>| {
        p.and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string())
    };
    let self_name = |i: &ast::Impl| match i.self_ty() {
        Some(ast::Type::PathType(p)) => last(p.path()),
        _ => None,
    };
    let impls: Vec<ast::Impl> = file.syntax().descendants().filter_map(ast::Impl::cast).collect();
    let inherent: Vec<(String, String)> = impls
        .iter()
        .filter(|i| i.trait_().is_none())
        .filter_map(|i| Some((self_name(i)?, i.assoc_item_list()?)))
        .flat_map(|(t, l)| {
            l.assoc_items().filter_map(move |a| match a {
                ast::AssocItem::Fn(f) => Some((t.clone(), f.name()?.text().to_string())),
                _ => None,
            })
        })
        .collect();
    impls
        .iter()
        .filter_map(|i| {
            let tr = match i.trait_()? {
                ast::Type::PathType(p) => last(p.path())?,
                _ => return None,
            };
            let ty = self_name(i)?;
            let (_, m, ..) = OP_TRAITS.iter().find(|(t, ..)| *t == tr)?;
            let shadowed = inherent.iter().any(|(t, n)| *t == ty && n == m);
            (!shadowed && !traits.contains(&tr)).then_some((ty, tr))
        })
        .collect()
}

pub fn has_prim_paths(text: &str) -> bool {
    PRIMS
        .iter()
        .chain(OTHERS)
        .any(|p| text.contains(&format!("{p}::")))
        // (`Result::Ok(1)`, `HashMap::from_iter(..)`, see `expand`.)
        || text.contains("Result::")
        || text.contains("from_iter")
        || text.contains("::from)")
        || text.contains("::from,")
        || OP_TRAITS.iter().any(|(t, ..)| {
            text.contains(&format!("{t}::"))
                || text.contains(&format!("as {t}"))
                || text.contains(&format!("ops::{t}"))
        })
}

/// Rewrite path calls and method values of built-in types in `text`, recording the edits in
/// `map`. Whether anything changed.
pub fn expand(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let toks: Vec<SyntaxToken> = file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let at = |i: usize| {
        toks.get(i)
            .map(|t| t.text().to_string())
            .unwrap_or_default()
    };
    let start = |i: usize| usize::from(toks[i].text_range().start());
    let end = |i: usize| usize::from(toks[i].text_range().end());
    // How deep `toks[j]` leaves a scan in turbofish brackets (`Ok::<i32, ()>(1)` is one argument).
    let turbofish = |j: usize, angle: &mut usize| {
        let colons =
            j >= 1 && (at(j - 1) == "::" || (j >= 2 && at(j - 1) == ":" && at(j - 2) == ":"));
        match at(j).as_str() {
            "<" if *angle > 0 || colons => *angle += 1,
            ">" => *angle = angle.saturating_sub(1),
            ">>" => *angle = angle.saturating_sub(2),
            _ => {}
        }
    };
    // Don't touch `macro_rules!` bodies or `use` items.
    let skip = |t: &SyntaxToken| {
        t.parent_ancestors().any(|a| {
            matches!(
                a.kind(),
                SyntaxKind::MACRO_RULES | SyntaxKind::USE | SyntaxKind::ATTR
            )
        })
    };
    // The file's own traits (a `trait Add` isn't `std::ops::Add`).
    let traits: Vec<String> = (1..toks.len())
        .filter(|&k| toks[k - 1].kind() == SyntaxKind::TRAIT_KW)
        .map(at)
        .collect();
    let op_types = op_impl_types(&file, &traits);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i + 3 < toks.len() {
        // `Option::Some(3)`, `std::option::Option::None`, `Result::<i32, String>::Ok(1)` (also as
        // patterns): the variant alone, as the prelude names it (a turbofish moves onto it).
        // type settles it).
        if let Some((first, variant, generics)) = variant_path(&toks, i)
            && !skip(&toks[i])
        {
            match generics {
                // `Result::<A, B>::Ok(1)` is `Ok::<A, B>(1)`.
                Some((lt, gt)) => {
                    edits.push((start(first), start(lt), format!("{}::", at(variant))));
                    edits.push((end(gt), end(variant), String::new()));
                }
                None => edits.push((start(first), start(variant), String::new())),
            }
            i = variant + 1;
            continue;
        }
        // `Add::add(a, b)` is `((a) + (b))`, `Add::add` as a value `(|a, b| a + b)`.
        if let Some((first, m, op, unary, prim)) = op_path(&toks, i, &traits, &op_types)
            && !skip(&toks[i])
        {
            let n = if unary { 1 } else { 2 };
            if at(m + 1) == "(" {
                // The arguments' separating commas and the closing `)`.
                let open = m + 1;
                let mut commas = Vec::new();
                let mut depth = 0;
                let mut angle = 0;
                let mut j = open + 1;
                while j < toks.len() {
                    turbofish(j, &mut angle);
                    match at(j).as_str() {
                        "(" | "[" | "{" => depth += 1,
                        ")" if depth == 0 => break,
                        ")" | "]" | "}" => depth -= 1,
                        "," if depth == 0 && angle == 0 => commas.push(j),
                        _ => {}
                    }
                    j += 1;
                }
                // (A trailing comma.)
                if commas.last().is_some_and(|&c| c + 1 == j) {
                    commas.pop();
                }
                if j >= toks.len() || commas.len() + 1 != n || j == open + 1 {
                    i += 1;
                    continue;
                }
                // `u8::add(200, 100)`: the literal gets the type's suffix.
                let suffix = prim
                    .filter(|p| {
                        let lit = if at(open + 1) == "-" {
                            open + 2
                        } else {
                            open + 1
                        };
                        let kind = toks[lit].kind();
                        let float = matches!(p.as_str(), "f32" | "f64");
                        lit + 1 == commas.first().copied().unwrap_or(j)
                            && !PRIMS.iter().any(|q| at(lit).ends_with(q))
                            && !matches!(p.as_str(), "char" | "bool")
                            && ((kind == SyntaxKind::INT_NUMBER && !float)
                                || (kind == SyntaxKind::FLOAT_NUMBER && float))
                    })
                    .unwrap_or_default();
                let last = if at(j - 1) == "," { j - 2 } else { j - 1 };
                if unary {
                    edits.push((start(first), end(open), format!("({op}(")));
                    edits.push((end(last), end(j), "))".to_string()));
                } else {
                    let c = commas[0];
                    edits.push((start(first), end(open), "((".to_string()));
                    edits.push((end(c - 1), start(c + 1), format!("{suffix}) {op} (")));
                    edits.push((end(last), end(j), "))".to_string()));
                }
                i = j + 1;
                continue;
            }
            if !matches!(at(m + 1).as_str(), "::" | ":" | "!") {
                let ty = prim.map(|p| format!(": {p}")).unwrap_or_default();
                let closure = if n == 1 {
                    format!("(|__ufcs0{ty}| {op}__ufcs0)")
                } else {
                    format!("(|__ufcs0{ty}, __ufcs1| __ufcs0 {op} __ufcs1)")
                };
                edits.push((start(first), end(m), closure));
                i = m + 1;
                continue;
            }
        }
        // `Vec::from_iter(it)`, `HashMap::<K, V>::from_iter(it)`, `FromIterator::from_iter(it)`:
        // `(it).into_iter().collect::<Vec<_>>()`.
        // As a value (`.map(Vec::from_iter)`, `let f = Vec::from_iter;`): a closure doing that.
        if let Some((first, name, ty)) = from_iter_path(&toks, i)
            && !skip(&toks[i])
            && matches!(at(name + 1).as_str(), "," | ")" | ";")
        {
            // (`let g = Vec::<i32>::from_iter;`: a generic fn, since a closure's parameter would
            // have no type there.)
            let closure = match ty.as_deref().and_then(from_iter_item) {
                Some(item) if at(name + 1) == ";" => {
                    let t = ty.unwrap_or_default();
                    format!(
                        "{{ fn __from_iter{i}<I: IntoIterator<Item = {item}>>(it: I) -> {t} {{ it.into_iter().collect() }} __from_iter{i} }}"
                    )
                }
                // (`let g = Vec::from_iter;`: generic over the items too.)
                _ if at(name + 1) == ";"
                    && let Some(f) = ty.as_deref().and_then(|t| from_iter_generic_fn(t, i)) =>
                {
                    f
                }
                _ => {
                    let fish = ty.map(|t| format!("::<{t}>")).unwrap_or_default();
                    format!("(|__ufcs0| (__ufcs0).into_iter().collect{fish}())")
                }
            };
            edits.push((start(first), end(name), closure));
            i = name + 1;
            continue;
        }
        if let Some((first, name, ty)) = from_iter_path(&toks, i)
            && !skip(&toks[i])
            && at(name + 1) == "("
        {
            let open = name + 1;
            let mut j = open + 1;
            let mut depth = 0;
            while j < toks.len() {
                match at(j).as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" if depth == 0 => break,
                    ")" | "]" | "}" => depth -= 1,
                    _ => {}
                }
                j += 1;
            }
            if j < toks.len() && j > open + 1 {
                // (`Vec::from_iter(it,)`: not a tuple.)
                let close = if at(j - 1) == "," { start(j - 1) } else { start(j) };
                let fish = ty.map(|t| format!("::<{t}>")).unwrap_or_default();
                edits.push((start(first), end(open), "(".to_string()));
                edits.push((close, end(j), format!(").into_iter().collect{fish}()")));
                i = open + 1;
                continue;
            }
        }
        // `.map(PathBuf::from)`, `.map(Id::from)`, `.map(f64::from)`: a closure calling it.
        if let Some((first, m)) = from_value_path(&toks, i)
            && !skip(&toks[i])
        {
            edits.push((start(first), start(first), "(|__ufcs0| ".to_string()));
            edits.push((end(m), end(m), "(__ufcs0))".to_string()));
            i = m + 1;
            continue;
        }
        let prim = at(i);
        // `u64` `:` `:` `pow` (a `::` is one token in paths but two in token trees)
        let (sep_len, ok) = if at(i + 1) == "::" {
            (1, true)
        } else if at(i + 1) == ":" && at(i + 2) == ":" && end(i + 1) == start(i + 2) {
            (2, true)
        } else {
            (0, false)
        };
        let m = i + 1 + sep_len;
        let prev = if i > 0 { at(i - 1) } else { String::new() };
        let other = OTHERS.contains(&prim.as_str());
        // `std::error::Error::source`, `std::vec::Vec::len`: from the path's start.
        let mut first_tok = i;
        if other {
            let seg = |k: usize| {
                toks[k].kind() == SyntaxKind::IDENT
                    && matches!(
                        at(k).as_str(),
                        "std"
                            | "core"
                            | "alloc"
                            | "error"
                            | "vec"
                            | "option"
                            | "convert"
                            | "clone"
                            | "string"
                            | "result"
                    )
            };
            loop {
                // (`::` is one token in a path, two `:` in a macro's tokens.)
                if first_tok >= 2 && at(first_tok - 1) == "::" && seg(first_tok - 2) {
                    first_tok -= 2;
                } else if first_tok >= 3
                    && at(first_tok - 1) == ":"
                    && at(first_tok - 2) == ":"
                    && seg(first_tok - 3)
                {
                    first_tok -= 3;
                } else {
                    break;
                }
            }
        }
        let prev = if first_tok != i {
            if first_tok > 0 {
                at(first_tok - 1)
            } else {
                String::new()
            }
        } else {
            prev
        };
        if !ok
            || !(PRIMS.contains(&prim.as_str()) || other)
            || matches!(prev.as_str(), "::" | ":" | "." | "<" | "as")
            || toks[m].kind() != SyntaxKind::IDENT
            || skip(&toks[i])
        {
            i += 1;
            continue;
        }
        let method = at(m);
        let Some(n_args) = arity(&prim, &method) else {
            i += 1;
            continue;
        };
        if at(m + 1) == "(" {
            // A call: find the first argument's extent.
            let open = m + 1;
            let mut j = open + 1;
            let mut depth = 0;
            let mut angle = 0;
            while j < toks.len() {
                turbofish(j, &mut angle);
                let t = at(j);
                match t.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" if depth > 0 => depth -= 1,
                    ")" if depth == 0 => break,
                    "," if depth == 0 && angle == 0 => break,
                    _ => {}
                }
                j += 1;
            }
            if j >= toks.len() || j == open + 1 {
                i += 1;
                continue;
            }
            // `str::len(&s)` → `(s).len()`
            let mut first = open + 1;
            let mut drop_to = None;
            if at(first) == "&" || at(first) == "&&" {
                first += 1;
                if at(first) == "mut" {
                    first += 1;
                }
                drop_to = Some(start(first));
            }
            // `u64::pow(2, 40)`: the literal gets the type's suffix.
            let suffix = {
                let lit = if at(first) == "-" { first + 1 } else { first };
                let numeric = !matches!(prim.as_str(), "char" | "bool" | "str" | "String");
                let float = matches!(prim.as_str(), "f32" | "f64");
                let kind = toks.get(lit).map(|t| t.kind());
                let plain = |t: &str| !PRIMS.iter().any(|p| t.ends_with(p));
                if numeric
                    && lit + 1 == j
                    && plain(&at(lit))
                    && ((kind == Some(SyntaxKind::INT_NUMBER) && !float)
                        || (kind == Some(SyntaxKind::FLOAT_NUMBER) && float))
                {
                    prim.clone()
                } else {
                    String::new()
                }
            };
            // (Unfinished code: a `,` at the very end.)
            if at(j) == "," && j + 1 >= toks.len() {
                i += 1;
                continue;
            }
            let last = j - 1;
            edits.push((start(first_tok), end(open), "(".to_string()));
            if let Some(to) = drop_to {
                edits.push((end(open), to, String::new()));
            }
            if at(j) == "," {
                // (up to the next argument, or the `)` after a trailing comma)
                edits.push((
                    end(last),
                    start(j + 1),
                    format!("{suffix}).{}(", emitted(&prim, &method)),
                ));
            } else {
                edits.push((
                    end(last),
                    end(last),
                    format!("{suffix}).{}(", emitted(&prim, &method)),
                ));
            }
            i = j;
            continue;
        }
        // A method named as a value: a closure over its arguments.
        if !matches!(at(m + 1).as_str(), "::" | ":" | "!") {
            let params: Vec<String> = (0..=n_args).map(|k| format!("__ufcs{k}")).collect();
            // The receiver's type, as rustc knows it from the path (`i32::abs` takes an `i32`).
            let typed =
                if matches!(prim.as_str(), "str" | "String") || other || method == "from_str" {
                    params.join(", ")
                } else {
                    let mut all = params.clone();
                    all[0] = format!("{}: {prim}", params[0]);
                    all.join(", ")
                };
            let closure = format!(
                "(|{typed}| {}.{}({}))",
                params[0],
                emitted(&prim, &method),
                params[1..].join(", ")
            );
            edits.push((start(first_tok), end(m), closure));
            i = m + 1;
            continue;
        }
        i += 1;
    }
    let changed = !edits.is_empty();
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    let mut last_start = usize::MAX;
    for (a, b, new) in edits {
        if b > last_start {
            continue;
        }
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
        last_start = a;
    }
    changed
}

/// Whether `text` may have a qualified type path without a trait (`<u8>::MAX`).
pub fn has_type_anchors(text: &str) -> bool {
    text.contains(">::") || text.contains("> ::")
}

/// Qualified type paths without a trait, `<u8>::MAX`, `<Vec<u8>>::new()`, `<T>::f()` (also in
/// macro arguments and `macro_rules!` expansions, `<$t>::MAX`), written as plain paths: `u8::MAX`,
/// `Vec::<u8>::new()`. Types that aren't paths (`<[u8]>::len`, `<&str>::..`) stay. Whether
/// anything changed.
pub fn expand_type_anchors(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let toks: Vec<SyntaxToken> = file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let at = |i: usize| {
        toks.get(i)
            .map(|t| t.text())
            .unwrap_or_default()
    };
    let start = |i: usize| usize::from(toks[i].text_range().start());
    let end = |i: usize| usize::from(toks[i].text_range().end());
    let in_rules = |t: &SyntaxToken| {
        t.parent_ancestors()
            .any(|a| matches!(a.kind(), SyntaxKind::MACRO_RULES | SyntaxKind::ATTR))
    };
    // `::` is one token in a path, two `:` in a macro's tokens.
    let colons =
        |i: usize| at(i) == "::" || (at(i) == ":" && at(i + 1) == ":" && end(i) == start(i + 1));
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if at(i) != "<" || in_rules(&toks[i]) {
            i += 1;
            continue;
        }
        // Not generic arguments (`Vec<..>`, `f::<..>`) nor a comparison (`a < b`).
        if i > 0 {
            let p = &toks[i - 1];
            let prev = at(i - 1);
            let value_end = matches!(
                p.kind(),
                SyntaxKind::IDENT
                    | SyntaxKind::INT_NUMBER
                    | SyntaxKind::FLOAT_NUMBER
                    | SyntaxKind::STRING
                    | SyntaxKind::CHAR
                    | SyntaxKind::SELF_KW
                    | SyntaxKind::SELF_TYPE_KW
                    | SyntaxKind::SUPER_KW
                    | SyntaxKind::CRATE_KW
                    | SyntaxKind::LIFETIME_IDENT
                    | SyntaxKind::TRUE_KW
                    | SyntaxKind::FALSE_KW
            ) || matches!(
                prev,
                "::" | ":"
                    | ")"
                    | "]"
                    | ">"
                    | "?"
                    | "$"
                    | "impl"
                    | "fn"
                    | "for"
                    | "struct"
                    | "enum"
                    | "union"
                    | "type"
                    | "trait"
            );
            if value_end {
                i += 1;
                continue;
            }
        }
        // The matching `>`, with no `as` (`<T as Tr>::f` is another pass's) at the top level.
        let mut depth = 0usize;
        let mut j = i + 1;
        let mut close = None;
        let mut nested_opens = Vec::new();
        while j < toks.len() {
            let t = at(j);
            match t {
                "<" => {
                    if depth == 0 && !colons(j - 1) && !(at(j - 1) == ":" && colons(j - 2)) {
                        nested_opens.push(j);
                    }
                    depth += 1;
                }
                ">" if depth == 0 => {
                    close = Some(j);
                    break;
                }
                ">" => depth -= 1,
                ">>" | "->" | "=>" | ";" | "{" | "}" | "=" | "as" | "<<" | "<=" | ">=" => break,
                _ => {}
            }
            j += 1;
        }
        let Some(c) = close else {
            i += 1;
            continue;
        };
        let first = at(i + 1);
        // `<(Comma)>::default()` (what `<Token![,]>` expands to): one parenthesized path, not a
        // tuple, is that path.
        if first == "("
            && c > i + 3
            && at(c - 1) == ")"
            && toks[i + 2].kind() == SyntaxKind::IDENT
            && colons(c + 1)
            && !(i + 2..c - 1).any(|k| matches!(at(k), "," | "(" | ")"))
        {
            edits.push((start(i), end(i + 1), String::new()));
            for o in nested_opens {
                edits.push((start(o), start(o), "::".to_string()));
            }
            edits.push((start(c - 1), end(c), String::new()));
            i = c + 1;
            continue;
        }
        // (Not a macro's type: `<Token![,]>::default()` stays as it is.)
        let path_like = c > i + 1
            && !(i + 1..c).any(|k| at(k) == "!")
            && (toks[i + 1].kind() == SyntaxKind::IDENT
                || matches!(first, "Self" | "crate" | "self" | "super" | "$")
                || first == "::");
        if !path_like || !colons(c + 1) {
            i += 1;
            continue;
        }
        edits.push((start(i), end(i), String::new()));
        for o in nested_opens {
            edits.push((start(o), start(o), "::".to_string()));
        }
        edits.push((start(c), end(c), String::new()));
        i = c + 1;
    }
    let changed = !edits.is_empty();
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    for (a, b, new) in edits {
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
    }
    changed
}

/// `Option::Some` / `Result::Err` at `toks[i]`, with an optional `std::option::` / `core::result::`
/// prefix before it and a turbofish after the type (`Result::<A, B>::Ok`): the index of the
/// path's first token and of the variant's, and the turbofish's `<` and `>` tokens when there is one.
fn variant_path(toks: &[SyntaxToken], i: usize) -> Option<(usize, usize, Option<(usize, usize)>)> {
    let at = |k: usize| toks.get(k).map(|t| t.text().to_string()).unwrap_or_default();
    let ident = |k: usize| toks.get(k).is_some_and(|t| t.kind() == SyntaxKind::IDENT);
    // (`::` is one token in a path, two `:` in a macro's tokens.)
    let sep = |k: usize| -> Option<usize> {
        if at(k) == "::" {
            Some(k + 1)
        } else if at(k) == ":" && at(k + 1) == ":" {
            Some(k + 2)
        } else {
            None
        }
    };
    let sep_before = |k: usize| -> Option<usize> {
        if k >= 1 && at(k - 1) == "::" {
            Some(k - 1)
        } else if k >= 2 && at(k - 1) == ":" && at(k - 2) == ":" {
            Some(k - 2)
        } else {
            None
        }
    };
    let ty = at(i);
    let (module, variants): (&str, &[&str]) = match ty.as_str() {
        "Option" => ("option", &["Some", "None"]),
        "Result" => ("result", &["Ok", "Err"]),
        _ => return None,
    };
    if !ident(i) {
        return None;
    }
    let mut k = sep(i + 1)?;
    let mut generics = None;
    if at(k) == "<" {
        let lt = k;
        // (`>>` closes two levels in a token tree.)
        let mut depth = 0i32;
        loop {
            let t = at(k);
            if t.is_empty() {
                return None;
            }
            depth += t.chars().filter(|c| *c == '<').count() as i32;
            depth -= t.chars().filter(|c| *c == '>').count() as i32;
            k += 1;
            if depth <= 0 {
                break;
            }
        }
        generics = Some((lt, k - 1));
        k = sep(k)?;
    }
    if !ident(k) || !variants.contains(&at(k).as_str()) {
        return None;
    }
    // `std::option::` before the type.
    let mut first = i;
    if let Some(s) = sep_before(first)
        && s >= 1
        && at(s - 1) == module
        && ident(s - 1)
    {
        first = s - 1;
        if let Some(s) = sep_before(first)
            && s >= 1
            && matches!(at(s - 1).as_str(), "std" | "core" | "alloc")
        {
            first = s - 1;
        }
    }
    // (`m::Option::Some`, `x.Option`: another `Option`.)
    if sep_before(first).is_some()
        || (first >= 1 && matches!(at(first - 1).as_str(), "." | "<" | "as"))
    {
        return None;
    }
    Some((first, k, generics))
}

/// The std collections `X::from_iter(..)` builds, with how many type parameters each has.
const FROM_ITER_TYPES: &[(&str, usize)] = &[
    ("Vec", 1),
    ("VecDeque", 1),
    ("LinkedList", 1),
    ("BinaryHeap", 1),
    ("HashSet", 1),
    ("BTreeSet", 1),
    ("HashMap", 2),
    ("BTreeMap", 2),
    ("String", 0),
    ("FromIterator", 0),
];

/// `Vec::from_iter`, `std::collections::HashMap::<K, V>::from_iter` or
/// `FromIterator::from_iter` at `toks[i]`: the index of the path's first token and of `from_iter`,
/// and the type to collect into (`Vec<_>`, `HashMap<K, V>`; none for `FromIterator`). Not a
/// type of the file's own (`toks` has its `struct` / `enum` names).
fn from_iter_path(toks: &[SyntaxToken], i: usize) -> Option<(usize, usize, Option<String>)> {
    let at = |k: usize| toks.get(k).map(|t| t.text().to_string()).unwrap_or_default();
    // A `::` at `toks[k]` (one token in a path, two `:` in a macro's tokens): its length.
    let sep = |k: usize| {
        if at(k) == "::" {
            Some(1)
        } else if at(k) == ":" && at(k + 1) == ":" {
            Some(2)
        } else {
            None
        }
    };
    let name = at(i);
    let &(_, n_params) = FROM_ITER_TYPES.iter().find(|(t, _)| *t == name)?;
    // `std::collections::` / `std::iter::` before it.
    let mut first = i;
    loop {
        let back = if first >= 2 && at(first - 1) == "::" {
            2
        } else if first >= 3 && at(first - 1) == ":" && at(first - 2) == ":" {
            3
        } else {
            break;
        };
        let seg = at(first - back);
        let std_seg = matches!(
            seg.as_str(),
            "std" | "core" | "alloc" | "collections" | "vec" | "string" | "iter" | "hash_map"
                | "hash_set" | "btree_map" | "btree_set" | "vec_deque" | "linked_list"
                | "binary_heap"
        );
        if !std_seg {
            break;
        }
        first -= back;
    }
    if first > 0 && matches!(at(first - 1).as_str(), "::" | ":" | "." | "<" | "as") {
        return None;
    }
    let mut k = i + 1 + sep(i + 1)?;
    // A turbofish: `Vec::<u8>::from_iter`.
    let mut ty = (name != "FromIterator").then(|| match n_params {
        0 => name.clone(),
        n => format!("{name}<{}>", vec!["_"; n].join(", ")),
    });
    if at(k) == "<" {
        let open = k;
        let mut angle = 0;
        while k < toks.len() {
            match at(k).as_str() {
                "<" => angle += 1,
                ">" => angle -= 1,
                ">>" => angle -= 2,
                _ => {}
            }
            k += 1;
            if angle <= 0 {
                break;
            }
        }
        if angle != 0 {
            return None;
        }
        let inner: Vec<String> = (open + 1..k - 1).map(at).collect();
        // (`Vec::<Vec<u8>>::from_iter`: the `>>` closes both.)
        let mut inner = inner.join(" ");
        if at(k - 1) == ">>" {
            inner.push('>');
        }
        ty = Some(format!("{name}<{inner}>"));
        k += sep(k)?;
    }
    if at(k) != "from_iter" {
        return None;
    }
    // (Not the file's own type of that name. Checked last: it reads the whole file.)
    if toks.iter().zip(toks.iter().skip(1)).any(|(kw, n)| {
        matches!(kw.kind(), SyntaxKind::STRUCT_KW | SyntaxKind::ENUM_KW | SyntaxKind::TRAIT_KW)
            && n.text() == name
    }) {
        return None;
    }
    Some((first, k, ty))
}

/// `let g = Vec::from_iter;` with no type argument written (`ty` is `Vec<_>`): a generic fn over
/// the items, with the bounds the collection's `FromIterator` impl has. (Not `HashMap` /
/// `HashSet`: their hasher would be unknown, rustc's E0283.)
fn from_iter_generic_fn(ty: &str, i: usize) -> Option<String> {
    let (name, args) = ty.split_once('<')?;
    if args.split(',').any(|a| a.trim().trim_end_matches('>') != "_") {
        return None;
    }
    let collect = "{ it.into_iter().collect() }";
    let (params, ret, bound) = match name {
        "Vec" => ("I: IntoIterator", "Vec<I::Item>".to_string(), ""),
        "VecDeque" | "LinkedList" => ("I: IntoIterator", format!("std::collections::{name}<I::Item>"), ""),
        "BTreeSet" | "BinaryHeap" => (
            "I: IntoIterator",
            format!("std::collections::{name}<I::Item>"),
            " where I::Item: Ord",
        ),
        "BTreeMap" => (
            "K: Ord, V, I: IntoIterator<Item = (K, V)>",
            "std::collections::BTreeMap<K, V>".to_string(),
            "",
        ),
        _ => return None,
    };
    Some(format!(
        "{{ fn __from_iter{i}<{params}>(it: I) -> {ret}{bound} {collect} __from_iter{i} }}"
    ))
}

/// The item type a std collection `ty` (`Vec<i32>`, `HashMap<u8, String>`) is collected from,
/// when every type argument is written.
fn from_iter_item(ty: &str) -> Option<String> {
    let (name, rest) = ty.split_once('<').unwrap_or((ty, ">"));
    let inner = rest.trim().strip_suffix('>')?;
    if ty.contains('_') {
        return None;
    }
    let mut args = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (k, c) in inner.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[start..k].trim());
                start = k + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim());
    Some(match (name.trim(), args.as_slice()) {
        ("String", _) => "char".to_string(),
        ("HashMap" | "BTreeMap", [k, v, ..]) => format!("({k}, {v})"),
        (_, [e, ..]) if !e.is_empty() => e.to_string(),
        _ => return None,
    })
}

/// `Type::from` named as a value at `toks[i]` (an argument: `.map(PathBuf::from)`,
/// `std::path::PathBuf::from`, `f64::from`): the index of the path's first token and of `from`.
/// `From::from` and `String::from` are taken elsewhere.
fn from_value_path(toks: &[SyntaxToken], i: usize) -> Option<(usize, usize)> {
    let at = |k: usize| toks.get(k).map(|t| t.text().to_string()).unwrap_or_default();
    let sep = |k: usize| {
        if at(k) == "::" {
            Some(1)
        } else if at(k) == ":" && at(k + 1) == ":" {
            Some(2)
        } else {
            None
        }
    };
    let name = at(i);
    let type_like = name.starts_with(|c: char| c.is_ascii_uppercase())
        || PRIMS.contains(&name.as_str());
    let taken = matches!(name.as_str(), "From" | "String");
    // (`.map(Self::from)` in an impl too.)
    let kind = toks[i].kind();
    if !matches!(kind, SyntaxKind::IDENT | SyntaxKind::SELF_TYPE_KW) || !type_like || taken {
        return None;
    }
    let m = i + 1 + sep(i + 1)?;
    if at(m) != "from" || !matches!(at(m + 1).as_str(), ")" | ",") {
        return None;
    }
    // `std::path::` before it.
    let mut first = i;
    loop {
        let back = if first >= 2 && at(first - 1) == "::" {
            2
        } else if first >= 3 && at(first - 1) == ":" && at(first - 2) == ":" {
            3
        } else {
            break;
        };
        if toks[first - back].kind() != SyntaxKind::IDENT {
            break;
        }
        first -= back;
    }
    // An argument: after `(` or `,`.
    (first > 0 && matches!(at(first - 1).as_str(), "(" | ",")).then_some((first, m))
}
