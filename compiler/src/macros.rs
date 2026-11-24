//! Built-in macro argument handling, shared by the type checker and codegen.
//!
//! rust-analyzer leaves macro arguments as an unparsed token tree. Both passes split it at
//! top-level commas and re-parse each argument from its source text, so the positions of the
//! re-parsed nodes (offset by the argument's absolute start) agree between the two passes.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, AstToken, Edition, NodeOrToken, SyntaxElement, SyntaxKind, SyntaxToken, T, ast,
};

pub const EDITION: Edition = Edition::Edition2021;

thread_local! {
    /// The last few whole-file parses, by their text (most recent last): the preprocessing
    /// passes, their gates, the type checker and codegen each parse the same text.
    static PARSES: std::cell::RefCell<Vec<(String, ra_ap_syntax::Parse<ast::SourceFile>, ast::SourceFile)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// How many parses `PARSES` keeps.
const PARSES_KEPT: usize = 8;

/// `SourceFile::parse(text, EDITION)`, from the parses kept when `text` was parsed recently
/// (the tree is then the same one, so caches keyed by its root hit too).
pub(crate) fn parse_file(text: &str) -> ra_ap_syntax::Parse<ast::SourceFile> {
    PARSES.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(i) = p.iter().position(|(t, _, _)| t == text) {
            if i + 1 != p.len() {
                let e = p.remove(i);
                p.push(e);
            }
            return p.last().unwrap().1.clone();
        }
        let parse = ra_ap_syntax::SourceFile::parse(text, EDITION);
        if p.len() >= PARSES_KEPT {
            p.remove(0);
        }
        p.push((text.to_string(), parse.clone(), parse.tree()));
        parse
    })
}

/// `SourceFile::parse(text, EDITION).tree()`, from the parses kept (see `parse_file`).
pub(crate) fn parse_tree(text: &str) -> ast::SourceFile {
    PARSES.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(i) = p.iter().position(|(t, _, _)| t == text) {
            if i + 1 != p.len() {
                let e = p.remove(i);
                p.push(e);
            }
            return p.last().unwrap().2.clone();
        }
        let parse = ra_ap_syntax::SourceFile::parse(text, EDITION);
        let tree = parse.tree();
        if p.len() >= PARSES_KEPT {
            p.remove(0);
        }
        p.push((text.to_string(), parse, tree.clone()));
        tree
    })
}

pub struct MacroArg {
    /// Absolute byte offset of the argument text in the original source.
    pub start: usize,
    pub text: String,
    /// `name` for a `name = value` argument (format macros).
    pub name: Option<String>,
    /// The argument's only token, when it is exactly one token (used for format strings).
    pub single_token: Option<SyntaxToken>,
}

impl MacroArg {
    pub fn end(&self) -> usize {
        self.start + self.text.len()
    }
}

pub fn macro_name(mc: &ast::MacroCall) -> Option<String> {
    mc.path()
        .and_then(|p| p.segment())
        .and_then(|s| s.name_ref())
        .map(|n| n.text().to_string())
}

/// Non-trivia elements between the token tree's delimiters.
fn inner_elements(mc: &ast::MacroCall) -> Vec<SyntaxElement> {
    let Some(tt) = mc.token_tree() else {
        return vec![];
    };
    let elems: Vec<_> = tt.syntax().children_with_tokens().collect();
    if elems.len() < 2 {
        return vec![];
    }
    elems[1..elems.len() - 1]
        .iter()
        .filter(|el| !el.kind().is_trivia())
        .cloned()
        .collect()
}

fn to_arg(src: &str, base: usize, mut group: Vec<SyntaxElement>) -> Option<MacroArg> {
    if group.is_empty() {
        return None;
    }
    let mut name = None;
    // `name = value`, but not `a == b`: inside a token tree `==` is two separate `=` tokens.
    if group.len() >= 3
        && group[0].kind() == SyntaxKind::IDENT
        && group[1].kind() == T![=]
        && group[2].kind() != T![=]
    {
        name = group[0].as_token().map(|t| t.text().to_string());
        group.drain(0..2);
    }
    let start = base + usize::from(group[0].text_range().start());
    let end = base + usize::from(group.last()?.text_range().end());
    let single_token = match group.as_slice() {
        [NodeOrToken::Token(t)] => Some(t.clone()),
        // A string literal from a macro's `$x:expr` comes wrapped in parens (`debug!("{}", x)`
        // forwarding `$($args:expr),*` to `println!`); rustc takes it as the format string.
        [NodeOrToken::Node(n)] if n.kind() == SyntaxKind::TOKEN_TREE => {
            let inner: Vec<_> = n
                .children_with_tokens()
                .filter(|el| !el.kind().is_trivia())
                .collect();
            match inner.as_slice() {
                [open, NodeOrToken::Token(t), close]
                    if open.kind() == T!['('] && close.kind() == T![')'] && t.kind() == SyntaxKind::STRING =>
                {
                    Some(t.clone())
                }
                _ => None,
            }
        }
        _ => None,
    };
    Some(MacroArg {
        start,
        text: src.get(start..end)?.to_string(),
        name,
        single_token,
    })
}

