//! Path-call syntax for methods of built-in types, as a pre-pass over the source tokens (so it
//! reaches macro arguments too): `u64::pow(2, 40)` is written as `(2u64).pow(40)`, `str::len(&s)`
//! as `(s).len()`, and a method named as a value (`.fold(0, i32::max)`, `.map(str::trim)`)
//! becomes a closure, `(|a, b| a.max(b))`. The same for some methods of `Vec`, `Option` and std
//! traits (`.map(Vec::len)`, `.map(Into::into)`, `std::error::Error::source(&e)`). An `OffsetMap`
//! keeps positions pointing at the original text.

use ra_ap_syntax::{AstNode, SourceFile, SyntaxKind, SyntaxToken};

use crate::macros::EDITION;
use crate::mbe::OffsetMap;

const PRIMS: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "char", "bool", "str", "String",
];

/// Other types and std traits whose methods are named by path (`Vec::len`,
/// `std::error::Error::source`); their receivers' types aren't written into the closures.
const OTHERS: &[&str] = &["Vec", "Option", "Into", "Clone", "ToString", "Error"];

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
        ("Into", "into") | ("Clone", "clone") | ("ToString", "to_string") => return Some(0),
        ("Error", "source" | "description") => return Some(0),
        ("Vec" | "Option" | "Into" | "Clone" | "ToString" | "Error", _) => return None,
        _ => {}
    }
    let numeric = !matches!(prim, "char" | "bool" | "str" | "String");
    let float = matches!(prim, "f32" | "f64");
    if numeric {
        if method.starts_with("checked_")
            || method.starts_with("wrapping_")
            || method.starts_with("saturating_")
        {
            let unary = method.ends_with("_neg") || method.ends_with("_abs");
            return Some(if unary { 0 } else { 1 });
        }
        return Some(match method {
            "abs" | "signum" | "to_string" | "count_ones" | "count_zeros" | "leading_zeros"
            | "trailing_zeros" | "is_power_of_two" | "ilog2" | "ilog10" | "isqrt"
            | "is_positive" | "is_negative" => 0,
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

pub fn has_prim_paths(text: &str) -> bool {
    PRIMS
        .iter()
        .chain(OTHERS)
        .any(|p| text.contains(&format!("{p}::")))
}

/// Rewrite path calls and method values of built-in types in `text`, recording the edits in
/// `map`. Whether anything changed.
pub fn expand(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = SourceFile::parse(text, EDITION).tree();
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
    // Don't touch `macro_rules!` bodies or `use` items.
    let skip = |t: &SyntaxToken| {
        t.parent_ancestors().any(|a| {
            matches!(
                a.kind(),
                SyntaxKind::MACRO_RULES | SyntaxKind::USE | SyntaxKind::ATTR
            )
        })
    };
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut i = 0;
    while i + 3 < toks.len() {
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
            while j < toks.len() {
                let t = at(j);
                match t.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" if depth > 0 => depth -= 1,
                    ")" if depth == 0 => break,
                    "," if depth == 0 => break,
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
                edits.push((end(last), start(j + 1), format!("{suffix}).{method}(")));
            } else {
                edits.push((end(last), end(last), format!("{suffix}).{method}(")));
            }
            i = j;
            continue;
        }
        // A method named as a value: a closure over its arguments.
        if !matches!(at(m + 1).as_str(), "::" | ":" | "!") {
            let params: Vec<String> = (0..=n_args).map(|k| format!("__ufcs{k}")).collect();
            // The receiver's type, as rustc knows it from the path (`i32::abs` takes an `i32`).
            let typed = if matches!(prim.as_str(), "str" | "String") || other {
                params.join(", ")
            } else {
                let mut all = params.clone();
                all[0] = format!("{}: {prim}", params[0]);
                all.join(", ")
            };
            let closure = format!(
                "(|{typed}| {}.{method}({}))",
                params[0],
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
    let file = SourceFile::parse(text, EDITION).tree();
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
                prev.as_str(),
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
            match t.as_str() {
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
        let path_like = c > i + 1
            && (toks[i + 1].kind() == SyntaxKind::IDENT
                || matches!(first.as_str(), "Self" | "crate" | "self" | "super" | "$")
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
