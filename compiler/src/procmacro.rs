//! User-written proc macros. A file whose functions carry `#[proc_macro]`,
//! `#[proc_macro_derive(Name)]` or `#[proc_macro_attribute]` is a proc-macro crate, used as in
//! Rust: `use mac::show;` then `show!(..)`, `mac::show!(..)`, `#[derive(mac::Show)]`,
//! `#[mac::traced]`. There's no `mod mac;`: the crate is read from `mac.rs` next to the file.
//!
//! The crate is compiled to JS with `TokenStream` as a `String` (`input.to_string()`,
//! `format!(..).parse().unwrap()`), and each invocation runs it through the host's
//! `__carciniRunMacro` (installed by the runtime), as a pre-pass over the source: the output
//! replaces the invocation (a derive's goes after the item). An `OffsetMap` keeps positions
//! pointing at the original text; errors in a macro's output point at the invocation.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, NodeOrToken, SyntaxKind, SyntaxNode, TextRange,
    ast::{self, HasAttrs, HasName},
};

use crate::mbe::OffsetMap;
use crate::typeck::Resolver;

/// A proc-macro crate: its macros, and its JS once compiled.
struct Crate {
    text: String,
    file: String,
    /// `#[proc_macro]` functions.
    fn_like: HashSet<String>,
    /// `#[proc_macro_derive(Name)]`: derive name → function.
    derives: HashMap<String, String>,
    /// `#[proc_macro_attribute]` functions.
    attrs: HashSet<String>,
    js: Option<Result<String, String>>,
}