/// Split `m!(a, b, name = c)` into its arguments. `base` is the absolute offset of the AST the
/// macro call belongs to (non-zero when the call itself sits inside a re-parsed argument).
pub fn split_args(src: &str, base: usize, mc: &ast::MacroCall) -> Vec<MacroArg> {
    let mut groups: Vec<Vec<SyntaxElement>> = vec![vec![]];
    // Commas inside a turbofish (`Pair::<i32, u8>::new()`) don't split: `<` right after `::`
    // opens generic arguments (elsewhere it may be a comparison).
    let mut angle = 0usize;
    let mut prev: Option<SyntaxKind> = None;
    let mut prev2: Option<SyntaxKind> = None;
    let mut closure_params = false;
    for el in inner_elements(mc) {
        let k = el.kind();
        let after_path_sep =
            prev == Some(T![:]) && prev2 == Some(T![:]) || prev == Some(SyntaxKind::COLON2);
        if k == T![<] && (after_path_sep || angle > 0) {
            angle += 1;
        } else if k == T![>] && angle > 0 && prev != Some(T![-]) && prev != Some(T![=]) {
            angle -= 1;
        }
        // A closure's parameters (`vec![|a, b| a + b]`): `|` opening an argument, up to the
        // next `|`.
        if k == T![|] {
            if closure_params {
                closure_params = false;
            } else {
                let opening = groups
                    .last()
                    .unwrap()
                    .iter()
                    .all(|e| e.kind().is_trivia() || matches!(e.kind(), T![move] | T![async]));
                closure_params = opening;
            }
        }
        prev2 = prev;
        prev = Some(k);
        if k == T![,] && angle == 0 && !closure_params {
            groups.push(vec![]);
        } else {
            groups.last_mut().unwrap().push(el);
        }
    }
    groups
        .into_iter()
        .filter_map(|g| to_arg(src, base, g))
        .collect()
}

/// For `vec![value; count]`: the two halves around the top-level `;`.
pub fn split_repeat(src: &str, base: usize, mc: &ast::MacroCall) -> Option<(MacroArg, MacroArg)> {
    let elems = inner_elements(mc);
    let semi = elems.iter().position(|el| el.kind() == T![;])?;
    let value = to_arg(src, base, elems[..semi].to_vec())?;
    let count = to_arg(src, base, elems[semi + 1..].to_vec())?;
    Some((value, count))
}

/// `matches!(x, P if g)` as the text of `match x { P if g => true, _ => false }`, laid out so `x`
/// and `P` keep their source offsets (diagnostics and source maps stay right).
pub fn matches_as_match(args: &[MacroArg]) -> Option<MacroArg> {
    let [x, p] = args else { return None };
    let gap = p.start.checked_sub(x.end())?;
    if gap == 0 || x.start < 6 {
        return None;
    }
    let text = format!(
        "match {}{}{{{} => true, _ => false }}",
        x.text,
        " ".repeat(gap - 1),
        p.text
    );
    Some(MacroArg {
        start: x.start - 6,
        text,
        name: None,
        single_token: None,
    })
}

/// Parse a macro argument as an expression. Errors carry absolute byte ranges.
pub fn parse_arg(arg: &MacroArg) -> (Option<ast::Expr>, Vec<(usize, usize, String)>) {
    let parse = ast::Expr::parse(&arg.text, EDITION);
    let mut errors: Vec<_> = parse
        .errors()
        .iter()
        .map(|e| {
            let r = e.range();
            (
                arg.start + usize::from(r.start()),
                arg.start + usize::from(r.end()),
                e.to_string(),
            )
        })
        .collect();
    // `Parse::tree()` panics when the root is an ERROR node (the argument isn't an expression).
    let expr = ast::Expr::cast(parse.syntax_node());
    if expr.is_none() && errors.is_empty() {
        errors.push((arg.start, arg.end(), "expected an expression".into()));
    }
    (expr, errors)
}

/// The value of a format-string argument, if it is a plain string literal.
pub fn format_string(arg: &MacroArg) -> Option<String> {
    let token = ast::String::cast(arg.single_token.clone()?)?;
    crate::lineindex::string_value(&token)
}

/// A piece of a `quote! { .. }` template.
#[derive(Debug, Clone)]
pub enum QuotePart {
    /// Tokens written out as they are (with the source's spacing, one space for any gap).
    Text(String),
    /// `#name`: the variable's tokens; its name's absolute range.
    Var {
        name: String,
        start: usize,
        end: usize,
    },
    /// `#( .. ) sep *`: the inside once per item of the iterables it uses; `at` is the absolute
    /// offset of its `#`.
    Rep {
        parts: Vec<QuotePart>,
        sep: String,
        at: usize,
    },
}

