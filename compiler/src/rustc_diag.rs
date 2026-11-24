//! Diagnostics in rustc's shape and its human rendering.
//!
//! The checkers report a diagnostic as one line of text: rustc's headline, often with its label
//! after a colon (`mismatched types: expected `u8`, found `&str``) and a hint in parentheses.
//! `shape` splits that into rustc's parts (code, headline, label, `help:` lines) by the message's
//! wording, as `diag_help` keys on it, and `render` prints them the way rustc does.

use crate::codegen::{Child, CompileOutput, Diagnostic, SubSpan};
use crate::lineindex::LineIndex;
use ra_ap_syntax::{AstNode, ast};
use serde::Deserialize;

/// Gives each diagnostic rustc's structure, puts them in the order rustc reports them (lints
/// rustc wouldn't get to dropped), adds the lint-level notes, then renders them all into
/// `rendered`.
pub fn finish(src: &str, filename: &str, out: &mut CompileOutput) {
    let idx = LineIndex::new(src);
    for d in &mut out.diagnostics {
        if d.lint.is_none() {
            shape(d, src, &idx);
        }
    }
    order(src, &mut out.diagnostics);
    lint_notes(&idx, &mut out.diagnostics);
    let views: Vec<View> = out.diagnostics.iter().map(View::from).collect();
    out.rendered = render(src, filename, &views);
}

// ------------------------------------------------------------------------------------ ordering

/// rustc's borrow checker's errors: it runs per function after type checking, so the lints
/// before it still come (unlike after a type error, which stops the build before them).
const BORROWCK: &[&str] = &[
    "E0373", "E0381", "E0382", "E0384", "E0499", "E0502", "E0503", "E0505", "E0506", "E0507",
    "E0508", "E0509", "E0515", "E0521", "E0594", "E0596", "E0597", "E0712", "E0713", "E0716",
];

/// Where a diagnostic comes in rustc's output: the compiler pass that reports it (see `order`),
/// then for the per-function passes the function, then the step within it.
fn phase(d: &Diagnostic) -> (u8, u8) {
    match d.lint.as_deref() {
        Some("unused_imports" | "while_true" | "unused_parens" | "non_camel_case_types") => (0, 0),
        Some("unreachable_code") => (2, 0),
        Some("unreachable_patterns") => (3, 0),
        Some("unused_mut") => (3, 2),
        Some("unused_variables") => (3, 3),
        Some("unused_assignments") => (3, 4),
        Some("dead_code") => (4, 0),
        Some(_) => (5, 0),
        None if borrowck(d) => (3, 1),
        None => (1, 0),
    }
}

fn borrowck(d: &Diagnostic) -> bool {
    d.severity == "error"
        && (d.code.as_deref().is_some_and(|c| BORROWCK.contains(&c))
            || d.message == "lifetime may not live long enough")
}

/// Puts the diagnostics in rustc's order: the early lints (imports, `while true`, parentheses,
/// type names) by position; type errors; unreachable code; then each function's patterns,
/// borrow errors, `mut`s, variables and assignments; dead code; the late lints. After a type
/// error rustc stops before unreachable code, so the lints from there on are dropped.
fn order(src: &str, diags: &mut Vec<Diagnostic>) {
    if diags.iter().all(|d| d.lint.is_none()) {
        return;
    }
    // (After a borrow error rustc still finishes each function's lints, but stops before
    // dead code and the late lints.)
    let errors = diags.iter().filter(|d| d.lint.is_none() && d.severity == "error");
    let typeck_failed = errors.clone().any(|d| !borrowck(d));
    let borrowck_failed = errors.clone().any(borrowck);
    if typeck_failed {
        diags.retain(|d| phase(d).0 < 2);
    } else if borrowck_failed {
        diags.retain(|d| phase(d).0 < 4);
    }
    // The functions, for the per-function passes (a closure's lints are its function's).
    let file = crate::macros::parse_tree(src);
    let fns: Vec<(usize, usize)> = file
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .map(|f| {
            let r = f.syntax().text_range();
            (usize::from(r.start()), usize::from(r.end()))
        })
        .collect();
    let body = |at: usize| {
        fns.iter()
            .filter(|(s, e)| *s <= at && at < *e)
            .map(|(s, _)| *s)
            .max()
            .unwrap_or(0)
    };
    diags.sort_by_cached_key(|d| {
        let (p, step) = phase(d);
        let f = if p == 3 { body(d.start) } else { 0 };
        (p, f, step, d.start)
    });
}

/// The lint group rustc's "on by default" note names.
fn lint_group(lint: &str) -> Option<&'static str> {
    match lint {
        "non_snake_case" | "non_camel_case_types" | "non_upper_case_globals" => {
            Some("nonstandard_style")
        }
        "while_true" | "inexact_64bit" => None,
        _ => Some("unused"),
    }
}