#[cfg(target_arch = "wasm32")]
mod host {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(catch, js_name = __carciniRunMacro)]
        fn run(
            code: &str,
            name: &str,
            input: &str,
            attr: Option<String>,
        ) -> Result<String, JsValue>;
    }

    pub fn run_macro(
        code: &str,
        name: &str,
        input: &str,
        attr: Option<&str>,
    ) -> Result<String, String> {
        run(code, name, input, attr.map(str::to_string)).map_err(|e| {
            e.as_string().unwrap_or_else(|| {
                "the proc macro runner isn't installed (load CarciniScript's runtime)".into()
            })
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod host {
    pub fn run_macro(_: &str, _: &str, _: &str, _: Option<&str>) -> Result<String, String> {
        Err("proc macros run only in the JavaScript build of CarciniScript".into())
    }
}

thread_local! {
    /// The proc-macro crates the current compile read.
    static DEPENDENCIES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The proc-macro crates read since the last call (and forget them).
pub(crate) fn take_dependencies() -> Vec<String> {
    DEPENDENCIES.with(|d| std::mem::take(&mut *d.borrow_mut()))
}

pub(crate) fn restore_dependencies(deps: Vec<String>) {
    DEPENDENCIES.with(|d| *d.borrow_mut() = deps);
}

/// What `prepared` appends: `TokenStream` is text.
const PRELUDE: &str = include_str!("procmacro_prelude.rs");

/// The part of `syn` the prelude has, for crates that use it.
const SYN: &str = include_str!("procmacro_syn.rs");

/// `syn`'s tokens: (name in `syn::token`, its text, the type the prelude gives it). The type's
/// name is never longer than `Token![text]`, so writing it there keeps positions (padded with
/// spaces).
const TOKENS: &[(&str, &str, &str)] = &[
    ("Comma", ",", "TkComma"),
    ("Semi", ";", "TkSemi"),
    ("Colon", ":", "TkColon"),
    ("PathSep", "::", "TkPathSep"),
    ("Dot", ".", "TkDot"),
    ("DotDot", "..", "TkDotDot"),
    ("DotDotDot", "...", "TkDot3"),
    ("DotDotEq", "..=", "TkDotDotEq"),
    ("Eq", "=", "TkEq"),
    ("EqEq", "==", "TkEqEq"),
    ("Ne", "!=", "TkNe"),
    ("Lt", "<", "TkLt"),
    ("Le", "<=", "TkLe"),
    ("Gt", ">", "TkGt"),
    ("Ge", ">=", "TkGe"),
    ("FatArrow", "=>", "TkFatArrow"),
    ("RArrow", "->", "TkRArrow"),
    ("LArrow", "<-", "TkLArrow"),
    ("Pound", "#", "TkPound"),
    ("Not", "!", "TkNot"),
    ("Plus", "+", "TkPlus"),
    ("PlusEq", "+=", "TkPlusEq"),
    ("Minus", "-", "TkMinus"),
    ("MinusEq", "-=", "TkMinusEq"),
    ("Star", "*", "TkStar"),
    ("StarEq", "*=", "TkStarEq"),
    ("Slash", "/", "TkSlash"),
    ("SlashEq", "/=", "TkSlashEq"),
    ("Percent", "%", "TkPercent"),
    ("PercentEq", "%=", "TkRemEq"),
    ("Caret", "^", "TkCaret"),
    ("CaretEq", "^=", "TkCaretEq"),
    ("And", "&", "TkAnd"),
    ("AndAnd", "&&", "TkAndAnd"),
    ("AndEq", "&=", "TkAndEq"),
    ("Or", "|", "TkOr"),
    ("OrOr", "||", "TkOrOr"),
    ("OrEq", "|=", "TkOrEq"),
    ("Shl", "<<", "TkShl"),
    ("ShlEq", "<<=", "TkShlEq"),
    ("Shr", ">>", "TkShr"),
    ("ShrEq", ">>=", "TkShrEq"),
    ("At", "@", "TkAt"),
    ("Question", "?", "TkQues"),
    ("Dollar", "$", "TkDollar"),
    ("Tilde", "~", "TkTilde"),
    ("Underscore", "_", "TkUnder"),
    ("Abstract", "abstract", "TkAbstract"),
    ("As", "as", "TkAs"),
    ("Async", "async", "TkAsync"),
    ("Await", "await", "TkAwait"),
    ("Become", "become", "TkBecome"),
    ("Box", "box", "TkBox"),
    ("Break", "break", "TkBreak"),
    ("Const", "const", "TkConst"),
    ("Continue", "continue", "TkContinue"),
    ("Crate", "crate", "TkCrate"),
    ("Default", "default", "TkDefault"),
    ("Do", "do", "TkDo"),
    ("Dyn", "dyn", "TkDyn"),
    ("Else", "else", "TkElse"),
    ("Enum", "enum", "TkEnum"),
    ("Extern", "extern", "TkExtern"),
    ("Final", "final", "TkFinal"),
    ("Fn", "fn", "TkFn"),
    ("For", "for", "TkFor"),
    ("If", "if", "TkIf"),
    ("Impl", "impl", "TkImpl"),
    ("In", "in", "TkIn"),
    ("Let", "let", "TkLet"),
    ("Loop", "loop", "TkLoop"),
    ("Macro", "macro", "TkMacro"),
    ("Match", "match", "TkMatch"),
    ("Mod", "mod", "TkMod"),
    ("Move", "move", "TkMove"),
    ("Mut", "mut", "TkMut"),
    ("Override", "override", "TkOverride"),
    ("Priv", "priv", "TkPriv"),
    ("Pub", "pub", "TkPub"),
    ("Raw", "raw", "TkRaw"),
    ("Ref", "ref", "TkRef"),
    ("Return", "return", "TkReturn"),
    ("SelfType", "Self", "TkSelfType"),
    ("SelfValue", "self", "TkSelfValue"),
    ("Static", "static", "TkStatic"),
    ("Struct", "struct", "TkStruct"),
    ("Super", "super", "TkSuper"),
    ("Trait", "trait", "TkTrait"),
    ("Try", "try", "TkTry"),
    ("Type", "type", "TkType"),
    ("Typeof", "typeof", "TkTypeof"),
    ("Union", "union", "TkUnion"),
    ("Unsafe", "unsafe", "TkUnsafe"),
    ("Unsized", "unsized", "TkUnsized"),
    ("Use", "use", "TkUse"),
    ("Virtual", "virtual", "TkVirtual"),
    ("Where", "where", "TkWhere"),
    ("While", "while", "TkWhile"),
    ("Yield", "yield", "TkYield"),
];

/// The delimiter tokens (`parenthesized!` returns a `token::Paren`).
const DELIMITERS: &[(&str, &str)] = &[
    ("Paren", "TkParen"),
    ("Brace", "TkBrace"),
    ("Bracket", "TkBracket"),
];

/// The prelude's token types: each parses, peeks and quotes as its text.
fn token_types() -> String {
    let mut out = String::new();
    for (_, text, ty) in TOKENS {
        let keyword = text
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_');
        out.push_str(&format!(
            "#[derive(Clone, Copy, Debug, Default)]\npub struct {ty};\n\
             impl {ty} {{ pub fn __tt(&self) -> String {{ {text:?}.to_string() }} pub fn span(&self) -> Span {{ Span }} }}\n\
             impl Parse for {ty} {{ fn parse(input: ParseStream) -> SynResult<{ty}> {{ input.__token({text:?}, {keyword})?; Ok({ty}) }} }}\n\
             impl __Peek for {ty} {{ fn __peek_at(&self, input: &ParseBuffer, k: usize) -> bool {{ input.__peek_token(k, {text:?}, {keyword}) }} fn __display(&self) -> String {{ \"`{text}`\".to_string() }} }}\n"
        ));
    }
    for (_, ty) in DELIMITERS {
        out.push_str(&format!(
            "#[derive(Clone, Copy, Debug, Default)]\npub struct {ty};\nimpl {ty} {{ pub fn span(&self) -> Span {{ Span }} }}\n"
        ));
    }
    out
}

/// The crates whose paths are dropped (`proc_macro::TokenStream` is the prelude's).
const MACRO_CRATES: [&str; 4] = ["proc_macro", "proc_macro2", "quote", "syn"];

/// Whether `text` is a proc-macro crate (not yet prepared).
pub(crate) fn is_proc_crate(text: &str) -> bool {
    text.contains("#[proc_macro") && !text.contains("// (proc-macro crate prelude)")
}

/// The proc macros `text` defines.
fn read_crate(file: String, text: String) -> Crate {
    let parsed = crate::macros::parse_tree(&text);
    let mut c = Crate {
        text: String::new(),
        file,
        fn_like: HashSet::new(),
        derives: HashMap::new(),
        attrs: HashSet::new(),
        js: None,
    };
    for f in parsed.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(fname) = f.name().map(|n| n.text().to_string()) else {
            continue;
        };
        for a in f.attrs() {
            let Some(p) = a.path() else { continue };
            match p.syntax().text().to_string().as_str() {
                "proc_macro" => {
                    c.fn_like.insert(fname.clone());
                }
                "proc_macro_attribute" => {
                    c.attrs.insert(fname.clone());
                }
                "proc_macro_derive" => {
                    // `#[proc_macro_derive(Name, attributes(helper))]`
                    if let Some(tt) = attr_tt(&a)
                        && let Some(first) = tt
                            .syntax()
                            .children_with_tokens()
                            .filter_map(|e| e.into_token())
                            .find(|t| t.kind() == SyntaxKind::IDENT)
                    {
                        c.derives.insert(first.text().to_string(), fname.clone());
                    }
                }
                _ => {}
            }
        }
    }
    c.text = text;
    c
}

/// The crate's source as CarciniScript compiles it: `use proc_macro::..` (and `proc_macro2`,
/// `quote`) blanked out and those crates' paths dropped (same length, so positions hold), then
/// the prelude (`TokenStream`, `Ident`, `Span`, `Literal`) and a `String`-to-`String` entry
/// point `__pm_name` for each macro, which the runner calls.
pub(crate) fn prepared(text: &str) -> String {
    let parsed = crate::macros::parse_tree(text);
    let mut out = text.to_string();
    let blank = |out: &mut String, r: TextRange| {
        let (a, b) = (usize::from(r.start()), usize::from(r.end()));
        let spaces: String = out[a..b]
            .chars()
            .map(|c| if c == '\n' { '\n' } else { ' ' })
            .collect();
        out.replace_range(a..b, &spaces);
    };
    let mut aliases = String::new();
    let mut entries = String::new();
    // `use syn::Result;` (or `syn::parse::Result`): `Result<T>` is syn's here.
    let mut syn_result = false;
    for item in parsed.syntax().descendants() {
        if let Some(u) = ast::Use::cast(item.clone()) {
            let root = u
                .use_tree()
                .and_then(|t| t.path())
                .and_then(|p| p.first_segment())
                .map(|s| s.syntax().text().to_string())
                .unwrap_or_default();
            if MACRO_CRATES.contains(&root.as_str()) {
                // `use proc_macro2::TokenStream as TokenStream2;` keeps its name.
                for (segs, rename, _) in crate::typeck::use_leaves(&u) {
                    if segs.first().is_some_and(|f| f == "syn")
                        && segs.last().is_some_and(|l| l == "Result")
                        && rename.is_none()
                    {
                        syn_result = true;
                    }
                    if let (Some(last), Some(new)) = (segs.last(), rename)
                        && matches!(last.as_str(), "TokenStream" | "Ident" | "Span" | "Literal")
                    {
                        aliases.push_str(&format!("type {new} = {last};\n"));
                    }
                }
                blank(&mut out, item.text_range());
            }
        } else if let Some(e) = ast::ExternCrate::cast(item.clone()) {
            if e.name_ref()
                .is_some_and(|n| MACRO_CRATES.iter().any(|c| n.text() == *c))
            {
                blank(&mut out, item.text_range());
            }
        } else if let Some(f) = ast::Fn::cast(item.clone())
            && let Some(name) = f.name().map(|n| n.text().to_string())
        {
            for a in f.attrs() {
                let Some(p) = a.path() else { continue };
                match p.syntax().text().to_string().as_str() {
                    "proc_macro" | "proc_macro_derive" => entries.push_str(&format!(
                        "pub fn __pm_{name}(input: String) -> String {{ {name}(TokenStream {{ __text: input }}).to_string() }}\n"
                    )),
                    "proc_macro_attribute" => entries.push_str(&format!(
                        "pub fn __pm_{name}(attr: String, input: String) -> String {{ {name}(TokenStream {{ __text: attr }}, TokenStream {{ __text: input }}).to_string() }}\n"
                    )),
                    _ => {}
                }
            }
        }
    }
    // `proc_macro::TokenStream::new()`, `quote::quote!(..)`: the path's crate goes.
    let toks: Vec<ra_ap_syntax::SyntaxToken> = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    // Rewrites that keep the text's length (padded with spaces), so positions hold.
    let put = |out: &mut String, r: TextRange, with: &str| {
        let (a, b) = (usize::from(r.start()), usize::from(r.end()));
        if with.len() <= b - a && !out[a..b].contains('\n') {
            out.replace_range(a..b, &format!("{:<width$}", with, width = b - a));
        }
    };
    let span = |a: &ra_ap_syntax::SyntaxToken, b: &ra_ap_syntax::SyntaxToken| {
        TextRange::new(a.text_range().start(), b.text_range().end())
    };
    for (i, t) in toks.iter().enumerate() {
        let at = |k: usize| {
            toks.get(i + k)
                .map(|t| t.text().to_string())
                .unwrap_or_default()
        };
        let colon2 = |k: usize| {
            toks.get(i + k)
                .is_some_and(|t| t.kind() == SyntaxKind::COLON2)
        };
        // `syn::Result<T>` / `parse::Result<T>`: the prelude's `SynResult<T>`.
        if matches!(t.text(), "syn" | "parse") && colon2(1) && at(2) == "Result" {
            put(&mut out, span(t, &toks[i + 2]), "SynResult");
        }
        // `syn::parse::{Parse, ParseStream}` paths: the prelude's names.
        if (t.text() == "parse"
            && colon2(1)
            && matches!(at(2).as_str(), "Parse" | "ParseStream" | "ParseBuffer"))
            || (t.text() == "punctuated" && colon2(1) && at(2) == "Punctuated")
        {
            blank(&mut out, span(t, &toks[i + 1]));
        }
        // `Token![,]`: the token's type.
        if t.text() == "Token" && at(1) == "!" && at(2) == "[" {
            let close = (i + 3..toks.len()).find(|&k| toks[k].text() == "]");
            if let Some(close) = close {
                let sym: String = toks[i + 3..close]
                    .iter()
                    .map(|t| t.text().to_string())
                    .collect();
                if let Some((_, _, ty)) = TOKENS.iter().find(|(_, text, _)| *text == sym) {
                    put(&mut out, span(t, &toks[close]), ty);
                }
            }
        }
        // `token::Comma`, `syn::token::Paren`
        if t.text() == "token" && colon2(1) {
            let name = at(2);
            let ty = TOKENS
                .iter()
                .find(|(n, _, _)| *n == name)
                .map(|t| t.2)
                .or_else(|| DELIMITERS.iter().find(|(n, _)| *n == name).map(|d| d.1));
            if let Some(ty) = ty {
                put(&mut out, span(t, &toks[i + 2]), ty);
            }
        }
        // `input.peek(Ident)`, `lookahead.peek(LitStr)`: a value standing for the type.
        if matches!(t.text(), "peek" | "peek2" | "peek3") && at(1) == "(" {
            // (`syn::Ident` too: its `syn::` is blanked.)
            let k = if at(2) == "syn" && colon2(3) {
                i + 4
            } else {
                i + 2
            };
            if let Some(arg) = toks.get(k)
                && toks.get(k + 1).is_some_and(|c| c.text() == ")")
                && matches!(
                    arg.text(),
                    "Ident" | "LitStr" | "LitInt" | "LitFloat" | "LitBool" | "Lit" | "Lifetime"
                )
            {
                put(&mut out, arg.text_range(), &arg.text().to_uppercase());
            }
        }
    }
    for w in toks.windows(2) {
        if w[0].kind() == SyntaxKind::IDENT
            && MACRO_CRATES.contains(&w[0].text())
            && w[1].kind() == SyntaxKind::COLON2
            && !w[0]
                .parent_ancestors()
                .any(|a| ast::Use::can_cast(a.kind()) || ast::Attr::can_cast(a.kind()))
        {
            blank(
                &mut out,
                TextRange::new(w[0].text_range().start(), w[1].text_range().end()),
            );
        }
    }
    // `syn::parse::<T>(..)` / `parse2` / `parse_str`: the prelude's (after `syn::` is gone).
    for (i, t) in toks.iter().enumerate() {
        let next = |k: usize| {
            toks.get(i + k)
                .map(|t| t.text().to_string())
                .unwrap_or_default()
        };
        if t.text() == "syn"
            && toks
                .get(i + 1)
                .is_some_and(|t| t.kind() == SyntaxKind::COLON2)
            && matches!(next(2).as_str(), "parse" | "parse2" | "parse_str")
            && (next(3) == "(" || (next(3) == "::" && next(4) == "<"))
            && !t
                .parent_ancestors()
                .any(|a| ast::Use::can_cast(a.kind()) || ast::Attr::can_cast(a.kind()))
        {
            let with = format!("__s{}", next(2));
            put(&mut out, span(t, &toks[i + 2]), &with);
        }
    }
    out.push_str(PRELUDE);
    if toks.iter().any(|t| t.text() == "syn") {
        out.push_str(SYN);
        out.push_str(&token_types());
        if syn_result {
            out.push_str("type Result<T> = SynResult<T>;\n");
        }
    }
    out.push_str(&aliases);
    out.push_str(&entries);
    out
}

impl Crate {
    /// The crate as JS, compiled on first use.
    fn js(&mut self, resolver: Resolver) -> Result<String, String> {
        if let Some(js) = &self.js {
            return js.clone();
        }
        // The editor compiles the calling file on every keystroke: the crate's JS is kept
        // between compiles (by its text, and the setting that changes the JS).
        let key = format!("{}{}", crate::codegen::overflow_checks(), self.text);
        if let Some(js) = COMPILED.with(|c| {
            c.borrow()
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, js)| js.clone())
        }) {
            self.js = Some(js.clone());
            return js;
        }
        // (`compile_with` prepares the crate's text.)
        let out = crate::codegen::compile_with(&self.text, &self.file, resolver);
        let js = match out.code {
            Some(code) if !code.contains("\nimport ") && !code.starts_with("import ") => {
                Ok(with_step_limit(&code))
            }
            Some(_) => Err(format!(
                "the proc macro crate `{}` can't use `mod` files",
                self.file
            )),
            None => {
                let e = out
                    .diagnostics
                    .iter()
                    .find(|d| d.severity == "error")
                    .map(|d| format!("{}:{}:{}: {}", self.file, d.line, d.column, d.message))
                    .unwrap_or_default();
                Err(format!("the proc macro crate doesn't compile: {e}"))
            }
        };
        self.js = Some(js.clone());
        COMPILED.with(|c| {
            let mut c = c.borrow_mut();
            if c.len() >= 8 {
                let _ = c.remove(0);
            }
            c.push((key, js.clone()));
        });
        js
    }
}

thread_local! {
    /// Proc-macro crates compiled recently: (key, JS or the error).
    static COMPILED: std::cell::RefCell<Vec<(String, Result<String, String>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The text of `node`'s tokens as rustc's `TokenStream::to_string` gives it: comments dropped,
/// whitespace between tokens as one space (none where the source had none), a line break after
/// each `///` doc comment. Tokens in `skip` (removed attributes) are left out.
fn token_text(node: &SyntaxNode, skip: &[TextRange]) -> String {
    let mut out = String::new();
    let mut gap = false;
    for e in node.descendants_with_tokens() {
        let NodeOrToken::Token(t) = e else { continue };
        if skip.iter().any(|r| r.contains_range(t.text_range())) {
            gap = true;
            continue;
        }
        match t.kind() {
            SyntaxKind::WHITESPACE => gap = true,
            SyntaxKind::COMMENT | SyntaxKind::OUTER_DOC_COMMENT | SyntaxKind::INNER_DOC_COMMENT => {
                let s = t.text();
                if s.starts_with("///") || s.starts_with("//!") {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(s.trim_end());
                    out.push('\n');
                    gap = false;
                } else {
                    gap = true;
                }
            }
            _ => {
                if gap && !out.is_empty() && !out.ends_with('\n') {
                    out.push(' ');
                }
                out.push_str(t.text());
                gap = false;
            }
        }
    }
    out.trim().to_string()
}

/// The inside of a token tree (`(a, b)` → `a, b`), as `token_text` renders it.
fn tt_inner(tt: &ast::TokenTree) -> String {
    let full = token_text(tt.syntax(), &[]);
    let mut chars = full.chars();
    chars.next();
    chars.next_back();
    chars.as_str().trim().to_string()
}

/// Expand every proc macro invocation in `text` (several rounds, since a macro's output may
/// use more). Whether anything changed; errors as original ranges.
pub fn expand(
    text: &mut String,
    map: &mut OffsetMap,
    filename: &str,
    resolver: Resolver,
) -> Result<bool, Vec<(usize, usize, String)>> {
    let mut crates: HashMap<String, Crate> = HashMap::new();
    let mut not_crates: HashSet<String> = HashSet::new();
    let mut changed = false;
    // `use mac::{show, Show as Describe}`: local name → (crate, macro). Kept across rounds (the
    // `use`s are gone after the first).
    let mut imported: HashMap<String, (String, String)> = HashMap::new();
    for _round in 0..8 {
        let file = crate::macros::parse_tree(text);
        // Names a path could start with that are other crates (`mac::show!`, `use mac::Show`).
        let mut local_mods: HashSet<String> = HashSet::new();
        for m in file.syntax().descendants().filter_map(ast::Module::cast) {
            if let Some(n) = m.name() {
                local_mods.insert(n.text().to_string());
            }
        }
        let mut load = |name: &str, crates: &mut HashMap<String, Crate>| -> bool {
            if crates.contains_key(name) {
                return true;
            }
            if not_crates.contains(name)
                || local_mods.contains(name)
                || matches!(
                    name,
                    "std" | "core" | "alloc" | "crate" | "self" | "super" | "Self"
                )
            {
                return false;
            }
            let path = crate::typeck::module_path(filename, name);
            match resolver(&path) {
                Some(t) if is_proc_crate(&t) => {
                    DEPENDENCIES.with(|d| d.borrow_mut().push(path.clone()));
                    crates.insert(name.to_string(), read_crate(path, t));
                    true
                }
                _ => {
                    not_crates.insert(name.to_string());
                    false
                }
            }
        };
        let mut uses_to_drop: Vec<TextRange> = Vec::new();
        for u in file.syntax().descendants().filter_map(ast::Use::cast) {
            let leaves = crate::typeck::use_leaves(&u);
            let mut all_proc = !leaves.is_empty();
            for (segs, rename, star) in &leaves {
                let Some(root) = segs.first() else {
                    all_proc = false;
                    continue;
                };
                if !load(root, &mut crates) {
                    all_proc = false;
                    continue;
                }
                let c = &crates[root];
                if *star && segs.len() == 1 {
                    for n in c
                        .fn_like
                        .iter()
                        .chain(c.attrs.iter())
                        .chain(c.derives.keys())
                    {
                        imported.insert(n.clone(), (root.clone(), n.clone()));
                    }
                } else if segs.len() == 2 {
                    let local = rename.clone().unwrap_or_else(|| segs[1].clone());
                    imported.insert(local, (root.clone(), segs[1].clone()));
                }
            }
            if all_proc {
                uses_to_drop.push(u.syntax().text_range());
            }
        }
        // A macro path (`show`, `mac::show`) → (crate, macro name).
        let resolve = |segs: &[String],
                       crates: &mut HashMap<String, Crate>,
                       load: &mut dyn FnMut(&str, &mut HashMap<String, Crate>) -> bool|
         -> Option<(String, String)> {
            match segs {
                [one] => imported.get(one).cloned(),
                [krate, name] if load(krate, crates) => Some((krate.clone(), name.clone())),
                _ => None,
            }
        };
        let path_segs = |p: &ast::Path| -> Vec<String> {
            p.segments()
                .filter_map(|s| s.name_ref())
                .map(|n| n.text().to_string())
                .collect()
        };

        // (range in the current text, replacement)
        let mut edits: Vec<(TextRange, String)> = Vec::new();
        let mut errors: Vec<(TextRange, String)> = Vec::new();
        let run = |crates: &mut HashMap<String, Crate>,
                   krate: &str,
                   func: &str,
                   input: &str,
                   attr: Option<&str>|
         -> Result<String, String> {
            let c = crates.get_mut(krate).unwrap();
            let js = c.js(resolver)?;
            let out = host::run_macro(&js, &format!("__pm_{func}"), input, attr)?;
            // `compile_error!("..")` in the output (`syn::Error::to_compile_error()`): that's
            // the error, at the invocation.
            match compile_error_in(&out) {
                Some(msg) => Err(format!("\u{1}{msg}")),
                None => Ok(out),
            }
        };
        let taken = |edits: &Vec<(TextRange, String)>, r: TextRange| {
            edits
                .iter()
                .any(|(e, _)| e.contains_range(r) || r.contains_range(*e))
        };

        // Attribute macros first: they replace the whole item.
        for item in file.syntax().descendants().filter_map(ast::Item::cast) {
            let Some(holder) = ast::AnyHasAttrs::cast(item.syntax().clone()) else {
                continue;
            };
            for a in holder.attrs() {
                let Some(p) = a.path() else { continue };
                let segs = path_segs(&p);
                let Some((krate, name)) = resolve(&segs, &mut crates, &mut load) else {
                    continue;
                };
                if !crates[&krate].attrs.contains(&name) {
                    continue;
                }
                let range = item.syntax().text_range();
                if taken(&edits, range) {
                    break;
                }
                let args = attr_tt(&a).map(|t| tt_inner(&t)).unwrap_or_default();
                let input = token_text(item.syntax(), &[a.syntax().text_range()]);
                match run(&mut crates, &krate, &name, &input, Some(&args)) {
                    Ok(out) => edits.push((range, out)),
                    Err(e) => errors.push((a.syntax().text_range(), e)),
                }
                break;
            }
        }
        // Derives: the output goes after the item; the derive list loses the name.
        for item in file.syntax().descendants().filter_map(ast::Item::cast) {
            if !matches!(
                item,
                ast::Item::Struct(_) | ast::Item::Enum(_) | ast::Item::Union(_)
            ) {
                continue;
            }
            let Some(holder) = ast::AnyHasAttrs::cast(item.syntax().clone()) else {
                continue;
            };
            let derive_attrs: Vec<ast::Attr> = holder
                .attrs()
                .filter(|a| a.path().is_some_and(|p| p.syntax().text() == "derive"))
                .collect();
            let mut outputs = Vec::new();
            let mut attr_edits = Vec::new();
            for a in &derive_attrs {
                let Some(tt) = attr_tt(a) else { continue };
                let inner = tt_inner(&tt);
                let mut kept = Vec::new();
                let mut any = false;
                for name in inner.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    let segs: Vec<String> =
                        name.split("::").map(|s| s.trim().to_string()).collect();
                    let found = resolve(&segs, &mut crates, &mut load).and_then(|(k, n)| {
                        crates[&k].derives.get(&n).map(|f| (k.clone(), f.clone()))
                    });
                    match found {
                        Some((krate, func)) => {
                            any = true;
                            let skip: Vec<TextRange> = derive_attrs
                                .iter()
                                .map(|d| d.syntax().text_range())
                                .collect();
                            let input = token_text(item.syntax(), &skip);
                            match run(&mut crates, &krate, &func, &input, None) {
                                Ok(out) => outputs.push(out),
                                Err(e) => errors.push((a.syntax().text_range(), e)),
                            }
                        }
                        None => kept.push(name.to_string()),
                    }
                }
                if any {
                    let new = if kept.is_empty() {
                        String::new()
                    } else {
                        format!("#[derive({})]", kept.join(", "))
                    };
                    attr_edits.push((a.syntax().text_range(), new));
                }
            }
            if attr_edits.is_empty() || taken(&edits, item.syntax().text_range()) {
                continue;
            }
            edits.extend(attr_edits);
            let end = item.syntax().text_range().end();
            edits.push((TextRange::empty(end), format!("\n{}\n", outputs.join("\n"))));
        }
        // Function-like macros.
        for mc in file.syntax().descendants().filter_map(ast::MacroCall::cast) {
            if mc
                .syntax()
                .ancestors()
                .skip(1)
                .any(|a| ast::MacroRules::can_cast(a.kind()) || ast::MacroCall::can_cast(a.kind()))
            {
                continue;
            }
            let Some(p) = mc.path() else { continue };
            let segs = path_segs(&p);
            let Some((krate, name)) = resolve(&segs, &mut crates, &mut load) else {
                continue;
            };
            if !crates[&krate].fn_like.contains(&name) {
                continue;
            }
            let range = mc.syntax().text_range();
            if taken(&edits, range) {
                continue;
            }
            let input = mc.token_tree().map(|t| tt_inner(&t)).unwrap_or_default();
            match run(&mut crates, &krate, &name, &input, None) {
                Ok(out) => {
                    // In an expression the expansion is one expression: `2 * show!(1 + 2)`.
                    let expr = mc
                        .syntax()
                        .parent()
                        .is_some_and(|x| x.kind() == SyntaxKind::MACRO_EXPR)
                        && !mc
                            .syntax()
                            .parent()
                            .and_then(|x| x.parent())
                            .is_some_and(|x| {
                                matches!(x.kind(), SyntaxKind::EXPR_STMT | SyntaxKind::STMT_LIST)
                            });
                    let out = if expr { format!("({out})") } else { out };
                    // An item-position call's `;` goes with it.
                    edits.push((range, out));
                }
                Err(e) => errors.push((range, e)),
            }
        }
        // Function-like macros inside other macros' arguments: `println!("{}", show!(x))`
        // is tokens there, `show` `!` `(..)`.
        for tt in file.syntax().descendants().filter_map(ast::TokenTree::cast) {
            if tt
                .syntax()
                .ancestors()
                .any(|a| ast::MacroRules::can_cast(a.kind()) || ast::Attr::can_cast(a.kind()))
            {
                continue;
            }
            let kids: Vec<NodeOrToken<SyntaxNode, ra_ap_syntax::SyntaxToken>> = tt
                .syntax()
                .children_with_tokens()
                .filter(|e| !matches!(e.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
                .collect();
            let mut i = 0;
            while i < kids.len() {
                // `a` (`::` `b`)* `!` `(..)`
                let start = i;
                let mut segs = Vec::new();
                let mut j = i;
                while let Some(NodeOrToken::Token(t)) = kids.get(j) {
                    if t.kind() != SyntaxKind::IDENT {
                        break;
                    }
                    segs.push(t.text().to_string());
                    j += 1;
                    match kids.get(j).map(|e| e.kind()) {
                        Some(SyntaxKind::COLON2) => j += 1,
                        _ => break,
                    }
                }
                let preceded_by_path = start > 0
                    && matches!(kids[start - 1].kind(), SyntaxKind::COLON2 | SyntaxKind::DOT);
                let (Some(bang), Some(NodeOrToken::Node(group))) = (kids.get(j), kids.get(j + 1))
                else {
                    i = j.max(i + 1);
                    continue;
                };
                if segs.is_empty() || preceded_by_path || bang.kind() != SyntaxKind::BANG {
                    i = j.max(i + 1);
                    continue;
                }
                let Some(group) = ast::TokenTree::cast(group.clone()) else {
                    i = j + 1;
                    continue;
                };
                let found = resolve(&segs, &mut crates, &mut load)
                    .filter(|(k, n)| crates[k].fn_like.contains(n));
                if let Some((krate, name)) = found {
                    let range = TextRange::new(
                        kids[start].text_range().start(),
                        group.syntax().text_range().end(),
                    );
                    if !taken(&edits, range) {
                        match run(&mut crates, &krate, &name, &tt_inner(&group), None) {
                            Ok(out) => edits.push((range, format!("({out})"))),
                            Err(e) => errors.push((range, e)),
                        }
                    }
                }
                i = j + 2;
            }
        }
        if !errors.is_empty() {
            return Err(errors
                .into_iter()
                .map(|(r, e)| {
                    let (a, b) = (usize::from(r.start()), usize::from(r.end()));
                    let msg = if let Some(m) = e.strip_prefix('\u{1}') {
                        m.to_string()
                    } else if e.starts_with("the proc macro") || e.starts_with("proc macros run") {
                        e
                    } else {
                        format!("proc macro panicked: {e}")
                    };
                    (map.orig(a, false), map.orig(b, true), msg)
                })
                .collect());
        }
        // The `use`s of proc-macro crates go too (they aren't modules).
        let done = edits.is_empty();
        if done && uses_to_drop.is_empty() {
            return Ok(changed);
        }
        edits.extend(uses_to_drop.into_iter().map(|r| (r, String::new())));
        apply(text, map, edits);
        changed = true;
        if done {
            return Ok(true);
        }
    }
    Ok(changed)
}

fn apply(text: &mut String, map: &mut OffsetMap, mut edits: Vec<(TextRange, String)>) {
    edits.sort_by_key(|(r, _)| std::cmp::Reverse((r.start(), r.end())));
    let mut last_start = usize::MAX;
    for (r, new) in edits {
        let (a, b) = (usize::from(r.start()), usize::from(r.end()));
        if b > last_start {
            continue;
        }
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
        last_start = a;
    }
}

/// The crate's JS with a step count at the top of every loop (`κσ.$tick()`), so a macro that
/// never finishes stops with an error instead of hanging the editor, which expands macros on
/// every keystroke.
fn with_step_limit(js: &str) -> String {
    let mut out = String::with_capacity(js.len() + 1024);
    for line in js.lines() {
        out.push_str(line);
        let head = line.trim_start();
        let head = match head.split_once(": ") {
            // `outer: while (..) {`
            Some((label, rest))
                if label
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '$') =>
            {
                rest
            }
            _ => head,
        };
        if (head.starts_with("while (") || head.starts_with("for (") || head.starts_with("do {"))
            && line.ends_with('{')
        {
            out.push_str(" κσ.$tick();");
        }
        out.push('\n');
    }
    out
}

/// The message of a `compile_error!("..")` in a macro's output.
fn compile_error_in(out: &str) -> Option<String> {
    if !out.contains("compile_error") {
        return None;
    }
    let file = crate::macros::parse_tree(out);
    let toks: Vec<ra_ap_syntax::SyntaxToken> = file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let at = toks.iter().position(|t| t.text() == "compile_error")?;
    toks[at..]
        .iter()
        .find_map(|t| <ast::String as ra_ap_syntax::AstToken>::cast(t.clone()))
        .and_then(|s| s.value().ok().map(|v| v.into_owned()))
}

/// An attribute's token tree: the `(..)` of `#[derive(..)]`.
fn attr_tt(a: &ast::Attr) -> Option<ast::TokenTree> {
    a.syntax().descendants().find_map(ast::TokenTree::cast)
}