/// The template of a `quote!` call. `base` is the absolute offset of the macro call's text.
pub fn quote_parts(mc: &ast::MacroCall, base: usize) -> Vec<QuotePart> {
    let Some(tt) = mc.token_tree() else {
        return vec![];
    };
    let elems: Vec<SyntaxElement> = tt.syntax().children_with_tokens().collect();
    let mut out = Vec::new();
    if elems.len() >= 2 {
        let mut gap = false;
        quote_walk(&elems[1..elems.len() - 1], base, &mut out, &mut gap);
    }
    out
}

fn push_text(out: &mut Vec<QuotePart>, gap: &mut bool, text: &str) {
    if let Some(QuotePart::Text(t)) = out.last_mut() {
        if *gap && !t.is_empty() {
            t.push(' ');
        }
        t.push_str(text);
    } else {
        let lead = if *gap && !out.is_empty() { " " } else { "" };
        out.push(QuotePart::Text(format!("{lead}{text}")));
    }
    *gap = false;
}

fn quote_walk(elems: &[SyntaxElement], base: usize, out: &mut Vec<QuotePart>, gap: &mut bool) {
    let mut i = 0;
    while i < elems.len() {
        let el = &elems[i];
        if el.kind().is_trivia() {
            *gap = true;
            i += 1;
            continue;
        }
        // The next non-trivia element after `i`.
        let next = |from: usize| (from + 1..elems.len()).find(|&j| !elems[j].kind().is_trivia());
        if el.kind() == T![#]
            && let Some(j) = next(i)
        {
            match &elems[j] {
                NodeOrToken::Token(t) if t.kind() == SyntaxKind::IDENT => {
                    if *gap {
                        push_text(out, gap, "");
                    }
                    let r = t.text_range();
                    out.push(QuotePart::Var {
                        name: t.text().to_string(),
                        start: base + usize::from(r.start()),
                        end: base + usize::from(r.end()),
                    });
                    i = j + 1;
                    continue;
                }
                NodeOrToken::Node(n)
                    if n.kind() == SyntaxKind::TOKEN_TREE
                        && n.first_token().is_some_and(|t| t.kind() == T!['(']) =>
                {
                    // `#( .. ) sep *`: the separator runs up to the `*`.
                    let star = (j + 1..elems.len()).find(|&k| elems[k].kind() == T![*]);
                    if let Some(star) = star {
                        let inner: Vec<SyntaxElement> = n.children_with_tokens().collect();
                        let mut parts = Vec::new();
                        let mut inner_gap = false;
                        if inner.len() >= 2 {
                            quote_walk(
                                &inner[1..inner.len() - 1],
                                base,
                                &mut parts,
                                &mut inner_gap,
                            );
                        }
                        let sep: String = elems[j + 1..star]
                            .iter()
                            .filter(|e| !e.kind().is_trivia())
                            .map(|e| e.to_string())
                            .collect();
                        if *gap {
                            push_text(out, gap, "");
                        }
                        let at = base + usize::from(el.text_range().start());
                        out.push(QuotePart::Rep { parts, sep, at });
                        i = star + 1;
                        continue;
                    }
                }
                _ => {}
            }
        }
        match el {
            NodeOrToken::Token(t) => push_text(out, gap, t.text()),
            NodeOrToken::Node(n) => {
                // A group: its delimiters around its tokens.
                let inner: Vec<SyntaxElement> = n.children_with_tokens().collect();
                if inner.len() >= 2 {
                    push_text(out, gap, &inner[0].to_string());
                    quote_walk(&inner[1..inner.len() - 1], base, out, gap);
                    push_text(out, gap, &inner[inner.len() - 1].to_string());
                } else {
                    push_text(out, gap, &n.to_string());
                }
            }
        }
        i += 1;
    }
}

/// What `stringify!(..)` gives: the tokens as rustc prints them. Tokens written together stay
/// together (`a+b`); between tokens written apart there's one space, except before `,` `;` `.`,
/// after `.`, between a name and its `(..)`, and between `#` and `[..]`. `{ .. }` gets spaces
/// inside.
pub fn stringify(mc: &ast::MacroCall) -> String {
    match mc.token_tree() {
        Some(tt) => tts_text(&tt),
        None => String::new(),
    }
}

/// A token tree's inner tokens.
fn tts_text(tt: &ast::TokenTree) -> String {
    #[derive(PartialEq)]
    enum Kind {
        Punct(String),
        /// A name that may take `(..)` without a space: a non-keyword, `fn`, `Self`, `pub`.
        Callee,
        Other,
        Group(char),
    }
    let elems: Vec<SyntaxElement> = tt.syntax().children_with_tokens().collect();
    // (Skip the delimiters.)
    let inner = &elems[1..elems.len().saturating_sub(1).max(1)];
    let mut out = String::new();
    let mut prev: Option<Kind> = None;
    let mut gap = false;
    for e in inner {
        let (text, kind) = match e {
            NodeOrToken::Token(t)
                if t.kind() == SyntaxKind::WHITESPACE || t.kind() == SyntaxKind::COMMENT =>
            {
                gap = true;
                continue;
            }
            NodeOrToken::Token(t) => {
                let k = t.kind();
                let kind = if k.is_punct() {
                    Kind::Punct(t.text().to_string())
                } else if k == SyntaxKind::IDENT || matches!(t.text(), "fn" | "Self" | "pub") {
                    Kind::Callee
                } else {
                    Kind::Other
                };
                (t.text().to_string(), kind)
            }
            NodeOrToken::Node(n) => {
                let Some(sub) = ast::TokenTree::cast(n.clone()) else {
                    continue;
                };
                let open = sub
                    .syntax()
                    .first_token()
                    .map(|t| t.text().chars().next().unwrap_or('('))
                    .unwrap_or('(');
                let body = tts_text(&sub);
                let text = match open {
                    '{' if body.is_empty() => "{}".to_string(),
                    '{' => format!("{{ {body} }}"),
                    '[' => format!("[{body}]"),
                    _ => format!("({body})"),
                };
                (text, Kind::Group(open))
            }
        };
        if let Some(p) = &prev
            && gap
        {
            let is_punct = |k: &Kind| matches!(k, Kind::Punct(_));
            let space = match (p, &kind) {
                (Kind::Punct(d), k) if d == "." && !is_punct(k) => false,
                (Kind::Punct(d), Kind::Callee) if d == "$" => false,
                (p, Kind::Punct(c)) if !is_punct(p) && matches!(c.as_str(), "," | ";" | ".") => {
                    false
                }
                (Kind::Callee, Kind::Group('(')) => false,
                (Kind::Punct(h), Kind::Group('[')) if h == "#" => false,
                _ => true,
            };
            if space {
                out.push(' ');
            }
        }
        out.push_str(&text);
        prev = Some(kind);
        gap = false;
    }
    out
}

/// `cfg!(..)`'s predicate: `debug_assertions` (on in debug builds), `test` and `feature = ".."`
/// (off), the target's (see `target_cfg`), and `not` / `all` / `any` of those. Other names are
/// off, as in rustc.
pub fn eval_cfg(mc: &ast::MacroCall, debug: bool) -> Result<bool, String> {
    let tt = mc.token_tree().ok_or("`cfg!` needs a predicate")?;
    let toks: Vec<String> = tt
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE && t.kind() != SyntaxKind::COMMENT)
        .map(|t| t.text().to_string())
        .collect();
    // (Without the outer parentheses.)
    let toks = &toks[1..toks.len().saturating_sub(1).max(1)];
    let mut i = 0;
    let v = cfg_pred(toks, &mut i, debug)?;
    if i != toks.len() {
        return Err("`cfg!` takes one predicate".into());
    }
    Ok(v)
}