/// rustc's note on the first diagnostic of each lint (and level): where its level comes from.
fn lint_notes(idx: &LineIndex, diags: &mut [Diagnostic]) {
    let mut seen: std::collections::HashSet<(String, Option<(usize, usize)>)> = Default::default();
    for d in diags {
        let Some(lint) = d.lint.clone() else { continue };
        let at = d.lint_attr.as_ref().map(|(s, e, _)| (*s, *e));
        if !seen.insert((lint.clone(), at)) {
            continue;
        }
        let level = if d.severity == "error" { "deny" } else { "warn" };
        match &d.lint_attr {
            None => d.notes.push(match lint_group(&lint) {
                Some(g) => format!("`#[{level}({lint})]` (part of `#[{level}({g})]`) on by default"),
                None => format!("`#[{level}({lint})]` on by default"),
            }),
            Some((s, e, written)) => {
                let mut span = SubSpan::at(idx, *s, *e, String::new());
                span.primary = true;
                d.children.push(Child {
                    kind: "note".into(),
                    message: "the lint level is defined here".into(),
                    spans: vec![span],
                    edits: Vec::new(),
                });
                if *written != lint {
                    d.children.push(Child {
                        kind: "note".into(),
                        message: format!("`#[{level}({lint})]` implied by `#[{level}({written})]`"),
                        spans: Vec::new(),
                        edits: Vec::new(),
                    });
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------ shaping

/// Splits `d.message` into rustc's code, headline, label and help lines. Idempotent.
pub fn shape(d: &mut Diagnostic, src: &str, idx: &LineIndex) {
    if d.code.is_some() || d.label.is_some() {
        return;
    }
    // What the message says about the implementation moves to the editor's help.
    let (plain, aside) = plain(&d.message);
    if !aside.is_empty() {
        d.help = Some(match d.help.take() {
            Some(h) => format!("{h} {aside}"),
            None => aside,
        });
    }
    d.message = plain;
    let (msg, hint) = split_hint(&d.message);
    let mut msg = msg.to_string();
    let mut hints: Vec<String> = hint.into_iter().map(str::to_string).collect();
    // (`..; did you mean `x`?`)
    if let Some(i) = msg.find("; did you mean ") {
        let name = ticks(&msg[i..]).first().map(|s| s.to_string());
        if let Some(n) = name {
            hints.push(format!("a similar name exists: `{n}`"));
        }
        msg.truncate(i);
    }
    let mut s = Shaped::default();
    rules(&msg, &mut s, d, src);
    // (rustc leaves the label off a missing method when it suggests another.)
    if s.code == Some("E0599") && !hints.is_empty() && s.label.as_deref().is_some_and(|l| l.starts_with("method not found")) {
        s.label = None;
    }
    let headline = s.headline.take().unwrap_or(msg);
    d.message = headline;
    d.code = s.code.map(str::to_string);
    d.label = s.label;
    // rustc's own help replaces the hint it stands for.
    if s.helps.is_empty() && s.suggestion.is_none() {
        d.helps = hints.into_iter().map(|h| tidy_hint(&h)).collect();
    } else {
        d.helps = s.helps;
    }
    d.notes = s.notes;
    if let Some((message, edits)) = s.suggestion {
        d.children.push(Child {
            kind: "help".into(),
            message,
            spans: Vec::new(),
            edits: edits.into_iter().map(|(a, b, text)| SubSpan::at(idx, a, b, text)).collect(),
        });
    }
    for (start, end, label) in s.spans {
        if !d.spans.iter().any(|x| x.start == start && x.end == end) {
            d.spans.push(SubSpan::at(idx, start, end, label));
        }
    }
    if let Some((start, end)) = s.focus
        && end <= src.len()
        && start <= end
    {
        let (l0, c0) = idx.line_col(start);
        let (l1, c1) = idx.line_col(end);
        (d.start, d.end) = (start, end);
        (d.line, d.column, d.end_line, d.end_column) = (l0 + 1, c0 + 1, l1 + 1, c1 + 1);
    }
}

#[derive(Default)]
struct Shaped {
    code: Option<&'static str>,
    headline: Option<String>,
    label: Option<String>,
    helps: Vec<String>,
    notes: Vec<String>,
    /// A narrower primary span (the method's name in a call, the operator of a binary
    /// expression).
    focus: Option<(usize, usize)>,
    spans: Vec<(usize, usize, String)>,
    /// rustc's suggestion: its message and edits (a range and the text replacing it).
    suggestion: Option<(String, Vec<(usize, usize, String)>)>,
}

/// Whether `s` talks about the implementation (JS, this compiler, its docs).
fn about_implementation(s: &str) -> bool {
    let word = |w: &str| {
        s.match_indices(w).any(|(i, _)| {
            !s[..i].ends_with(|c: char| c.is_alphanumeric())
                && !s[i + w.len()..].starts_with(|c: char| c.is_alphanumeric())
        })
    };
    word("JS")
        || word("JavaScript")
        || s.contains("CarciniScript")
        || s.contains("FEATURES.md")
        || s.contains("the spike")
        || s.contains("std surface")
}

/// The message without what it says about the implementation: "not supported yet" loses its
/// "yet", and each parenthetical, `; ..` or `: ..` clause about JS or this compiler goes to the
/// second value (sentences for the editor's help).
fn plain(m: &str) -> (String, String) {
    let mut msg = m
        .replace(" not supported yet", " not supported")
        .replace(" isn't supported yet", " isn't supported")
        .replace(" aren't supported yet", " aren't supported");
    if !about_implementation(&msg) {
        return (msg, String::new());
    }
    let mut aside: Vec<String> = Vec::new();
    // Parenthetical groups (outside backticks) about the implementation.
    loop {
        let b = msg.as_bytes();
        let mut in_tick = false;
        let mut open: Vec<usize> = Vec::new();
        let mut cut = None;
        for i in 0..b.len() {
            match b[i] {
                b'`' => in_tick = !in_tick,
                b'(' if !in_tick => open.push(i),
                b')' if !in_tick => {
                    if let Some(o) = open.pop()
                        && open.is_empty()
                        && about_implementation(&msg[o + 1..i])
                    {
                        cut = Some((o, i + 1));
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some((o, c)) = cut else { break };
        aside.push(msg[o + 1..c - 1].to_string());
        let before = msg[..o].trim_end().to_string();
        msg = before + &msg[c..];
    }
    // `; ..` and `: ..` clauses about it (the headline before the first one stays).
    let mut out = String::new();
    let mut rest = msg.as_str();
    let mut first = true;
    while !rest.is_empty() {
        let mut in_tick = false;
        let mut split = None;
        for (i, c) in rest.char_indices() {
            match c {
                '`' => in_tick = !in_tick,
                ';' | ':' if !in_tick && rest[i + 1..].starts_with(' ') && !rest[..i].ends_with(':') && !rest[i + 1..].starts_with("::") => {
                    split = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let (part, sep, next) = match split {
            Some(i) => (&rest[..i], &rest[i..i + 2], &rest[i + 2..]),
            None => (rest, "", ""),
        };
        if !first && about_implementation(part) {
            aside.push(part.trim().to_string());
            // (The separator before it goes too.)
            if out.ends_with("; ") || out.ends_with(": ") {
                out.truncate(out.len() - 2);
            }
            if !next.is_empty() {
                out.push_str("; ");
            }
        } else {
            out.push_str(part);
            out.push_str(sep);
        }
        first = false;
        rest = next;
    }
    let out = out.trim_end().trim_end_matches([';', ':']).to_string();
    let aside = aside
        .iter()
        .map(|a| {
            let a = a.trim();
            let mut c = a.chars();
            let cap = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
            if cap.ends_with('.') { cap } else { cap + "." }
        })
        .collect::<Vec<_>>()
        .join(" ");
    (out, aside)
}

/// A hint as a `help:` line: rustc's helps start lower-case and have no final period.
fn tidy_hint(h: &str) -> String {
    h.trim().trim_end_matches('.').to_string()
}

/// The message and its trailing parenthesized hint (` (..)` at the end, outside backticks).
fn split_hint(m: &str) -> (&str, Option<&str>) {
    let t = m.trim_end();
    if !t.ends_with(')') {
        return (m, None);
    }
    // Walk back to the `(` that opens the final group, skipping backticked text.
    let b = t.as_bytes();
    let mut depth = 0i32;
    let mut in_tick = false;
    let mut i = b.len();
    while i > 0 {
        i -= 1;
        match b[i] {
            b'`' => in_tick = !in_tick,
            b')' if !in_tick => depth += 1,
            b'(' if !in_tick => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    if depth != 0 || i == 0 || b[i - 1] != b' ' || in_tick {
        return (m, None);
    }
    let head = t[..i - 1].trim_end();
    // (`foo()` written as the message's end is code, not a hint.)
    if head.is_empty() || head.ends_with('`') && t[i..].starts_with("()") {
        return (m, None);
    }
    (head, Some(&t[i + 1..t.len() - 1]))
}

/// The backticked parts of `s`.
fn ticks(s: &str) -> Vec<&str> {
    s.split('`').skip(1).step_by(2).collect()
}

/// rustc's words for an inferred number type in "expected .., found ..".
fn ty_words(label: &str) -> String {
    label
        .replace("`{integer}`", "integer")
        .replace("`{float}`", "floating-point number")
}

/// What rustc calls a type in "no method named `m` found for struct `S`".
fn type_kind(t: &str, src: &str) -> &'static str {
    let base = t.split('<').next().unwrap_or(t);
    if t.starts_with('&') {
        "reference"
    } else if t.starts_with('(') {
        "tuple"
    } else if t.starts_with('[') {
        if t.contains(';') { "array" } else { "slice" }
    } else if matches!(
        base,
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
            | "usize" | "f32" | "f64" | "bool" | "char" | "str" | "{integer}" | "{float}"
    ) {
        "type"
    } else if matches!(base, "Option" | "Result" | "Ordering" | "Cow")
        || src.contains(&format!("enum {base} "))
        || src.contains(&format!("enum {base}<"))
        || src.contains(&format!("enum {base}{{"))
    {
        "enum"
    } else {
        "struct"
    }
}

/// The byte range of the identifier `name` in `src[start..end]` (its last occurrence when
/// `last`), as a whole word.
fn find_word(src: &str, start: usize, end: usize, name: &str, last: bool) -> Option<(usize, usize)> {
    let text = src.get(start..end)?;
    let is_id = |c: char| c.is_alphanumeric() || c == '_';
    let hits: Vec<usize> = text
        .match_indices(name)
        .map(|(i, _)| i)
        .filter(|&i| {
            !text[..i].chars().next_back().is_some_and(is_id)
                && !text[i + name.len()..].chars().next().is_some_and(is_id)
        })
        .collect();
    let i = if last { *hits.last()? } else { *hits.first()? };
    Some((start + i, start + i + name.len()))
}

/// In `recv.method(args)` spanning `start..end`, the method's name.
fn method_name(src: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let text = src.get(start..end)?;
    if !text.ends_with(')') {
        return None;
    }
    // The `(` matching the final `)`.
    let b = text.as_bytes();
    let mut depth = 0;
    let mut open = None;
    for i in (0..b.len()).rev() {
        match b[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    open = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let open = open?;
    let head = text[..open].trim_end();
    // (A turbofish: `v.iter().sum::<i32>()`.)
    let head = match head.rfind("::<") {
        Some(t) if head.ends_with('>') => &head[..t],
        _ => head,
    };
    let name_start = head.rfind(|c: char| !(c.is_alphanumeric() || c == '_'))? + 1;
    if !head[..name_start].ends_with('.') || name_start == head.len() {
        return None;
    }
    Some((start + name_start, start + head.len()))
}

/// In `callee(args)` spanning `start..end`, the callee (`add`, `Foo::new`, or a method's name).
fn callee(src: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if let Some(m) = method_name(src, start, end) {
        return Some(m);
    }
    let text = src.get(start..end)?;
    let open = text.find('(')?;
    let head = text[..open].trim_end();
    (!head.is_empty() && !head.contains(|c: char| c.is_whitespace()))
        .then_some((start, start + head.len()))
}

/// The identifier starting at `at`.
fn word_at(src: &str, at: usize) -> Option<(usize, usize)> {
    let rest = src.get(at..)?;
    let n = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (n > 0).then_some((at, at + n))
}

/// A byte offset of a 1-based line and UTF-16 column.
fn offset_of(src: &str, line: usize, col: usize) -> Option<usize> {
    let start = if line <= 1 {
        0
    } else {
        src.match_indices('\n').nth(line - 2)?.0 + 1
    };
    let mut units = 0;
    for (i, c) in src[start..].char_indices() {
        if units + 1 >= col {
            return Some(start + i);
        }
        if c == '\n' {
            return None;
        }
        units += c.len_utf16();
    }
    None
}

/// The rules: rustc's code, headline, label and helps for each message rustc has.
fn rules(msg: &str, s: &mut Shaped, d: &Diagnostic, src: &str) {
    let t = ticks(msg);
    let t0 = t.first().copied().unwrap_or("");
    let has = |p: &str| msg.starts_with(p);
    let (start, end) = (d.start, d.end);

    // ---- types
    if let Some(rest) = msg.strip_prefix("mismatched types: ") {
        s.code = Some("E0308");
        s.headline = Some("mismatched types".into());
        // (A block's value: rustc points at the expression it ends with.)
        let (start, end) = block_tail(src, start, end).unwrap_or((start, end));
        if (start, end) != (d.start, d.end) {
            s.focus = Some((start, end));
        }
        let mut label = ty_words(rest);
        if label.ends_with("found `String`") && borrowed_str(src, start, end) {
            label = label.replace("found `String`", "found `&str`");
        }
        // `let x: T = value;`: rustc points at `T` too.
        if let Some((a, b)) = let_annotation(src, start) {
            if label.starts_with("expected `String`")
                && matches!(src[a..b].trim(), "&str" | "&'static str")
            {
                label = label.replacen("expected `String`", "expected `&str`", 1);
            }
            s.spans.push((a, b, "expected due to this".into()));
        } else if let Some((a, b, what)) = enclosing_call(src, start) {
            s.spans.push((a, b, format!("arguments to this {what} are incorrect")));
        }
        s.label = Some(label);
    } else if has("mismatched types") {
        s.code = Some("E0308");
    } else if let Some((head, rest)) = msg
        .split_once(" have incompatible types: ")
        .filter(|(h, _)| matches!(*h, "`if` and `else`" | "`match` arms"))
    {
        s.code = Some("E0308");
        s.headline = Some(format!("{head} have incompatible types"));
        let (start, end) = block_tail(src, start, end).unwrap_or((start, end));
        s.focus = Some((start, end));
        let mut label = ty_words(rest);
        if label.ends_with("found `String`") && borrowed_str(src, start, end) {
            label = label.replace("found `String`", "found `&str`");
        }
        s.label = Some(label);
        // `if c { a } else { b }`: the `if` branch's value set the type.
        if head.starts_with("`if`")
            && let Some((a, b)) = then_tail(src, d.start)
        {
            s.spans.push((a, b, "expected because of this".into()));
        }
    } else if has("type annotations needed") {
        s.code = Some("E0282");
        if let Some(rest) = msg.strip_prefix("type annotations needed: ") {
            s.headline = Some("type annotations needed".into());
            s.helps.push(rest.into());
        }
    } else if has("type `") && msg.ends_with("` cannot be dereferenced") {
        s.code = Some("E0614");
        s.label = Some("can't be dereferenced".into());
    } else if has("can't call method `") && msg.contains("on ambiguous numeric type") {
        s.code = Some("E0689");
        s.focus = method_name(src, start, end);
    // ---- names
    } else if (has("cannot find value `")
        || has("cannot find function `")
        || has("cannot find type `"))
        && msg.ends_with("in this scope")
    {
        s.code = Some("E0425");
        s.label = Some("not found in this scope".into());
    } else if let Some(rest) = msg.strip_prefix("failed to resolve: ") {
        s.code = Some("E0433");
        if let Some(ty) = rest.strip_prefix("use of undeclared type `") {
            let ty = ty.trim_end_matches('`');
            s.headline = Some(format!("cannot find type `{ty}` in this scope"));
            s.focus = find_word(src, start, end, ty, false);
        } else {
            s.headline = Some(format!("failed to resolve: {rest}"));
        }
        s.label = Some(rest.into());
    } else if has("unresolved import `") {
        s.code = Some("E0432");
        if let Some((path, item)) = t0.rsplit_once("::") {
            s.label = Some(format!("no `{item}` in `{path}`"));
        }
    } else if has("file not found for module `") {
        s.code = Some("E0583");
    } else if has("the name `") && msg.ends_with("is defined multiple times") {
        s.code = Some("E0428");
    // ---- methods, fields, calls
    } else if let Some(rest) = msg.strip_prefix("no method named `") {
        s.code = Some("E0599");
        s.focus = method_name(src, start, end);
        let m = rest.split('`').next().unwrap_or("");
        // `.. found for `T``: rustc says what kind of type, and where it looked.
        if let Some(ty) = rest
            .split_once("` found for `")
            .map(|(_, t)| t.trim_end_matches('`'))
            .filter(|t| !t.contains('`'))
        {
            s.headline = Some(format!(
                "no method named `{m}` found for {} `{ty}` in the current scope",
                type_kind(ty, src)
            ));
            s.label = Some(format!("method not found in `{ty}`"));
        } else if !msg.ends_with("in the current scope") && msg.contains(" found for ") {
            // (An iterator's type is a std struct.)
            let msg = msg.replacen(" found for iterator `", " found for struct `", 1);
            s.headline = Some(format!("{msg} in the current scope"));
        }
    } else if has("no function or associated item named `")
        || has("no variant or associated item named `")
        || has("no associated item named `")
    {
        s.code = Some("E0599");
        let what = msg
            .trim_start_matches("no ")
            .split(" named `")
            .next()
            .unwrap_or("associated item");
        if let Some(ty) = msg.split(" found for ").nth(1) {
            let ty = ty.trim_end_matches(" in the current scope");
            let tt = ticks(ty);
            if let Some(name) = tt.first() {
                s.label = Some(format!("{what} not found in `{name}`"));
            }
            if !msg.ends_with("in the current scope") {
                s.headline = Some(format!("{msg} in the current scope"));
            }
        }
    } else if let Some(rest) = msg
        .strip_prefix("this function takes ")
        .map(|r| ("function", r))
        .or_else(|| msg.strip_prefix("this method takes ").map(|r| ("method", r)))
        .or_else(|| msg.strip_prefix("this enum variant takes ").map(|r| ("enum variant", r)))
        .or_else(|| msg.strip_prefix("this struct takes ").map(|r| ("struct", r)))
    {
        let (what, rest) = rest;
        s.code = Some("E0061");
        // `.. but 1 were supplied`: rustc's grammar.
        if let Some((want, got)) = rest.split_once(" but ") {
            let n: usize = got.split(' ').next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let supplied = if n == 1 {
                "1 argument was supplied".to_string()
            } else {
                format!("{n} arguments were supplied")
            };
            s.headline = Some(format!("this {what} takes {want} but {supplied}"));
        }
        s.focus = callee(src, start, end);
    } else if has("this closure takes ") {
        s.code = Some("E0057");
    } else if let Some(rest) = msg.strip_prefix("no field `") {
        s.code = Some("E0609");
        s.label = Some("unknown field".into());
        let f = rest.split('`').next().unwrap_or("");
        s.focus = find_word(src, start, end, f, true);
    } else if has("struct `") && msg.contains("` has no field named `") {
        s.code = Some("E0560");
        s.label = Some(format!("`{t0}` does not have this field"));
    } else if has("variant `") && msg.contains("` has no field named `") {
        s.code = Some("E0559");
        s.label = Some("field does not exist".into());
    } else if has("missing field `") || has("missing fields `") {
        s.code = Some("E0063");
        if let Some((what, _)) = msg
            .strip_prefix("missing field ")
            .or_else(|| msg.strip_prefix("missing fields "))
            .and_then(|r| r.split_once(" in initializer of "))
        {
            s.label = Some(format!("missing {what}"));
        }
    } else if has("expected function, found ") {
        s.code = Some("E0618");
    } else if has("cannot index into a value of type `") {
        s.code = Some("E0608");
    } else if has("not all trait items implemented, missing: ") {
        s.code = Some("E0046");
        s.label = Some(format!(
            "missing {} in implementation",
            msg.trim_start_matches("not all trait items implemented, missing: ")
        ));
    } else if has("method `") && msg.contains("` is not a member of trait `") {
        s.code = Some("E0407");
        s.label = Some(format!("not a member of trait `{}`", t.get(1).unwrap_or(&"")));
    } else if has("type `") && msg.contains("` is not a member of trait `") {
        s.code = Some("E0437");
    // ---- traits and operators
    } else if let Some(rest) = msg.strip_prefix("the trait bound `") {
        if let Some((ty, tr)) = rest
            .strip_suffix("` is not satisfied")
            .and_then(|b| b.split_once(": "))
        {
            s.code = Some("E0277");
            s.label = Some(format!("the trait `{tr}` is not implemented for `{ty}`"));
            s.focus = method_name(src, start, end);
        }
    } else if let Some(ty) = msg
        .strip_suffix("` doesn't implement `std::fmt::Display`")
        .and_then(|m| m.strip_prefix('`'))
    {
        s.code = Some("E0277");
        s.label = Some(format!("`{ty}` cannot be formatted with the default formatter"));
        s.helps
            .push(format!("the trait `std::fmt::Display` is not implemented for `{ty}`"));
        s.notes.push(
            "in format strings you may be able to use `{:?}` (or `{:#?}` for pretty-print) instead"
                .into(),
        );
    } else if let Some(ty) = msg
        .strip_suffix("` doesn't implement `Debug`")
        .or_else(|| msg.strip_suffix("` doesn't implement `std::fmt::Debug`"))
        .and_then(|m| m.strip_prefix('`'))
    {
        s.code = Some("E0277");
        s.label = Some(format!("`{ty}` cannot be formatted using `{{:?}}`"));
        s.helps.push(format!("the trait `Debug` is not implemented for `{ty}`"));
        let base = ty.split('<').next().unwrap_or(ty);
        s.notes.push(format!(
            "add `#[derive(Debug)]` to `{base}` or manually `impl Debug for {base}`"
        ));
    } else if has("the type `") && msg.contains("` cannot be indexed by `") {
        s.code = Some("E0277");
        let by = t.get(1).copied().unwrap_or("");
        if t0.starts_with('[') {
            s.label = Some("slice indices are of type `usize` or ranges of `usize`".into());
            s.helps.push(format!("the trait `SliceIndex<{t0}>` is not implemented for `{by}`"));
        } else if t0 == "str" {
            s.label = Some("string indices are ranges of `usize`".into());
            s.helps.push(format!("the trait `SliceIndex<str>` is not implemented for `{by}`"));
        }
    } else if has("the `?` operator can only be used in ") {
        s.code = Some("E0277");
        s.label = Some(
            "cannot use the `?` operator in a function that returns `()`".to_string(),
        )
        .filter(|_| msg.contains("returns `()`"));
    } else if has("the `?` operator can only be applied to ") {
        s.code = Some("E0277");
    } else if has("`?` couldn't convert the error to `") {
        s.code = Some("E0277");
    } else if has("can't compare `") {
        s.code = Some("E0277");
    } else if let Some(rest) = msg.strip_prefix("cannot ").filter(|_| {
        ["add `", "subtract `", "multiply `", "divide `", "calculate the remainder of `"]
            .iter()
            .any(|p| msg["cannot ".len()..].starts_with(p))
    }) {
        // rustc: E0369 when the left side has no such operator at all, E0277 for a number
        // with the wrong right side.
        let num = |t: &str| {
            matches!(
                t,
                "{integer}" | "{float}" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8"
                    | "u16" | "u32" | "u64" | "u128" | "usize" | "f32" | "f64"
            )
        };
        // ("cannot add `R` to `L`", "cannot subtract `R` from `L`", but "cannot multiply `L` by
        // `R`".)
        let rhs_first = rest.starts_with("add") || rest.starts_with("subtract");
        let (lhs, rhs) = if rhs_first {
            (t.get(1).copied().unwrap_or(""), t0)
        } else {
            (t0, t.get(1).copied().unwrap_or(""))
        };
        s.code = Some(if num(lhs) { "E0277" } else { "E0369" });
        if matches!(lhs, "&str" | "&String") && msg.starts_with("cannot add") {
            s.label = Some(if rhs == "String" {
                format!("`+` cannot be used to concatenate a `{lhs}` with a `String`")
            } else {
                "`+` cannot be used to concatenate two `&str` strings".to_string()
            });
            s.notes.push("string concatenation requires an owned `String` on the left".into());
            s.helps.push("create an owned `String` from a string reference".into());
        }
        if num(lhs) {
            let op = binop_symbol(msg);
            s.label = Some(format!("no implementation for `{lhs} {op} {rhs}`"));
            s.helps
                .push(format!("the trait `{}<{rhs}>` is not implemented for `{lhs}`", op_trait(op)));
        }
        binop_focus(s, src, start, end);
    } else if has("binary operation `") && msg.contains("cannot be applied to type") {
        s.code = Some("E0369");
        binop_focus(s, src, start, end);
    } else if has("binary assignment operation `") {
        s.code = Some("E0368");
    } else if has("cannot apply unary operator `") {
        s.code = Some("E0600");
    } else if has("non-primitive cast: ") {
        s.code = Some("E0605");
    } else if has("casting `") && msg.ends_with("is invalid") {
        s.code = Some("E0606");
    // ---- patterns
    } else if let Some(rest) = msg.strip_prefix("non-exhaustive patterns: ") {
        s.code = Some("E0004");
        if let Some(pats) = rest.strip_suffix(" not covered") {
            let one = !pats.contains("` and `") && !pats.contains("`, `");
            s.label = Some(format!(
                "{} {pats} not covered",
                if one { "pattern" } else { "patterns" }
            ));
        }
    } else if has("refutable pattern in ") {
        s.code = Some("E0005");
    } else if let Some(rest) = msg.strip_prefix("this pattern has ") {
        s.code = Some("E0023");
        // `.. but `E::A` has 2 fields`: rustc names what the path is.
        if let Some((have, want)) = rest.split_once(", but ") {
            let want_n = want.rsplit(' ').nth(1).unwrap_or("");
            let fields = want.rsplit(' ').next().unwrap_or("fields");
            let what = if t0.contains("::") { "tuple variant" } else { "tuple struct" };
            if want.starts_with('`') {
                s.headline =
                    Some(format!("this pattern has {have}, but the corresponding {what} has {want_n} {fields}"));
            }
            let have_n = have.split(' ').next().unwrap_or("");
            s.label = Some(format!("expected {want_n} {fields}, found {have_n}"));
        }
    } else if has("variable `") && msg.contains("is not bound in all patterns") {
        s.code = Some("E0408");
    // ---- mutability, moves, borrows
    } else if has("cannot borrow `") && msg.ends_with(", as it is not declared as mutable") {
        s.code = Some("E0596");
        s.label = Some("cannot borrow as mutable".into());
        s.helps.push("consider changing this to be mutable".into());
    } else if has("cannot borrow `") && msg.contains(", as it is behind a `&` reference") {
        s.code = Some("E0596");
        s.label = Some(format!(
            "`{t0}` is a `&` reference, so the data it refers to cannot be borrowed as mutable"
        ));
    } else if has("cannot borrow data in a `&` reference as mutable") {
        s.code = Some("E0596");
        s.label = Some("cannot borrow as mutable".into());
    } else if has("cannot assign twice to immutable variable `") {
        s.code = Some("E0384");
        s.label = Some("cannot assign twice to immutable variable".into());
        if let Some(end) = assignment_end(src, end) {
            s.focus = Some((start, end));
        }
        match binding_before(src, start, t0) {
            Some(b) => {
                s.spans.push((b.0, b.1, format!("first assignment to `{t0}`")));
                s.suggestion = Some(("consider making this binding mutable".into(), vec![(b.0, b.0, "mut ".into())]));
            }
            None => s.helps.push("consider making this binding mutable".into()),
        }
    } else if has("cannot assign to `") && msg.ends_with("is not declared as mutable") {
        s.code = Some("E0594");
        s.label = Some("cannot assign".into());
        s.helps.push("consider changing this to be mutable".into());
    } else if has("cannot assign to `") && msg.contains("which is behind a `&` reference") {
        s.code = Some("E0594");
    } else if has("use of moved value: `")
        || has("borrow of moved value: `")
        || has("use of partially moved value: `")
        || has("borrow of partially moved value: `")
    {
        s.code = Some("E0382");
        let borrow = has("borrow of");
        let partially = msg.contains("partially moved value");
        s.label = Some(format!(
            "value {} here after {}move",
            if borrow { "borrowed" } else { "used" },
            if partially { "partial " } else { "" }
        ));
        // (The hint said where it moved: `moved on line 3:13 by `let``.)
        let moved = d.message.split("moved on line ").nth(1).and_then(|r| {
            let (l, c) = r.split(|c: char| !(c.is_ascii_digit() || c == ':')).next()?.split_once(':')?;
            offset_of(src, l.parse().ok()?, c.parse().ok()?)
        });
        let moved = moved.or_else(|| d.message.contains("moved here").then_some(start));
        if let Some(at) = moved
            && let Some((a, b)) = word_at(src, at)
        {
            let partial = has("use of partially") || has("borrow of partially");
            let what = if partial { "value partially moved here" } else { "value moved here" };
            if (a, b) == (start, end) {
                s.label = Some(format!(
                    "{what}, in previous iteration of loop"
                ));
            } else {
                s.spans.push((a, b, what.into()));
            }
        }
        s.helps.push("consider cloning the value if the performance cost is acceptable".into());
    } else if has("used binding `") && msg.ends_with("isn't initialized") {
        s.code = Some("E0381");
        s.label = Some(format!("`{t0}` used here but it isn't initialized"));
        if let Some(b) = binding_before(src, start, t0) {
            s.spans.push((b.0, b.1, "binding declared here but left uninitialized".into()));
        }
    } else if has("used binding `") && msg.ends_with("is possibly-uninitialized") {
        s.code = Some("E0381");
        s.label = Some(format!("`{t0}` used here but it is possibly-uninitialized"));
        if let Some(b) = binding_before(src, start, t0) {
            s.spans.push((b.0, b.1, "binding declared here but left uninitialized".into()));
        }
    } else if has("cannot borrow `") && msg.ends_with("as mutable more than once at a time") {
        s.code = Some("E0499");
        s.label = Some("second mutable borrow occurs here".into());
    } else if has("cannot borrow `") && msg.ends_with("as mutable because it is also borrowed as immutable") {
        s.code = Some("E0502");
        s.label = Some("mutable borrow occurs here".into());
    } else if has("cannot borrow `") && msg.ends_with("as immutable because it is also borrowed as mutable") {
        s.code = Some("E0502");
        s.label = Some("immutable borrow occurs here".into());
    } else if has("cannot move out of `") && msg.ends_with("because it is borrowed") {
        s.code = Some("E0505");
        s.label = Some(format!("move out of `{t0}` occurs here"));
    } else if has("cannot move out of `") && msg.contains("which is behind a") {
        s.code = Some("E0507");
    } else if has("cannot assign to `") && msg.ends_with("because it is borrowed") {
        s.code = Some("E0506");
        s.label = Some(format!("`{t0}` is assigned to here but it was already borrowed"));
        // (rustc's caret is on the place assigned, not the whole assignment.)
        if let Some(text) = src.get(start..end)
            && let Some(eq) = text.find(" =").or_else(|| text.find(" +=")).or_else(|| text.find(" -="))
        {
            s.focus = Some((start, start + text[..eq].trim_end().len()));
        }
    } else if has("cannot use `") && msg.ends_with("because it was mutably borrowed") {
        s.code = Some("E0503");
        s.label = Some(format!("use of borrowed `{t0}`"));
    } else if msg.starts_with('`') && msg.ends_with("` does not live long enough") {
        s.code = Some("E0597");
        s.label = Some("borrowed value does not live long enough".into());
        if let Some(b) = binding_before(src, start, t0) {
            s.spans.push((b.0, b.1, format!("binding `{t0}` declared here")));
        }
    } else if has("cannot return reference to ") {
        s.code = Some("E0515");
        s.label = Some("returns a reference to data owned by the current function".into());
    } else if has("cannot return value referencing ") {
        s.code = Some("E0515");
        s.label = Some("returns a value referencing data owned by the current function".into());
    } else if has("temporary value dropped while borrowed") {
        s.code = Some("E0716");
        s.label = Some("creates a temporary value which is freed while still in use".into());
    } else if has("closure may outlive the current function") {
        s.code = Some("E0373");
    // ---- items and modules
    } else if has("`await` is only allowed inside `async` functions and blocks") {
        s.code = Some("E0728");
        s.label = Some("only allowed inside `async` functions and blocks".into());
    } else if (has("function `") || has("struct `") || has("enum `") || has("constant `") || has("module `"))
        && (msg.ends_with("is private"))
    {
        s.code = Some("E0603");
    } else if has("field `") && msg.contains("is private") {
        s.code = Some("E0616");
        s.label = Some("private field".into());
    } else if has("method `") && msg.ends_with("is private") {
        s.code = Some("E0624");
        s.label = Some("private method".into());
    } else if has("expected value, found ") {
        s.code = Some("E0423");
    } else if has("use of mutable static is unsafe") {
        s.code = Some("E0133");
    } else if has("invalid left-hand side of assignment") {
        s.code = Some("E0070");
        s.label = Some("cannot assign to this expression".into());
    } else if has("conflicting implementations of trait `") {
        s.code = Some("E0119");
    } else if has("`main` function not found") {
        s.code = Some("E0601");
    } else if has("multiple applicable items in scope") {
        s.code = Some("E0034");
    } else if has("pattern requires ") && msg.contains(" but array has ") {
        s.code = Some("E0527");
    }
}

/// Whether `text` has a `;` outside its strings and brackets: whether a block's body is more
/// than one expression.
fn has_top_level_semi(text: &str) -> bool {
    let b = text.as_bytes();
    let (mut depth, mut i) = (0i32, 0);
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b';' if depth == 0 => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

/// `{ value }` spanning `start..end`: the value's range.
fn block_tail(src: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let text = src.get(start..end)?;
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let lead = inner.len() - inner.trim_start().len();
    let body = inner.trim();
    if body.is_empty() || has_top_level_semi(body) || body.starts_with("let ") {
        return None;
    }
    Some((start + 1 + lead, start + 1 + lead + body.len()))
}

/// The `{` matching the `}` at `close`.
fn open_brace(src: &str, close: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut depth = 0;
    let mut i = close + 1;
    while i > 0 {
        i -= 1;
        match b[i] {
            b'}' => depth += 1,
            b'{' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// For an `else` block starting at `at`, the value of the `if` block before it.
fn then_tail(src: &str, at: usize) -> Option<(usize, usize)> {
    let before = src.get(..at)?.trim_end().strip_suffix("else")?.trim_end();
    let close = before.len().checked_sub(1).filter(|&i| before.as_bytes()[i] == b'}')?;
    let open = open_brace(src, close)?;
    block_tail(src, open, close + 1)
}

/// Whether the expression at `start..end` is a `&str` (references are erased, so the checker
/// calls it `String`): a string literal, `.as_str()`, or a name declared `: &str`.
fn borrowed_str(src: &str, start: usize, end: usize) -> bool {
    let Some(text) = src.get(start..end).map(str::trim) else { return false };
    if text.starts_with('"') || text.starts_with("r\"") || text.starts_with("r#\"") || text.ends_with(".as_str()") {
        return true;
    }
    if text.is_empty() || !text.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return false;
    }
    let before = &src[..start];
    let Some(decl) = before.rfind(&format!("{text}: &")) else { return false };
    let ty = &before[decl + text.len() + 3..];
    let ty = match ty.strip_prefix('\'') {
        Some(r) => r.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_').trim_start(),
        None => ty,
    };
    let rebound = before[decl..].contains(&format!("let {text} ")) || before[decl..].contains(&format!("let mut {text} "));
    ty.starts_with("str") && !ty[3..].starts_with(|c: char| c.is_alphanumeric() || c == '_') && !rebound
}

/// `let x: T = ` just before `at`: `T`'s range.
fn let_annotation(src: &str, at: usize) -> Option<(usize, usize)> {
    let line_start = src.get(..at)?.rfind('\n').map_or(0, |i| i + 1);
    let prefix = &src[line_start..at];
    let p = prefix.trim_end().strip_suffix('=')?;
    if p.ends_with(['=', '<', '>', '!', '+', '-', '*', '/', '%', '&', '|', '^']) {
        return None;
    }
    let let_at = p.find("let ")?;
    let b = p.as_bytes();
    let colon = (let_at + 4..b.len()).find(|&i| {
        b[i] == b':' && b.get(i + 1) != Some(&b':') && (i == 0 || b[i - 1] != b':')
    })?;
    let ty = &p[colon + 1..];
    let lead = ty.len() - ty.trim_start().len();
    let ty_text = ty.trim();
    if ty_text.is_empty() {
        return None;
    }
    let a = line_start + colon + 1 + lead;
    Some((a, a + ty_text.len()))
}

/// An argument starting at `at` of a call: the callee's range (a method's name), and what rustc
/// calls it (`function`, `method`, `struct`, `enum variant`).
fn enclosing_call(src: &str, at: usize) -> Option<(usize, usize, &'static str)> {
    let before = src.get(..at)?;
    if !before.trim_end().ends_with(['(', ',']) {
        return None;
    }
    let b = before.as_bytes();
    let mut depth = 0;
    let mut i = b.len();
    let open = loop {
        if i == 0 {
            return None;
        }
        i -= 1;
        match b[i] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' if depth == 0 => {
                if b[i] == b'(' {
                    break i;
                }
                return None;
            }
            b'(' | b'[' | b'{' => depth -= 1,
            b';' if depth == 0 => return None,
            _ => {}
        }
    };
    let head = &before[..open];
    let name_start = head
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .map_or(0, |i| i + 1);
    let callee = &head[name_start..];
    if callee.is_empty() || callee.starts_with(':') || matches!(callee, "if" | "while" | "match" | "return" | "in") {
        return None;
    }
    let last = callee.rsplit("::").next().unwrap_or(callee);
    let what = if head[..name_start].ends_with('.') {
        "method"
    } else if matches!(last, "Some" | "Ok" | "Err")
        || (last.starts_with(char::is_uppercase)
            && callee.rsplit("::").nth(1).is_some_and(|p| p.starts_with(char::is_uppercase)))
    {
        "enum variant"
    } else if last.starts_with(char::is_uppercase) {
        "struct"
    } else {
        "function"
    };
    Some((name_start, open, what))
}

/// The operator's symbol in "cannot add `B` to `A`".
fn binop_symbol(msg: &str) -> &'static str {
    let w = msg.trim_start_matches("cannot ");
    if w.starts_with("add") {
        "+"
    } else if w.starts_with("subtract") {
        "-"
    } else if w.starts_with("multiply") {
        "*"
    } else if w.starts_with("divide") {
        "/"
    } else {
        "%"
    }
}

fn op_trait(op: &str) -> &'static str {
    match op {
        "+" => "Add",
        "-" => "Sub",
        "*" => "Mul",
        "/" => "Div",
        _ => "Rem",
    }
}

/// `a + b`: the operator as the primary span (rustc's caret), the operands as secondary ones.
fn binop_focus(s: &mut Shaped, src: &str, start: usize, end: usize) {
    let Some(text) = src.get(start..end) else { return };
    // The first top-level binary operator.
    let b = text.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    for i in 0..b.len() {
        match b[i] {
            b'"' => in_str = !in_str,
            _ if in_str => {}
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!'
                if depth == 0 && i > 0 =>
            {
                let mut j = i + 1;
                while j < b.len() && matches!(b[j], b'=' | b'<' | b'>') {
                    j += 1;
                }
                // (A unary minus or `->` isn't the operator.)
                if b[i] == b'-' && text[..i].trim_end().is_empty() {
                    continue;
                }
                s.focus = Some((start + i, start + j));
                return;
            }
            _ => {}
        }
    }
}

/// The end of `x = value` (before its `;`) for an assignment starting at the target ending
/// at `at`.
fn assignment_end(src: &str, at: usize) -> Option<usize> {
    let rest = src.get(at..)?;
    let after = rest.trim_start();
    let assigns = ["=", "+=", "-=", "*=", "/=", "%=", "|=", "&=", "^=", "<<=", ">>="]
        .iter()
        .any(|o| after.starts_with(*o) && !after.starts_with("=="));
    if !assigns {
        return None;
    }
    let line_end = rest.find('\n').unwrap_or(rest.len());
    let stmt = &rest[..line_end];
    let semi = stmt.find(';').unwrap_or(stmt.trim_end().len());
    Some(at + stmt[..semi].trim_end().len())
}

/// The name in the last `let name` / `let mut name` of `name` before `at`.
fn binding_before(src: &str, at: usize, name: &str) -> Option<(usize, usize)> {
    let text = src.get(..at)?;
    let mut best = None;
    for (i, _) in text.match_indices("let ") {
        let after = &text[i + 4..];
        let skip = if after.starts_with("mut ") { 4 } else { 0 };
        let at_name = i + 4 + skip;
        if let Some((a, b)) = word_at(src, at_name)
            && &src[a..b] == name
        {
            best = Some((a, b));
        }
    }
    best
}

// ---------------------------------------------------------------------------------- rendering

/// What `render` reads of a diagnostic (also from a host's JSON, for `formatDiagnostics`).
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub message: String,
    pub severity: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub spans: Vec<SubSpan>,
    #[serde(default)]
    pub helps: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub children: Vec<Child>,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl From<&Diagnostic> for View {
    fn from(d: &Diagnostic) -> View {
        View {
            message: d.message.clone(),
            severity: d.severity.to_string(),
            code: d.code.clone(),
            label: d.label.clone(),
            spans: d.spans.clone(),
            helps: d.helps.clone(),
            notes: d.notes.clone(),
            children: d.children.clone(),
            line: d.line,
            column: d.column,
            end_line: d.end_line,
            end_column: d.end_column,
        }
    }
}

/// An annotation of the snippet: 0-based lines, display columns (tabs are 4 wide), end exclusive.
#[derive(Clone, Debug)]
struct Ann {
    line: usize,
    col: usize,
    end_line: usize,
    end_col: usize,
    primary: bool,
    label: Option<String>,
}

/// The diagnostics as rustc prints them, then rustc's summary (`error: aborting due to ..`).
pub fn render(src: &str, filename: &str, diags: &[View]) -> String {
    let lines: Vec<&str> = src.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let mut out = String::new();
    for d in diags {
        render_one(&mut out, &lines, filename, d);
        out.push('\n');
    }
    let errors = diags.iter().filter(|d| d.severity == "error").count();
    let warnings = diags.iter().filter(|d| d.severity == "warning").count();
    let plural = |n: usize, w: &str| if n == 1 { format!("1 {w}") } else { format!("{n} {w}s") };
    if errors > 0 {
        out.push_str(&format!("error: aborting due to {}", plural(errors, "previous error")));
        if warnings > 0 {
            out.push_str(&format!("; {} emitted", plural(warnings, "warning")));
        }
        out.push_str("\n\n");
        let mut codes: Vec<&str> = diags
            .iter()
            .filter(|d| d.severity == "error")
            .filter_map(|d| d.code.as_deref())
            .collect();
        codes.sort();
        codes.dedup();
        match codes.as_slice() {
            [] => {}
            [one] => out.push_str(&format!(
                "For more information about this error, try `rustc --explain {one}`.\n"
            )),
            [first, ..] => out.push_str(&format!(
                "Some errors have detailed explanations: {}.\nFor more information about an error, try `rustc --explain {first}`.\n",
                codes.join(", ")
            )),
        }
    } else if warnings > 0 {
        out.push_str(&format!("warning: {} emitted\n\n", plural(warnings, "warning")));
    }
    out
}

/// A line's display columns: a 1-based UTF-16 column as a 0-based display column.
fn display_col(line: &str, col16: u32) -> usize {
    let mut units = 1u32;
    let mut cols = 0usize;
    for c in line.chars() {
        if units >= col16 {
            break;
        }
        units += c.len_utf16() as u32;
        cols += if c == '\t' { 4 } else { 1 };
    }
    // (Past the end of the line: one column per unit.)
    cols + col16.saturating_sub(units) as usize
}

/// The 1-based character column rustc prints in `-->`.
fn char_col(line: &str, col16: u32) -> usize {
    let mut units = 1u32;
    let mut chars = 1usize;
    for c in line.chars() {
        if units >= col16 {
            break;
        }
        units += c.len_utf16() as u32;
        chars += 1;
    }
    chars
}

fn expand_tabs(s: &str) -> String {
    s.replace('\t', "    ")
}

fn ann(lines: &[&str], line: u32, col: u32, end_line: u32, end_col: u32, primary: bool, label: Option<String>) -> Ann {
    let get = |l: u32| lines.get(l.saturating_sub(1) as usize).copied().unwrap_or("");
    let (l0, mut l1) = (line.max(1) as usize - 1, end_line.max(line).max(1) as usize - 1);
    let c0 = display_col(get(line), col);
    let mut c1 = display_col(get(end_line), end_col);
    // (A span ending at the start of a line ends with the line before.)
    if l1 > l0 && c1 == 0 {
        l1 -= 1;
        c1 = expand_tabs(lines.get(l1).copied().unwrap_or("")).chars().count();
    }
    if l1 == l0 && c1 <= c0 {
        c1 = c0 + 1;
    }
    Ann { line: l0, col: c0, end_line: l1, end_col: c1, primary, label }
}

fn render_one(out: &mut String, lines: &[&str], filename: &str, d: &View) {
    let sev = if d.severity == "error" { "error" } else { "warning" };
    match &d.code {
        Some(c) => out.push_str(&format!("{sev}[{c}]: {}\n", d.message)),
        None => out.push_str(&format!("{sev}: {}\n", d.message)),
    }
    let mut anns = vec![ann(lines, d.line, d.column, d.end_line, d.end_column, true, d.label.clone())];
    anns.extend(d.spans.iter().map(|s| sub_ann(lines, s)));
    // (Nothing to show from the source: a position past the file's end.)
    let shown: Vec<usize> = {
        let mut v: Vec<usize> = Vec::new();
        for a in &anns {
            v.push(a.line);
            v.push(a.end_line);
        }
        v.retain(|&l| l < lines.len());
        v.sort();
        v.dedup();
        v
    };
    // (The gutter fits every line number the diagnostic shows, its children's too.)
    let child_lines = d
        .children
        .iter()
        .flat_map(|c| c.spans.iter().chain(&c.edits))
        .map(|s| s.end_line.max(s.line) as usize);
    let width = shown
        .iter()
        .map(|l| l + 1)
        .chain(child_lines)
        .map(|l| l.to_string().len())
        .max()
        .unwrap_or(1);
    let pad = " ".repeat(width);
    let first_line = lines.get(d.line.saturating_sub(1) as usize).copied().unwrap_or("");
    out.push_str(&format!(
        "{pad}--> {filename}:{}:{}\n",
        d.line,
        char_col(first_line, d.column)
    ));
    if !shown.is_empty() {
        out.push_str(&format!("{pad} |\n"));
        snippet(out, lines, &anns, width);
    }
    if !d.helps.is_empty() || !d.notes.is_empty() || !d.children.is_empty() {
        out.push_str(&format!("{pad} |\n"));
        for h in &d.helps {
            sub(out, &pad, "help", h);
        }
        for n in &d.notes {
            sub(out, &pad, "note", n);
        }
    }
    for c in &d.children {
        if let Some(first) = c.spans.iter().find(|s| s.primary).or(c.spans.first()) {
            out.push_str(&format!("{}: {}\n", c.kind, c.message));
            let line = lines.get(first.line.saturating_sub(1) as usize).copied().unwrap_or("");
            out.push_str(&format!("{pad}--> {filename}:{}:{}\n", first.line, char_col(line, first.column)));
            out.push_str(&format!("{pad} |\n"));
            let anns: Vec<Ann> = c.spans.iter().map(|s| sub_ann(lines, s)).collect();
            snippet(out, lines, &anns, width);
        } else if !c.edits.is_empty() {
            out.push_str(&format!("{}: {}\n", c.kind, c.message));
            suggestion(out, lines, &c.edits, width);
        } else {
            sub(out, &pad, &c.kind, &c.message);
        }
    }
}

/// A secondary span's annotation (or another primary one's); an empty label is none.
fn sub_ann(lines: &[&str], s: &SubSpan) -> Ann {
    let label = (!s.label.is_empty()).then(|| s.label.clone());
    ann(lines, s.line, s.column, s.end_line, s.end_column, s.primary, label)
}

/// The byte offset of a 1-based UTF-16 column in `line`.
fn byte_col(line: &str, col16: u32) -> usize {
    let mut units = 1u32;
    for (i, c) in line.char_indices() {
        if units >= col16 {
            return i;
        }
        units += c.len_utf16() as u32;
    }
    line.len()
}

/// A suggestion as rustc shows it: the lines it changes as they would read, with `+++` under
/// what it inserts, or (when it removes or replaces text) the old lines marked `-` and the new
/// ones `+`. Each edit's label is its replacement text; edits don't contain line breaks.
fn suggestion(out: &mut String, lines: &[&str], edits: &[SubSpan], width: usize) {
    let pad = " ".repeat(width);
    out.push_str(&format!("{pad} |\n"));
    let first = edits.iter().map(|e| e.line).min().unwrap_or(1).max(1) as usize - 1;
    let last = edits.iter().map(|e| e.end_line).max().unwrap_or(1).max(1) as usize - 1;
    let last = last.min(lines.len().saturating_sub(1));
    // Each line's new text, and the inserted ranges (in display columns) on it.
    let mut new_lines: Vec<String> = Vec::new();
    let mut marks: Vec<Vec<(usize, usize)>> = Vec::new();
    let insert_only = edits.iter().all(|e| (e.line, e.column) == (e.end_line, e.end_column));
    for l in first..=last {
        let old = lines.get(l).copied().unwrap_or("");
        let mut here: Vec<(usize, usize, &str)> = edits
            .iter()
            .filter(|e| e.line as usize - 1 == l)
            .map(|e| {
                let end = if e.end_line as usize - 1 == l { byte_col(old, e.end_column) } else { old.len() };
                (byte_col(old, e.column), end, e.label.as_str())
            })
            .collect();
        here.sort();
        let mut s = String::new();
        let mut m = Vec::new();
        let mut at = 0;
        for (a, b, text) in here {
            s.push_str(&old[at..a.max(at)]);
            let col = expand_tabs(&s).chars().count();
            s.push_str(text);
            // (Under what's inserted, but not its trailing space: `let _ = ` is `+++++++`.)
            m.push((col, col + text.trim_end().chars().count()));
            at = b.max(at);
        }
        s.push_str(&old[at.min(old.len())..]);
        new_lines.push(s);
        marks.push(m);
    }
    if insert_only {
        for (i, s) in new_lines.iter().enumerate() {
            let row = format!("{:>width$} | {}", first + i + 1, expand_tabs(s));
            out.push_str(row.trim_end());
            out.push('\n');
            let mut cells: Vec<char> = Vec::new();
            for &(a, b) in &marks[i] {
                cells.resize(b.max(cells.len()), ' ');
                cells[a..b].fill('+');
            }
            if !cells.is_empty() {
                out.push_str(&format!("{pad} | {}\n", cells.iter().collect::<String>()));
            }
        }
    } else {
        for l in first..=last {
            let row = format!("{:>width$} - {}", l + 1, expand_tabs(lines.get(l).copied().unwrap_or("")));
            out.push_str(row.trim_end());
            out.push('\n');
        }
        for (i, s) in new_lines.iter().enumerate() {
            let row = format!("{:>width$} + {}", first + i + 1, expand_tabs(s));
            out.push_str(row.trim_end());
            out.push('\n');
        }
        out.push_str(&format!("{pad} |\n"));
    }
}

/// `  = help: text`, later lines lined up after the colon.
fn sub(out: &mut String, pad: &str, kind: &str, text: &str) {
    let mut it = text.lines();
    out.push_str(&format!("{pad} = {kind}: {}\n", it.next().unwrap_or("")));
    let indent = " ".repeat(pad.len() + 3 + kind.len() + 2);
    for l in it {
        out.push_str(&format!("{indent}{l}\n"));
    }
}

/// The source lines with their annotations, rustc's way.
fn snippet(out: &mut String, lines: &[&str], anns: &[Ann], width: usize) {
    // One multi-line span is drawn in a margin (`/` or `__^` where it starts, `|` along it,
    // `|__^ label` where it ends); any other span covering several lines is marked on its
    // first line only.
    let multi = anns.iter().position(|a| a.end_line > a.line);
    let mut singles: Vec<Ann> = Vec::new();
    for (i, a) in anns.iter().enumerate() {
        if a.end_line > a.line && Some(i) != multi {
            let len = expand_tabs(lines.get(a.line).copied().unwrap_or("")).chars().count();
            singles.push(Ann { end_line: a.line, end_col: len.max(a.col + 1), ..a.clone() });
        } else if a.end_line == a.line {
            singles.push(a.clone());
        }
    }
    let m = multi.map(|i| anns[i].clone());
    // (Whether the multi-line span starts at its line's first non-blank: drawn with `/`.)
    let starts_line = m.as_ref().is_some_and(|m| {
        expand_tabs(lines.get(m.line).copied().unwrap_or(""))
            .chars()
            .take(m.col)
            .all(char::is_whitespace)
    });
    // The lines to show: annotated ones, a long multi-line span's first three and last two,
    // and a single line between two shown ones.
    let mut show: Vec<usize> = singles.iter().map(|a| a.line).collect();
    if let Some(m) = &m {
        if m.end_line - m.line < 6 {
            show.extend(m.line..=m.end_line);
        } else {
            show.extend([m.line, m.line + 1, m.line + 2, m.end_line - 1, m.end_line]);
        }
    }
    show.retain(|&l| l < lines.len());
    show.sort();
    show.dedup();
    let mut filled = Vec::new();
    for (i, &l) in show.iter().enumerate() {
        if i > 0 && l == show[i - 1] + 2 {
            filled.push(l - 1);
        }
        filled.push(l);
    }
    let pad = " ".repeat(width);
    // The margin on line `l`'s rows (empty without a multi-line span).
    let margin = |l: usize, source_row: bool| -> &'static str {
        match &m {
            None => "",
            Some(m) if l == m.line && source_row && starts_line => "/ ",
            Some(m) if l > m.line && l <= m.end_line => "| ",
            Some(m) if l == m.line && starts_line => "| ",
            Some(_) => "  ",
        }
    };
    let mut prev: Option<usize> = None;
    for &l in &filled {
        if let Some(p) = prev
            && l > p + 1
        {
            let inside = m.as_ref().is_some_and(|m| p >= m.line && l <= m.end_line);
            if inside {
                out.push_str(&format!("...{pad}|\n"));
            } else {
                out.push_str("...\n");
            }
        }
        prev = Some(l);
        let row = format!("{:>width$} | {}{}", l + 1, margin(l, true), expand_tabs(lines[l]));
        out.push_str(row.trim_end());
        out.push('\n');
        let here: Vec<&Ann> = singles.iter().filter(|a| a.line == l).collect();
        if let Some(m) = &m
            && l == m.line
            && !starts_line
        {
            let ch = if m.primary { '^' } else { '-' };
            out.push_str(&format!("{pad} |  {}{ch}\n", "_".repeat(m.col + 1)));
        }
        if !here.is_empty() {
            annotate(out, &format!("{pad} | {}", margin(l, false)), &here);
        }
        if let Some(m) = &m
            && l == m.end_line
        {
            let ch = if m.primary { '^' } else { '-' };
            let mut row = format!("{pad} | |{}{ch}", "_".repeat(m.end_col.max(1)));
            if let Some(label) = &m.label {
                row.push(' ');
                row.push_str(label);
            }
            out.push_str(&row);
            out.push('\n');
        }
    }
}

/// The underline row of a line's annotations and their labels: the rightmost label inline,
/// the others below it on their own rows, joined to their spans by `|`. `prefix` is the gutter
/// (and margin) each row starts with.
fn annotate(out: &mut String, prefix: &str, here: &[&Ann]) {
    let mut anns: Vec<&Ann> = here.to_vec();
    anns.sort_by_key(|a| (a.col, a.end_col));
    let row = |cells: &[char]| -> String {
        let s: String = cells.iter().collect();
        format!("{prefix}{s}").trim_end().to_string()
    };
    let width = anns.iter().map(|a| a.end_col).max().unwrap_or(0);
    let mut cells = vec![' '; width];
    // Drawn right to left (the primary first among those starting together), so the leftmost
    // wins where they overlap, as rustc draws them (`----^` under `mut x`, the `mut ` over it).
    let mut draw: Vec<&Ann> = here.to_vec();
    draw.sort_by_key(|a| (std::cmp::Reverse(a.col), !a.primary));
    for a in draw {
        cells[a.col..a.end_col].fill(if a.primary { '^' } else { '-' });
    }
    let mut first = row(&cells);
    let mut rest: Vec<&Ann> = anns.iter().copied().filter(|a| a.label.is_some()).collect();
    // The rightmost annotation's label goes on the underline row.
    if let Some(&last) = anns.last()
        && let Some(label) = &last.label
    {
        first.push(' ');
        first.push_str(label);
        rest.retain(|a| !std::ptr::eq(*a, last));
    }
    out.push_str(&first);
    out.push('\n');
    if rest.is_empty() {
        return;
    }
    let bars = |upto: &[&Ann]| -> Vec<char> {
        let w = upto.iter().map(|a| a.col + 1).max().unwrap_or(0);
        let mut cells = vec![' '; w];
        for a in upto {
            cells[a.col] = '|';
        }
        cells
    };
    out.push_str(&row(&bars(&rest)));
    out.push('\n');
    while let Some(a) = rest.pop() {
        let mut cells = bars(&rest);
        cells.resize(a.col, ' ');
        let s: String = cells.iter().collect();
        out.push_str(&format!("{prefix}{s}{}\n", a.label.as_deref().unwrap_or("")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(message: &str, line: u32, col: u32, end_col: u32) -> View {
        View {
            message: message.into(),
            severity: "error".into(),
            code: None,
            label: None,
            spans: Vec::new(),
            helps: Vec::new(),
            notes: Vec::new(),
            children: Vec::new(),
            line,
            column: col,
            end_line: line,
            end_column: end_col,
        }
    }

    #[test]
    fn splits_trailing_hints() {
        assert_eq!(split_hint("cannot borrow `v` (declare it with `let mut v`)"), ("cannot borrow `v`", Some("declare it with `let mut v`")));
        assert_eq!(split_hint("no hint here"), ("no hint here", None));
        assert_eq!(split_hint("the trait `Fn(i32)` (x)").1, Some("x"));
        assert_eq!(split_hint("call `f()`").1, None);
    }

    #[test]
    fn moves_implementation_talk_to_the_editor_help() {
        assert_eq!(
            plain("mutable string slices (`&mut s[a..b]`) are not supported (strings are values in JS); work on `s` itself"),
            ("mutable string slices (`&mut s[a..b]`) are not supported; work on `s` itself".to_string(), "Strings are values in JS.".to_string())
        );
        assert_eq!(
            plain("no method named `x` found for `T` (the std surface is still small; see docs/FEATURES.md)"),
            ("no method named `x` found for `T`".to_string(), "The std surface is still small; see docs/FEATURES.md.".to_string())
        );
        assert_eq!(
            plain("`Rc::f` isn't supported: JS values have no reference counts (the garbage collector frees them)").0,
            "`Rc::f` isn't supported"
        );
        assert_eq!(plain("`let` chains are not supported yet").0, "`let` chains are not supported");
        assert_eq!(plain("expected `JSON`: a `serde_json::Value`").1, "");
    }

    #[test]
    fn renders_like_rustc() {
        let src = "fn main() {\n    let x: i32 = \"hello\";\n}\n";
        let mut v = d("mismatched types", 2, 18, 25);
        v.code = Some("E0308".into());
        v.label = Some("expected `i32`, found `&str`".into());
        v.spans.push(SubSpan { start: 0, end: 0, line: 2, column: 12, end_line: 2, end_column: 15, label: "expected due to this".into(), primary: false });
        let want = "error[E0308]: mismatched types\n --> e.rs:2:18\n  |\n2 |     let x: i32 = \"hello\";\n  |            ---   ^^^^^^^ expected `i32`, found `&str`\n  |            |\n  |            expected due to this\n\nerror: aborting due to 1 previous error\n\nFor more information about this error, try `rustc --explain E0308`.\n";
        assert_eq!(render(src, "e.rs", &[v]), want);
    }

    #[test]
    fn renders_three_labels_on_a_line() {
        let src = "fn main() {\n    let a = \"x\";\n    let b = \"y\";\n    let c = a + b;\n}\n";
        let mut v = d("cannot add `&str` to `&str`", 4, 15, 16);
        v.code = Some("E0369".into());
        v.label = Some("`+` cannot be used to concatenate two `&str` strings".into());
        for (c, l) in [(13, "&str"), (17, "&str")] {
            v.spans.push(SubSpan { start: 0, end: 0, line: 4, column: c, end_line: 4, end_column: c + 1, label: l.into(), primary: false });
        }
        v.notes.push("string concatenation requires an owned `String` on the left".into());
        let got = render(src, "e.rs", &[v]);
        let want = "error[E0369]: cannot add `&str` to `&str`\n --> e.rs:4:15\n  |\n4 |     let c = a + b;\n  |             - ^ - &str\n  |             | |\n  |             | `+` cannot be used to concatenate two `&str` strings\n  |             &str\n  |\n  = note: string concatenation requires an owned `String` on the left\n\n";
        assert!(got.starts_with(want), "{got}");
    }

    #[test]
    fn shows_a_line_between_and_elides_more() {
        let src = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n    x = 2;\n}\n";
        let mut v = d("cannot assign twice to immutable variable `x`", 4, 5, 10);
        v.code = Some("E0384".into());
        v.label = Some("cannot assign twice to immutable variable".into());
        v.spans.push(SubSpan { start: 0, end: 0, line: 2, column: 9, end_line: 2, end_column: 10, label: "first assignment to `x`".into(), primary: false });
        v.helps.push("consider making this binding mutable".into());
        let got = render(src, "e.rs", &[v]);
        let want = "error[E0384]: cannot assign twice to immutable variable `x`\n --> e.rs:4:5\n  |\n2 |     let x = 1;\n  |         - first assignment to `x`\n3 |     println!(\"{}\", x);\n4 |     x = 2;\n  |     ^^^^^ cannot assign twice to immutable variable\n  |\n  = help: consider making this binding mutable\n\n";
        assert!(got.starts_with(want), "{got}");
    }

    #[test]
    fn summarizes_errors_and_warnings() {
        let src = "fn main() {}\n";
        let mut w = d("unused", 1, 1, 3);
        w.severity = "warning".into();
        let mut e = d("bad", 1, 4, 8);
        e.code = Some("E0425".into());
        let mut e2 = d("worse", 1, 4, 8);
        e2.code = Some("E0308".into());
        let got = render(src, "e.rs", &[w.clone(), e, e2]);
        assert!(got.ends_with("error: aborting due to 2 previous errors; 1 warning emitted\n\nSome errors have detailed explanations: E0308, E0425.\nFor more information about an error, try `rustc --explain E0308`.\n"), "{got}");
        assert!(render(src, "e.rs", &[w]).ends_with("warning: 1 warning emitted\n\n"));
        assert_eq!(render(src, "e.rs", &[]), "");
    }
}