/// Whether `node` sits under a `#[cfg(..)]` (on it or an enclosing item, statement or
/// expression) that's off for the script's target (`#[cfg(not(any(unix, windows)))]
/// compile_error!(..)` fires, as it would on wasm), or malformed.
pub fn under_false_cfg(node: &ra_ap_syntax::SyntaxNode) -> bool {
    node.ancestors().any(|a| {
        a.children()
            .filter_map(ast::Attr::cast)
            .any(|attr| match attr_cfg_tokens(&attr) {
                Some(toks) => {
                    let mut i = 0;
                    !matches!(cfg_pred(&toks, &mut i, false), Ok(true) if i == toks.len())
                }
                None => false,
            })
    })
}

/// The predicate tokens of a `#[cfg(..)]` attribute.
fn attr_cfg_tokens(attr: &ast::Attr) -> Option<Vec<String>> {
    let toks: Vec<String> = attr
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != SyntaxKind::WHITESPACE && t.kind() != SyntaxKind::COMMENT)
        .map(|t| t.text().to_string())
        .filter(|t| t != "!")
        .collect();
    let n = toks.len();
    (n >= 6 && toks[..4] == ["#", "[", "cfg", "("] && toks[n - 2..] == [")", "]"])
        .then(|| toks[4..n - 2].to_vec())
}

fn cfg_pred(toks: &[String], i: &mut usize, debug: bool) -> Result<bool, String> {
    Ok(cfg_pred3(toks, i, debug, false)?.unwrap_or(false))
}

/// A target cfg of the one target scripts compile for: JS, described as rustc's
/// `wasm32-unknown-unknown` (not `unix` / `windows`, little-endian, no target features), except
/// that `usize` is 64 bits here (`usize::MAX` is `u64::MAX`), so `target_pointer_width = "64"`, and
/// that panics unwind (`panic = "unwind"`).
/// `None` for a name that isn't about the target.
fn target_cfg(name: &str, value: Option<&str>) -> Option<bool> {
    let v = value.map(|v| v.trim_matches('"'));
    Some(match (name, v) {
        ("unix" | "windows" | "miri" | "doc" | "docsrs" | "proc_macro", None) => false,
        ("target_family", Some(v)) => v == "wasm",
        ("target_arch", Some(v)) => v == "wasm32",
        ("target_os", Some(v)) => v == "unknown",
        ("target_vendor", Some(v)) => v == "unknown",
        ("target_env" | "target_abi", Some(v)) => v.is_empty(),
        ("target_endian", Some(v)) => v == "little",
        ("target_pointer_width", Some(v)) => v == "64",
        ("target_has_atomic", Some(_)) => true,
        ("target_feature", Some(_)) => false,
        // (A panic throws, and `Drop`s and `catch_unwind` see it go by.)
        ("panic", Some(v)) => v == "unwind",
        _ => return None,
    })
}

/// `cfg_pred`, three-valued: with `features_unknown`, `feature = ".."` is neither on nor off
/// (`None`), so an item under it is kept.
fn cfg_pred3(
    toks: &[String],
    i: &mut usize,
    debug: bool,
    features_unknown: bool,
) -> Result<Option<bool>, String> {
    let features = |_: &str| if features_unknown { None } else { Some(false) };
    cfg_eval(toks, i, debug, &features)
}

/// The predicate at `toks[*i..]`, with each `feature = "x"` answered by `features("\"x\"")`
/// (`None`: undecided, which `not` / `all` / `any` carry through as a third value).
fn cfg_eval(
    toks: &[String],
    i: &mut usize,
    debug: bool,
    features: &dyn Fn(&str) -> Option<bool>,
) -> Result<Option<bool>, String> {
    let name = toks.get(*i).ok_or("`cfg!` needs a predicate")?.clone();
    *i += 1;
    match name.as_str() {
        "not" | "all" | "any" => {
            if toks.get(*i).map(String::as_str) != Some("(") {
                return Err(format!("`{name}` takes a list: `{name}(..)`"));
            }
            *i += 1;
            let mut vals = Vec::new();
            while toks.get(*i).map(String::as_str) != Some(")") {
                vals.push(cfg_eval(toks, i, debug, features)?);
                if toks.get(*i).map(String::as_str) == Some(",") {
                    *i += 1;
                }
                if *i >= toks.len() {
                    return Err("unclosed `(` in `cfg!`".into());
                }
            }
            *i += 1;
            let (on, unknown) = (vals.contains(&Some(true)), vals.contains(&None));
            match name.as_str() {
                "not" if vals.len() == 1 => Ok(vals[0].map(|v| !v)),
                "not" => Err("`not(..)` takes one predicate".into()),
                "all" if vals.contains(&Some(false)) => Ok(Some(false)),
                "all" => Ok((!unknown).then_some(true)),
                _ if on => Ok(Some(true)),
                _ => Ok((!unknown).then_some(false)),
            }
        }
        _ => {
            let value = if toks.get(*i).map(String::as_str) == Some("=") {
                *i += 2;
                toks.get(*i - 1).cloned()
            } else {
                None
            };
            if let Some(v) = target_cfg(&name, value.as_deref()) {
                return Ok(Some(v));
            }
            match (name.as_str(), value) {
                ("debug_assertions", None) => Ok(Some(debug)),
                ("feature", Some(v)) => Ok(features(&v)),
                // `test`, and names nothing sets (`loom`, `fuzzing`, ..) are off.
                _ => Ok(Some(false)),
            }
        }
    }
}

/// The `feature = ".."` values a predicate mentions (with their quotes), added to `out`.
fn cfg_features(toks: &[String], out: &mut Vec<String>) {
    for w in toks.windows(3) {
        if w[0] == "feature" && w[1] == "=" && !out.contains(&w[2]) {
            out.push(w[2].clone());
        }
    }
}

/// Whether two predicates are each other's negation however the features are set, so that
/// exactly one of the two items they guard is ever compiled: `feature = "x"` and
/// `not(feature = "x")`, or `all(feature = "std", not(feature = "ffi"))` and
/// `any(not(feature = "std"), feature = "ffi")`.
fn cfg_complementary(a: &[String], b: &[String], debug: bool) -> bool {
    let mut feats = Vec::new();
    cfg_features(a, &mut feats);
    cfg_features(b, &mut feats);
    if feats.len() > 8 {
        return false;
    }
    (0..1u32 << feats.len()).all(|bits| {
        let features = |f: &str| {
            let k = feats.iter().position(|x| x == f)?;
            Some(bits >> k & 1 == 1)
        };
        let eval = |t: &[String]| {
            let mut i = 0;
            match cfg_eval(t, &mut i, debug, &features) {
                Ok(Some(v)) if i == t.len() => Some(v),
                _ => None,
            }
        };
        matches!((eval(a), eval(b)), (Some(x), Some(y)) if x != y)
    })
}

/// How a node's own `#[cfg(..)]`s come out for a script, with features undecided.
#[derive(Clone, Copy, PartialEq)]
enum CfgState {
    /// No `#[cfg]` at all.
    Plain,
    On,
    Off,
    /// Depends on a feature.
    Undecided,
}

/// A node's `#[cfg]` state, whether it's on when every feature is off, and the tokens of its
/// `#[cfg]` when it has exactly one.
fn node_cfg(node: &ra_ap_syntax::SyntaxNode, debug: bool) -> (CfgState, bool, Option<Vec<String>>) {
    let mut state = CfgState::Plain;
    let mut all_off = true;
    let mut cfgs = Vec::new();
    for attr in node.children().filter_map(ast::Attr::cast) {
        if attr.excl_token().is_some() {
            continue;
        }
        let Some(toks) = attr_cfg_tokens(&attr) else {
            continue;
        };
        let mut i = 0;
        let v = match cfg_pred3(&toks, &mut i, debug, true) {
            Ok(v) if i == toks.len() => v,
            // Malformed: kept, as `strip_cfg` keeps it.
            _ => Some(true),
        };
        let mut j = 0;
        all_off &=
            matches!(cfg_pred3(&toks, &mut j, debug, false), Ok(Some(true)) if j == toks.len());
        state = match (state, v) {
            (CfgState::Off, _) | (_, Some(false)) => CfgState::Off,
            (CfgState::Undecided, _) | (_, None) => CfgState::Undecided,
            _ => CfgState::On,
        };
        cfgs.push(toks);
    }
    let one = (cfgs.len() == 1).then(|| cfgs.remove(0));
    (state, all_off, one)
}

/// What a node would collide with if a sibling of the same key were kept too: a named item
/// (in its namespace), a field, a variant, or an `impl` with the same header (those collide
/// only when both define an associated item of one name, see `assoc_names`).
fn cfg_key(node: &ra_ap_syntax::SyntaxNode) -> Option<String> {
    use ast::HasName;
    fn named(space: &str, n: Option<ast::Name>) -> Option<String> {
        n.map(|n| format!("{space} {}", n.text()))
    }
    if let Some(item) = ast::Item::cast(node.clone()) {
        return match item {
            ast::Item::Fn(x) => named("value", x.name()),
            ast::Item::Const(x) => named("value", x.name()),
            ast::Item::Static(x) => named("value", x.name()),
            ast::Item::Struct(x) => named("type", x.name()),
            ast::Item::Enum(x) => named("type", x.name()),
            ast::Item::Union(x) => named("type", x.name()),
            ast::Item::TypeAlias(x) => named("type", x.name()),
            ast::Item::Trait(x) => named("type", x.name()),
            ast::Item::Module(x) => named("mod", x.name()),
            ast::Item::MacroRules(x) => named("macro", x.name()),
            ast::Item::MacroDef(x) => named("macro", x.name()),
            ast::Item::Impl(x) => {
                // The header, without attributes, comments and whitespace.
                let header: String = x
                    .syntax()
                    .children_with_tokens()
                    .filter(|e| {
                        !matches!(
                            e.kind(),
                            SyntaxKind::ATTR
                                | SyntaxKind::ASSOC_ITEM_LIST
                                | SyntaxKind::COMMENT
                                | SyntaxKind::WHITESPACE
                        )
                    })
                    .map(|e| e.to_string())
                    .collect::<String>()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                Some(format!("impl {header}"))
            }
            _ => None,
        };
    }
    if let Some(f) = ast::RecordField::cast(node.clone()) {
        return named("field", f.name());
    }
    if let Some(v) = ast::Variant::cast(node.clone()) {
        return named("variant", v.name());
    }
    None
}

/// The names an `impl` block defines.
fn assoc_names(node: &ra_ap_syntax::SyntaxNode) -> Vec<String> {
    use ast::HasName;
    let Some(list) = ast::Impl::cast(node.clone()).and_then(|i| i.assoc_item_list()) else {
        return Vec::new();
    };
    list.assoc_items()
        .filter_map(|a| match a {
            ast::AssocItem::Fn(f) => f.name(),
            ast::AssocItem::Const(c) => c.name(),
            ast::AssocItem::TypeAlias(t) => t.name(),
            ast::AssocItem::MacroCall(_) => None,
        })
        .map(|n| n.text().to_string())
        .collect()
}

/// A sibling that `strip_cfg` weighs an undecided node against.
struct Alt {
    node: ra_ap_syntax::SyntaxNode,
    start: usize,
    key: Option<String>,
    state: CfgState,
    all_off: bool,
    cfg: Option<Vec<String>>,
}

/// Whether `node`, under the undecided `#[cfg]` `toks`, gives way to a sibling: one whose
/// predicate is the complement of its own, or one of the same name (see `cfg_key`) that a
/// script compiles instead. A script has no features, so of several same-named alternatives
/// that depend on them, the ones on with every feature off win (`not(feature = "std")` over
/// `feature = "std"`); when none is, or several are, the first written wins (the one rustc
/// would compile with, say, only `--features std`, next to one that needs more).
fn loses_to_alternative(
    node: &ra_ap_syntax::SyntaxNode,
    toks: &[String],
    all_off: bool,
    siblings: &[Alt],
    debug: bool,
) -> bool {
    let start = usize::from(node.text_range().start());
    let others = || siblings.iter().filter(|e| e.node != *node);
    if !all_off
        && others().any(|e| {
            e.cfg
                .as_deref()
                .is_some_and(|t| cfg_complementary(toks, t, debug))
        })
    {
        return true;
    }
    let Some(key) = cfg_key(node) else {
        return false;
    };
    let is_impl = key.starts_with("impl ");
    let mine = is_impl.then(|| assoc_names(node));
    let rivals: Vec<&Alt> = others()
        .filter(|e| e.key.as_deref() == Some(key.as_str()) && e.state != CfgState::Off)
        .filter(|e| {
            mine.as_ref()
                .is_none_or(|mine| assoc_names(&e.node).iter().any(|n| mine.contains(n)))
        })
        .collect();
    if rivals.is_empty() {
        return false;
    }
    if rivals
        .iter()
        .any(|e| matches!(e.state, CfgState::Plain | CfgState::On))
    {
        return true;
    }
    let candidates = rivals
        .iter()
        .map(|e| (e.all_off, e.start))
        .chain(std::iter::once((all_off, start)));
    let winner = if all_off || rivals.iter().any(|e| e.all_off) {
        candidates.filter(|c| c.0).map(|c| c.1).min()
    } else {
        candidates.map(|c| c.1).min()
    };
    winner != Some(start)
}

/// Whether a `macro_rules!` body's matchers (each token tree just before a `=>`) have a `#`.
fn matchers_take_attrs(body: &ast::TokenTree) -> bool {
    let elems: Vec<SyntaxElement> = body
        .syntax()
        .children_with_tokens()
        .filter(|e| !e.kind().is_trivia())
        .collect();
    elems.windows(3).any(|w| {
        w[0].kind() == SyntaxKind::TOKEN_TREE
            && w[1].kind() == T![=]
            && w[2].kind() == T![>]
            && w[0].as_node().is_some_and(|n| {
                n.descendants_with_tokens().any(|t| t.kind() == T![#])
            })
    })
}

/// Blanks out (with spaces, so offsets stay put) every item, statement, field, variant, match
/// arm or expression under a `#[cfg(..)]` that's off for a script, so `#[cfg(windows)] fn x`
/// next to `#[cfg(not(windows))] fn x` leaves one `x`. Features count as unknown here (such
/// items stay), as they may be on in the crate the code comes from, except where two
/// alternatives would both stay (`loses_to_alternative`): `#[cfg(feature = "x")] fn x` next
/// to `#[cfg(not(feature = "x"))] fn x`, `all(feature = "std", not(feature = "ffi"))` next to
/// `any(not(feature = "std"), feature = "ffi")`, or three `fn now()`s each under another
/// combination of features.
pub fn strip_cfg(text: &mut String) -> bool {
    if !text.contains("cfg(") {
        return false;
    }
    let file = crate::macros::parse_tree(text);
    let debug = crate::codegen::overflow_checks();
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut scopes: HashMap<ra_ap_syntax::SyntaxNode, Vec<Alt>> = HashMap::new();
    for attr in file.syntax().descendants().filter_map(ast::Attr::cast) {
        if attr.excl_token().is_some() {
            continue;
        }
        let Some(toks) = attr_cfg_tokens(&attr) else {
            continue;
        };
        let mut i = 0;
        let val = cfg_pred3(&toks, &mut i, debug, true);
        let off = matches!(val, Ok(Some(false)) if i == toks.len());
        let undecided = matches!(val, Ok(None)) && i == toks.len();
        let Some(node) = attr.syntax().parent() else {
            continue;
        };
        if node.kind() == SyntaxKind::SOURCE_FILE {
            continue;
        }
        if !off {
            let Some(scope) = node.parent().filter(|_| undecided) else {
                continue;
            };
            let siblings = scopes.entry(scope.clone()).or_insert_with(|| {
                scope
                    .children()
                    .filter_map(|c| {
                        let (state, all_off, cfg) = node_cfg(&c, debug);
                        let key = cfg_key(&c);
                        (state != CfgState::Plain || key.is_some()).then(|| Alt {
                            start: usize::from(c.text_range().start()),
                            node: c,
                            key,
                            state,
                            all_off,
                            cfg,
                        })
                    })
                    .collect()
            });
            let mut j = 0;
            let all_off = matches!(
                cfg_pred3(&toks, &mut j, debug, false),
                Ok(Some(true)) if j == toks.len()
            );
            if !loses_to_alternative(&node, &toks, all_off, siblings, debug) {
                continue;
            }
        }
        let r = node.text_range();
        let (a, mut b) = (usize::from(r.start()), usize::from(r.end()));
        // A trailing comma goes too (fields, variants, arms, arguments).
        let mut next = node.next_sibling_or_token();
        while let Some(e) = &next {
            match e.kind() {
                SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => next = e.next_sibling_or_token(),
                SyntaxKind::COMMA => {
                    b = usize::from(e.text_range().end());
                    break;
                }
                _ => break,
            }
        }
        cuts.push((a, b));
    }
    // `#[cfg(..)]` on a macro's arguments (`vec![#[cfg(unix)] 1, 2]`, `sum!(1, #[cfg(feature =
    // "x")] 2)`): not items, so the attribute is read off the token tree. Off, the argument goes
    // too (through its comma); otherwise just the attribute.
    // (Not a call of a `macro_rules!` whose matchers take attributes themselves, `#[cfg(w =
    // $w:literal)]` or `$(#[$m:meta])*`: it gets them as written, and its expansion is stripped.)
    let attr_macros: Vec<String> = file
        .syntax()
        .descendants()
        .filter_map(ast::MacroRules::cast)
        .filter(|m| m.token_tree().is_some_and(|tt| matchers_take_attrs(&tt)))
        .filter_map(|m| ast::HasName::name(&m).map(|n| n.text().to_string()))
        .collect();
    for mc in file.syntax().descendants().filter_map(ast::MacroCall::cast) {
        let Some(tt) = mc.token_tree() else { continue };
        if mc.syntax().ancestors().skip(1).any(|a| ast::MacroCall::can_cast(a.kind())) {
            continue;
        }
        if macro_name(&mc).is_some_and(|n| attr_macros.contains(&n)) {
            continue;
        }
        // (`cfg_if! { if #[cfg(..)] { .. } }`: the attributes are its syntax.)
        if mc
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .is_some_and(|n| matches!(n.text().to_string().as_str(), "cfg_if" | "cfg_match"))
        {
            continue;
        }
        let toks: Vec<ra_ap_syntax::SyntaxToken> = tt
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() != SyntaxKind::WHITESPACE && t.kind() != SyntaxKind::COMMENT)
            .collect();
        let end = |k: usize| usize::from(toks[k].text_range().end());
        let mut i = 0;
        while i + 1 < toks.len() {
            if toks[i].text() != "#" || toks[i + 1].text() != "[" {
                i += 1;
                continue;
            }
            // The attribute's tokens, to the `]` closing it.
            let mut depth = 0i32;
            let mut j = i + 1;
            while j < toks.len() {
                match toks[j].text() {
                    "[" | "(" | "{" => depth += 1,
                    "]" | ")" | "}" => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if j >= toks.len() {
                break;
            }
            let attr: Vec<String> = toks[i..=j].iter().map(|t| t.text().to_string()).collect();
            let n = attr.len();
            if !(n >= 6 && attr[..4] == ["#", "[", "cfg", "("] && attr[n - 2..] == [")", "]"]) {
                i = j + 1;
                continue;
            }
            let pred = attr[4..n - 2].to_vec();
            let mut k = 0;
            let val = cfg_pred3(&pred, &mut k, debug, true);
            let off = matches!(val, Ok(Some(false)) if k == pred.len());
            let a = usize::from(toks[i].text_range().start());
            let mut b = end(j);
            if off {
                // The argument: to its comma (taken too) or the end of its token tree.
                let mut depth = 0i32;
                let mut m = j + 1;
                while m < toks.len() {
                    match toks[m].text() {
                        "(" | "[" | "{" => depth += 1,
                        ")" | "]" | "}" => {
                            if depth == 0 {
                                break;
                            }
                            depth -= 1;
                        }
                        "," | ";" if depth == 0 => {
                            b = end(m);
                            break;
                        }
                        _ => {}
                    }
                    b = end(m);
                    m += 1;
                }
            }
            cuts.push((a, b));
            i = j + 1;
        }
    }
    if cuts.is_empty() {
        return false;
    }
    cuts.sort();
    let mut end = 0;
    for (a, b) in cuts {
        if a < end || b > text.len() {
            continue;
        }
        let blank: String = text[a..b]
            .chars()
            .map(|c| match c {
                '\n' | '\r' => c.to_string(),
                _ => " ".repeat(c.len_utf8()),
            })
            .collect();
        text.replace_range(a..b, &blank);
        end = b;
    }
    true
}
