//! `jsx!` (the bundled `react` crate): JSX-shaped markup rewritten into calls of the react
//! prelude, before compiling, so the expressions in it are checked and compiled as ordinary Rust.
//!
//! ```text
//! jsx! { <div class="row" on_click={move |_| set.update(|n| n + 1)}>"clicks: " {n}</div> }
//! ```
//!
//! - A tag that names a fn or a closure in scope (`fn gauge(..)`, `let cat = move || ..`) is a
//!   component: its attributes are its arguments, by parameter name (a `children` parameter gets
//!   the children as a `Vec<Node>`; `{..s}` fills the others from the fields of `s`). Any other
//!   tag is an HTML element. `<>..</>` is a fragment; `<ctx.Provider value={..}>` provides a
//!   context.
//! - Attributes: `name="text"`, `name={expr}`, a bare `name` (`true`), or `{..pairs}` (an HTML
//!   element's). `class` is `className`, `for` `htmlFor`, `snake_case` names camelCase
//!   (`on_click` is `onClick`), and `aria-label` / `data-x` stay as written. Each takes what React
//!   gives it (`jsx_attrs`): an `on*` attribute a closure of the `js::Event` (or of nothing, or of a
//!   `JsValue`), `class` text, `disabled` a `bool`, `style` a CSS string or pairs, `ref` a
//!   `react::Ref`; an attribute React doesn't know is an error. `key` keys the element.
//! - Children: string literals (text is quoted: Rust's lexer drops whitespace and can't read
//!   `don't`), `{expr}` (`impl IntoNode`: a node, text, a number, or an `Option` / `Vec` of them),
//!   and elements.
//! - `jsx!` calls inside `{..}` are rewritten too.
//!
//! Each embedded expression stays where it was written (a kept span, see `edits`), so errors,
//! hover and completions inside it see the source.

use ra_ap_syntax::{
    AstNode, SyntaxElement, SyntaxKind, SyntaxNode, TextRange, TextSize,
    ast::{self, HasName},
};

use crate::jsx_attrs::Kind;

/// Whether `mc` is `jsx!(..)` or `react::jsx!(..)`.
pub(crate) fn is_jsx_call(mc: &ast::MacroCall) -> bool {
    let Some(path) = mc.path() else { return false };
    let last = path
        .segment()
        .and_then(|s| s.name_ref())
        .is_some_and(|n| n.text() == "jsx");
    last && path
        .qualifier()
        .is_none_or(|q| q.syntax().text() == "react")
}

/// The Rust a `jsx!` call's token tree stands for.
pub(crate) fn rewrite(tt: &ast::TokenTree, text: &str) -> Result<String, (TextRange, String)> {
    let items = crate::bundled::tt_items(tt);
    let mut p = Parser {
        items: &items,
        i: 0,
        text,
        scope: tt.syntax().clone(),
    };
    let mut roots = Vec::new();
    while p.i < items.len() {
        if let Some(n) = p.node()? {
            roots.push(n);
        }
    }
    match roots.len() {
        0 => Err((
            tt.syntax().text_range(),
            "`jsx!` needs an element: `jsx! { <div>..</div> }`".into(),
        )),
        1 => Ok(roots.pop().unwrap_or_default()),
        // (Several roots: a fragment.)
        _ => Ok(format!("__rx_el(\"\", &[], &[{}])", roots.join(", "))),
    }
}

/// A recursive-descent reader of a `jsx!` call's tokens.
struct Parser<'a> {
    items: &'a [SyntaxElement],
    i: usize,
    text: &'a str,
    /// Where the call is: components are looked up from here.
    scope: SyntaxNode,
}

type Res<T> = Result<T, (TextRange, String)>;

/// An attribute as written.
struct Attr {
    /// As written (`on_click`, `aria-label`); `..` for a spread.
    name: String,
    at: TextRange,
    /// Its value's Rust: a literal, `true`, or `(expr)` (a kept span).
    value: String,
    /// A `{expr}` value's source text.
    expr: Option<String>,
}

/// A kept span of the source, `range` (see `edits`).
fn keep(range: TextRange) -> String {
    format!(
        "{KEEP_OPEN}{}:{}{KEEP_CLOSE}",
        u32::from(range.start()),
        u32::from(range.end())
    )
}

impl Parser<'_> {
    fn peek(&self) -> Option<&SyntaxElement> {
        self.items.get(self.i)
    }
    fn peek_text(&self, k: usize) -> Option<String> {
        self.items.get(self.i + k).map(|e| e.to_string())
    }
    fn at(&self, s: &str) -> bool {
        self.peek_text(0).as_deref() == Some(s)
    }
    fn range(&self) -> TextRange {
        self.peek()
            .map(|e| e.text_range())
            .or_else(|| self.items.last().map(|e| e.text_range()))
            .unwrap_or_default()
    }
    fn err<T>(&self, msg: impl Into<String>) -> Res<T> {
        Err((self.range(), msg.into()))
    }
    fn expect(&mut self, s: &str, what: &str) -> Res<()> {
        if self.at(s) {
            self.i += 1;
            Ok(())
        } else {
            self.err(format!("expected `{s}` {what}"))
        }
    }

    /// One child: an element, a string literal, or `{expr}` (`None` for an empty `{}`).
    fn node(&mut self) -> Res<Option<String>> {
        let Some(e) = self.peek().cloned() else {
            return self.err("unexpected end of `jsx!`");
        };
        if self.at("<") {
            return self.element().map(Some);
        }
        if e.kind() == SyntaxKind::STRING {
            self.i += 1;
            return Ok(Some(format!("__rx_child({})", keep(e.text_range()))));
        }
        if let Some(tt) = curly(&e) {
            self.i += 1;
            let inner = crate::bundled::tt_items(&tt);
            // (`{}` or `{/* a comment */}`: nothing.)
            if inner.is_empty() {
                return Ok(None);
            }
            if spread_start(&inner).is_some() {
                return Err((
                    e.text_range(),
                    "`{..spread}` goes among an element's attributes, not its children".into(),
                ));
            }
            return Ok(Some(format!(
                "__rx_child({})",
                expr_text(&inner, self.text)?
            )));
        }
        self.err(format!(
            "text in `jsx!` is written as a string literal: `\"{}\"` (Rust's lexer drops the spaces in bare text)",
            e
        ))
    }

    /// A tag's or attribute's name: idents and keywords joined by `-` (`aria-label`), or by `.`
    /// in a tag (`theme.Provider`).
    fn name(&mut self, dots: bool) -> Res<String> {
        let mut out = String::new();
        loop {
            let Some(e) = self.peek() else {
                return self.err("expected a name");
            };
            let ok = e.kind() == SyntaxKind::IDENT || e.kind().is_keyword(crate::macros::EDITION);
            if !ok {
                return self.err(format!("expected a name, found `{e}`"));
            }
            out.push_str(&e.to_string());
            self.i += 1;
            let joined = self.at("-") || (dots && self.at("."));
            if joined
                && self
                    .peek_text(1)
                    .is_some_and(|t| t.chars().all(|c| c.is_alphanumeric() || c == '_'))
            {
                out.push_str(&self.peek_text(0).unwrap_or_default());
                self.i += 1;
                continue;
            }
            return Ok(out);
        }
    }

    fn element(&mut self) -> Res<String> {
        let open = self.range();
        self.expect("<", "")?;
        // `<>..</>`
        if self.at(">") {
            self.i += 1;
            let children = self.children("")?;
            return Ok(format!("__rx_el(\"\", &[], &[{}])", children.join(", ")));
        }
        let tag_at = self.range();
        let tag = self.name(true)?;
        let tag_at = TextRange::new(tag_at.start(), self.items[self.i - 1].text_range().end());
        let mut attrs: Vec<Attr> = Vec::new();
        while !self.at(">") && !self.at("/") {
            if self.peek().is_none() {
                return Err((open, format!("`<{tag}` isn't closed")));
            }
            let at = self.range();
            // `{..pairs}`
            if let Some(tt) = self.peek().and_then(curly) {
                let inner = crate::bundled::tt_items(&tt);
                let Some(k) = spread_start(&inner) else {
                    return Err((
                        at,
                        "an attribute is `name=value`, or `{..spread}` (an expression in braces goes among the children)".into(),
                    ));
                };
                if k == inner.len() {
                    return Err((at, "`{..}` needs a value to spread: `{..props}`".into()));
                }
                self.i += 1;
                let value = expr_text(&inner[k..], self.text)?;
                attrs.push(Attr {
                    name: "..".into(),
                    at,
                    value,
                    expr: None,
                });
                continue;
            }
            let name = self.name(false)?;
            let (value, expr) = if self.at("=") {
                self.i += 1;
                self.attr_value()?
            } else {
                ("true".into(), None)
            };
            attrs.push(Attr {
                name,
                at: TextRange::new(at.start(), self.items[self.i - 1].text_range().end()),
                value,
                expr,
            });
        }
        let children = if self.at("/") {
            self.i += 1;
            self.expect(">", "to end the self-closing tag")?;
            Vec::new()
        } else {
            self.i += 1;
            self.children(&tag)?
        };
        let key = attrs
            .iter()
            .find(|a| a.name == "key")
            .map_or("__rx_attr(())".to_string(), |k| {
                format!("__rx_key({})", k.value)
            });

        if let Some(ctx) = tag.strip_suffix(".Provider") {
            return self.provider(ctx, tag_at, open, attrs, key, children);
        }
        if tag.contains('.') {
            return Err((
                tag_at,
                format!(
                    "`<{tag}>`: a dotted tag is a context's provider, `<ctx.Provider value={{..}}>`"
                ),
            ));
        }
        match component_params(&tag, &self.scope) {
            Lookup::Component(params) => {
                let names = params.into_iter().map(|p| p.name).collect();
                return self.component(&tag, tag_at, open, names, attrs, key, children);
            }
            // (`<input>` where a local is named `input`.)
            Lookup::Other if crate::jsx_attrs::is_html_tag(&tag) => {}
            Lookup::Other => {
                return Err((
                    open,
                    format!(
                        "`{tag}` is bound here, but not to a fn or closure `jsx!` can see (a component is a fn, or a `let` of a closure, or of a block ending in one)"
                    ),
                ));
            }
            Lookup::None => {}
        }
        if tag.starts_with(|c: char| c.is_uppercase()) {
            return Err((
                open,
                format!(
                    "`{tag}` isn't a fn or closure in scope (a component is a fn or closure returning a `react::Node`; HTML tags are lowercase)"
                ),
            ));
        }
        let mut props = Vec::new();
        for a in attrs {
            if a.name == ".." {
                props.push(format!("(\"...\", __rx_spread({}))", a.value));
                continue;
            }
            let js = attr_name(&a.name);
            let Some(kind) = crate::jsx_attrs::kind(&tag, &js) else {
                return Err((a.at, unknown_attr(&tag, &a.name, &js)));
            };
            let value = match kind {
                Kind::Handler => match &a.expr {
                    Some(e) => format!("{}({})", handler_fn(e), a.value),
                    None => {
                        return Err((
                            a.at,
                            format!("`{}` takes a closure: `{}={{move |e| ..}}`", a.name, a.name),
                        ));
                    }
                },
                Kind::Text => format!("__rx_text({})", a.value),
                Kind::TextOrNum => format!("__rx_num({})", a.value),
                Kind::Bool => format!("__rx_bool({})", a.value),
                Kind::Style => format!("__rx_style({})", a.value),
                Kind::Key => format!("__rx_key({})", a.value),
                Kind::Ref => format!("__rx_ref(&({}))", a.value),
                Kind::Any => format!("__rx_attr({})", a.value),
            };
            props.push(format!("({js:?}, {value})"));
        }
        Ok(format!(
            "__rx_el({tag:?}, &[{}], &[{}])",
            props.join(", "),
            children.join(", ")
        ))
    }

    /// `<ctx.Provider value={..}>children</ctx.Provider>`
    fn provider(
        &self,
        ctx: &str,
        tag_at: TextRange,
        open: TextRange,
        attrs: Vec<Attr>,
        key: String,
        children: Vec<String>,
    ) -> Res<String> {
        let mut value = None;
        for a in &attrs {
            match a.name.as_str() {
                "value" => value = Some(a.value.clone()),
                "key" => {}
                _ => {
                    return Err((
                        a.at,
                        format!(
                            "`<{ctx}.Provider>` takes `value` (and `key`), not `{}`",
                            a.name
                        ),
                    ));
                }
            }
        }
        let Some(value) = value else {
            return Err((open, format!("`<{ctx}.Provider>` needs `value=..`")));
        };
        // (The context's name stays where it's written, for hover and errors.)
        let ctx_code = if ctx.contains('.') {
            ctx.to_string()
        } else {
            keep(TextRange::at(tag_at.start(), TextSize::of(ctx)))
        };
        Ok(format!(
            "__rx_provide(&{ctx_code}, {key}, {value}, &[{}])",
            children.join(", ")
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn component(
        &self,
        tag: &str,
        tag_at: TextRange,
        open: TextRange,
        params: Vec<String>,
        attrs: Vec<Attr>,
        key: String,
        children: Vec<String>,
    ) -> Res<String> {
        if params.len() > 6 {
            return Err((
                open,
                format!(
                    "components take up to 6 parameters; `{tag}` has {}",
                    params.len()
                ),
            ));
        }
        let spread = attrs.iter().find(|a| a.name == "..");
        for a in &attrs {
            if a.name != ".." && a.name != "key" && !params.contains(&a.name) {
                return Err((
                    a.at,
                    format!(
                        "`{tag}` has no parameter `{}` (it takes: {})",
                        a.name,
                        list(&params)
                    ),
                ));
            }
        }
        let mut args = Vec::new();
        for p in &params {
            if p == "children" {
                args.push(format!("__rx_children(&[{}])", children.join(", ")));
            } else if let Some(a) = attrs.iter().find(|a| &a.name == p) {
                args.push(a.value.clone());
            } else if spread.is_some() {
                // (From the spread value's field of that name.)
                args.push(format!("__rx_s.{p}.clone()"));
            } else {
                return Err((open, format!("`<{tag}>` needs `{p}=..`")));
            }
        }
        if !children.is_empty() && !params.iter().any(|p| p == "children") {
            return Err((
                open,
                format!("`{tag}` takes no children (give it a `children: Vec<Node>` parameter)"),
            ));
        }
        let names: Vec<String> = params.iter().map(|p| format!("{p:?}")).collect();
        let comp = match spread {
            Some(_) => "__rx_f".to_string(),
            None => keep(tag_at),
        };
        let mut out = format!(
            "__rx_c{}({comp}, {key}, &[{}]",
            params.len(),
            names.join(", ")
        );
        for a in args {
            out.push_str(", ");
            out.push_str(&a);
        }
        out.push(')');
        if let Some(s) = spread {
            // (The spread value once; the component first, as written.)
            out = format!(
                "{{ let __rx_f = {}; let __rx_s = {}; {out} }}",
                keep(tag_at),
                s.value
            );
        }
        Ok(out)
    }

    /// `="text"`, `={expr}`, or a number or `bool` literal: its Rust, and the expression's text.
    fn attr_value(&mut self) -> Res<(String, Option<String>)> {
        let Some(e) = self.peek().cloned() else {
            return self.err("expected an attribute value");
        };
        self.i += 1;
        if let Some(tt) = e.as_node().cloned().and_then(ast::TokenTree::cast) {
            let inner = crate::bundled::tt_items(&tt);
            let (Some(first), Some(last)) = (inner.first(), inner.last()) else {
                return Err((e.text_range(), "empty `{}` attribute".into()));
            };
            let src = self.text
                [usize::from(first.text_range().start())..usize::from(last.text_range().end())]
                .to_string();
            return Ok((expr_text(&inner, self.text)?, Some(src)));
        }
        // (Kept where it's written, so a type error points at it.)
        match e.kind() {
            SyntaxKind::STRING
            | SyntaxKind::INT_NUMBER
            | SyntaxKind::FLOAT_NUMBER
            | SyntaxKind::TRUE_KW
            | SyntaxKind::FALSE_KW => Ok((keep(e.text_range()), None)),
            _ => Err((
                e.text_range(),
                format!("an attribute's value is a literal or `{{expr}}`, not `{e}`"),
            )),
        }
    }

    /// Children up to `</tag>`.
    fn children(&mut self, tag: &str) -> Res<Vec<String>> {
        let mut out = Vec::new();
        loop {
            if self.peek().is_none() {
                return self.err(format!("missing `</{tag}>`"));
            }
            if self.at("<") && self.peek_text(1).as_deref() == Some("/") {
                self.i += 2;
                if tag.is_empty() {
                    self.expect(">", "to close the fragment")?;
                } else {
                    let at = self.range();
                    let close = self.name(true)?;
                    if close != tag {
                        return Err((at, format!("`</{close}>` closes `<{tag}>`")));
                    }
                    self.expect(">", "")?;
                }
                return Ok(out);
            }
            if let Some(n) = self.node()? {
                out.push(n);
            }
        }
    }
}

/// A `{..}` group.
fn curly(e: &SyntaxElement) -> Option<ast::TokenTree> {
    e.as_node()
        .cloned()
        .and_then(ast::TokenTree::cast)
        .filter(|tt| {
            tt.syntax()
                .first_token()
                .is_some_and(|t| t.kind() == SyntaxKind::L_CURLY)
        })
}

/// Where the value of a `{..spread}` group's tokens starts (after its `..`).
fn spread_start(inner: &[SyntaxElement]) -> Option<usize> {
    match inner {
        [a, ..] if a.kind() == SyntaxKind::DOT2 => Some(1),
        [a, b, ..] if a.kind() == SyntaxKind::DOT && b.kind() == SyntaxKind::DOT => Some(2),
        _ => None,
    }
}

/// The prelude function an `on*` attribute's closure goes through: by its parameters (none, a
/// `JsValue`, or the event).
fn handler_fn(expr: &str) -> &'static str {
    let file =
        ra_ap_syntax::SourceFile::parse(&format!("fn f() {{ {expr} }}"), crate::macros::EDITION)
            .tree();
    let closure = file
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
        .and_then(|f| f.body())
        .and_then(|b| b.stmt_list())
        .and_then(|s| s.tail_expr())
        .and_then(|e| match e {
            ast::Expr::ClosureExpr(c) => Some(c),
            _ => None,
        });
    let Some(c) = closure else { return "__rx_on" };
    let params: Vec<ast::Param> = c
        .param_list()
        .map(|l| l.params().collect())
        .unwrap_or_default();
    match params.as_slice() {
        [] => "__rx_on0",
        [p] if p.ty().is_some_and(|t| {
            let t = t.syntax().text().to_string();
            t == "JsValue" || t.ends_with("::JsValue")
        }) =>
        {
            "__rx_on_js"
        }
        _ => "__rx_on",
    }
}

/// The error for an attribute `<tag>` doesn't have (React's name `js`, written `name`).
fn unknown_attr(tag: &str, name: &str, js: &str) -> String {
    let names: Vec<String> = crate::jsx_attrs::names()
        .chain(crate::jsx_attrs::events())
        .map(str::to_string)
        .collect();
    let hint = match crate::bindings::suggest(js, names.iter()) {
        Some(s) if name.contains('_') => format!(" (did you mean `{}`?)", snake(&s)),
        Some(s) => format!(" (did you mean `{s}`?)"),
        None => String::new(),
    };
    format!(
        "`<{tag}>` has no attribute `{name}`{hint}; `data-*` and `aria-*` attributes take anything, and `{{..pairs}}` passes any"
    )
}

/// React's `tabIndex` as `tab_index` (`dangerouslySetInnerHTML` stays as it is).
pub(crate) fn snake(js: &str) -> String {
    if js
        .as_bytes()
        .windows(2)
        .any(|w| w[0].is_ascii_uppercase() && w[1].is_ascii_uppercase())
    {
        return js.into();
    }
    let mut out = String::new();
    for c in js.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    match out.as_str() {
        "class_name" => "class".into(),
        "html_for" => "for".into(),
        _ => out,
    }
}

thread_local! {
    static LENIENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `f()`, with a `jsx!` call that has a mistake rewritten into its expressions instead of failing
/// (the language service analyzes a file being typed).
pub(crate) fn leniently<T>(f: impl FnOnce() -> T) -> T {
    let was = LENIENT.with(|l| l.replace(true));
    let out = f();
    LENIENT.with(|l| l.set(was));
    out
}

pub(crate) fn is_lenient() -> bool {
    LENIENT.with(|l| l.get())
}

/// A `jsx!` call with a mistake, for the language service: the expressions in its braces, each
/// evaluated and dropped, then an empty node.
pub(crate) fn lenient(tt: &ast::TokenTree, text: &str) -> String {
    let mut out = String::from("({ ");
    for e in crate::bundled::tt_items(tt) {
        let Some(g) = curly(&e) else { continue };
        let inner = crate::bundled::tt_items(&g);
        let from = spread_start(&inner).unwrap_or(0);
        if from < inner.len()
            && let Ok(code) = expr_text(&inner[from..], text)
        {
            out.push_str(&format!("let _ = {code}; "));
        }
    }
    out.push_str("__rx_el(\"\", &[], &[]) })");
    out
}

/// Opens and closes a span of the source kept as it is (`\u{E000}12:40\u{E001}`): `edits` turns a
/// rewrite holding them into edits around those spans, so diagnostics in an embedded expression
/// point into it.
pub(crate) const KEEP_OPEN: char = '\u{E000}';
pub(crate) const KEEP_CLOSE: char = '\u{E001}';

/// A rewrite of the call at `call` as edits: the text between kept spans replaces the source
/// between them, the spans stay. One edit for the whole call when the spans aren't in order.
pub(crate) fn edits(call: TextRange, code: &str, text: &str) -> Vec<(TextRange, String)> {
    // (literal text before a span, the span)
    let mut parts: Vec<(String, Option<TextRange>)> = Vec::new();
    let mut lit = String::new();
    let mut rest = code;
    while let Some(i) = rest.find(KEEP_OPEN) {
        lit.push_str(&rest[..i]);
        let after = &rest[i + KEEP_OPEN.len_utf8()..];
        let Some(j) = after.find(KEEP_CLOSE) else {
            break;
        };
        let span = after[..j].split_once(':').and_then(|(a, b)| {
            Some(TextRange::new(
                a.parse::<u32>().ok()?.into(),
                b.parse::<u32>().ok()?.into(),
            ))
        });
        let Some(span) = span else { break };
        parts.push((std::mem::take(&mut lit), Some(span)));
        rest = &after[j + KEEP_CLOSE.len_utf8()..];
    }
    lit.push_str(rest);
    parts.push((lit, None));
    let spans: Vec<TextRange> = parts.iter().filter_map(|(_, s)| *s).collect();
    let ordered = spans.windows(2).all(|w| w[0].end() <= w[1].start())
        && spans.first().is_none_or(|s| s.start() >= call.start())
        && spans.last().is_none_or(|s| s.end() <= call.end());
    if !ordered || spans.is_empty() {
        // (The spans written out as their source text.)
        return vec![(call, expand(code, text))];
    }
    let mut out = Vec::new();
    let mut from = call.start();
    for (lit, span) in parts {
        match span {
            Some(s) => {
                out.push((TextRange::new(from, s.start()), lit));
                from = s.end();
            }
            None => out.push((TextRange::new(from, call.end()), lit)),
        }
    }
    out
}

/// `code` with its kept spans written out as their source text.
pub(crate) fn expand(code: &str, text: &str) -> String {
    let mut out = String::new();
    let mut rest = code;
    while let Some(i) = rest.find(KEEP_OPEN) {
        out.push_str(&rest[..i]);
        let after = &rest[i + KEEP_OPEN.len_utf8()..];
        let Some(j) = after.find(KEEP_CLOSE) else {
            break;
        };
        if let Some((a, b)) = after[..j].split_once(':')
            && let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>())
        {
            out.push_str(&text[a..b]);
        }
        rest = &after[j + KEEP_CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// An expression's text, with the `jsx!` calls in it rewritten: a kept span when there are none.
fn expr_text(items: &[SyntaxElement], text: &str) -> Res<String> {
    let (Some(first), Some(last)) = (items.first(), items.last()) else {
        return Ok(String::new());
    };
    let start = first.text_range().start();
    let end = last.text_range().end();
    let mut edits: Vec<(TextRange, String)> = Vec::new();
    jsx_in(items, text, &mut edits)?;
    if edits.is_empty() {
        return Ok(format!(
            "{KEEP_OPEN}{}:{}{KEEP_CLOSE}",
            u32::from(start),
            u32::from(end)
        ));
    }
    edits.sort_by_key(|(r, _)| r.start());
    let mut out = String::new();
    let mut pos = start;
    for (r, s) in edits {
        if r.start() < pos {
            continue;
        }
        out.push_str(&text[usize::from(pos)..usize::from(r.start())]);
        out.push_str(&s);
        pos = r.end();
    }
    out.push_str(&text[usize::from(pos)..usize::from(end)]);
    Ok(out)
}

/// The `jsx!` calls among `items` (and in their groups), as edits.
fn jsx_in(items: &[SyntaxElement], text: &str, edits: &mut Vec<(TextRange, String)>) -> Res<()> {
    let mut i = 0;
    while i < items.len() {
        let e = &items[i];
        let is_name = e.kind() == SyntaxKind::IDENT && e.to_string() == "jsx";
        if is_name
            && items
                .get(i + 1)
                .is_some_and(|b| b.kind() == SyntaxKind::BANG)
            && let Some(tt) = items
                .get(i + 2)
                .and_then(|t| t.as_node().cloned())
                .and_then(ast::TokenTree::cast)
        {
            // (`react::jsx!`: from the `react`.)
            let from = if i >= 2
                && items[i - 1].to_string() == "::"
                && items[i - 2].to_string() == "react"
            {
                items[i - 2].text_range().start()
            } else {
                e.text_range().start()
            };
            let r = TextRange::new(from, tt.syntax().text_range().end());
            edits.push((r, format!("({})", rewrite(&tt, text)?)));
            i += 3;
            continue;
        }
        if let Some(tt) = e.as_node().cloned().and_then(ast::TokenTree::cast) {
            jsx_in(&crate::bundled::tt_items(&tt), text, edits)?;
        }
        i += 1;
    }
    Ok(())
}

/// A component's parameter: its name, and its type as written (`""` if it has none).
pub(crate) struct Param {
    pub(crate) name: String,
    pub(crate) ty: String,
}

pub(crate) enum Lookup {
    /// A fn or closure: its parameters.
    Component(Vec<Param>),
    /// Bound to something else.
    Other,
    None,
}

/// A closure, or a block or parentheses ending in one: its parameters.
fn closure_params(e: ast::Expr) -> Option<Vec<Param>> {
    match e {
        ast::Expr::ClosureExpr(c) => Some(fn_params(c.param_list())),
        ast::Expr::BlockExpr(b) => b.stmt_list()?.tail_expr().and_then(closure_params),
        ast::Expr::ParenExpr(p) => p.expr().and_then(closure_params),
        _ => None,
    }
}

/// A fn or `let` a tag may name.
pub(crate) enum Def {
    Fn(ast::Fn),
    Let(ast::LetStmt),
}

/// The fns and `let`s visible at `at`, by name: the innermost block's first (within a block,
/// later ones first: they shadow), then the file's, then the `pub fn`s of the `mod` files.
pub(crate) fn defs_in_scope(at: &SyntaxNode) -> Vec<(String, Def)> {
    let pos: TextSize = at.text_range().start();
    let mut out = Vec::new();
    for anc in at.ancestors() {
        let stmts: Vec<SyntaxNode> = if let Some(sl) = ast::StmtList::cast(anc.clone()) {
            sl.syntax().children().collect()
        } else if let Some(sf) = ast::SourceFile::cast(anc.clone()) {
            sf.syntax().children().collect()
        } else {
            continue;
        };
        let mut scope = Vec::new();
        for s in stmts {
            if let Some(f) = ast::Fn::cast(s.clone())
                && let Some(n) = f.name()
            {
                scope.push((n.text().to_string(), Def::Fn(f)));
            }
            if s.text_range().start() >= pos {
                continue;
            }
            if let Some(l) = ast::LetStmt::cast(s.clone())
                && let Some(ast::Pat::IdentPat(ip)) = l.pat()
                && let Some(n) = ip.name()
            {
                scope.push((n.text().to_string(), Def::Let(l)));
            }
        }
        scope.reverse();
        out.extend(scope);
    }
    // A `pub fn` component of a `mod` file (`use ui::Gauge;`).
    MOD_SOURCES.with(|m| {
        for src in m.borrow().iter() {
            let file = crate::macros::parse_tree(src);
            for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
                if let Some(n) = f.name() {
                    out.push((n.text().to_string(), Def::Fn(f)));
                }
            }
        }
    });
    out
}

impl Def {
    /// A component's parameters, or `None` for a `let` of something other than a closure.
    pub(crate) fn params(&self) -> Option<Vec<Param>> {
        match self {
            Def::Fn(f) => Some(fn_params(f.param_list())),
            Def::Let(l) => l.initializer().and_then(closure_params),
        }
    }
}

/// A component's parameters: a fn of the file or a block around `at`, or a closure bound by a
/// `let` before it.
pub(crate) fn component_params(tag: &str, at: &SyntaxNode) -> Lookup {
    if tag.contains("::") || tag.contains('-') {
        return Lookup::None;
    }
    match defs_in_scope(at).into_iter().find(|(n, _)| n == tag) {
        Some((_, d)) => match d.params() {
            Some(params) => Lookup::Component(params),
            None => Lookup::Other,
        },
        None => Lookup::None,
    }
}

thread_local! {
    /// The sources of the `mod` files of the file being expanded, where components are found too.
    static MOD_SOURCES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The `mod name;` files of `text` (read with `resolver`), for components defined in them.
pub(crate) fn set_mod_sources(text: &str, resolver: crate::typeck::Resolver) {
    let file = crate::macros::parse_tree(text);
    let mut out = Vec::new();
    for m in file.syntax().descendants().filter_map(ast::Module::cast) {
        if m.item_list().is_some() {
            continue;
        }
        let Some(name) = m.name().map(|n| n.text().to_string()) else {
            continue;
        };
        if let Some(src) =
            resolver(&format!("{name}.rs")).or_else(|| resolver(&format!("{name}/mod.rs")))
        {
            out.push(src);
        }
    }
    MOD_SOURCES.with(|m| *m.borrow_mut() = out);
}

fn fn_params(pl: Option<ast::ParamList>) -> Vec<Param> {
    let Some(pl) = pl else { return Vec::new() };
    pl.params()
        .enumerate()
        .map(|(i, p)| Param {
            name: match p.pat() {
                Some(ast::Pat::IdentPat(ip)) => {
                    ip.name().map_or(format!("_{i}"), |n| n.text().to_string())
                }
                _ => format!("_{i}"),
            },
            ty: p
                .ty()
                .map(|t| t.syntax().text().to_string())
                .unwrap_or_default(),
        })
        .collect()
}

/// An attribute's React name.
pub(crate) fn attr_name(name: &str) -> String {
    match name {
        "class" => return "className".into(),
        "for" => return "htmlFor".into(),
        _ => {}
    }
    if name.contains('-') {
        return name.into();
    }
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' && !out.is_empty() {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn list(names: &[String]) -> String {
    if names.is_empty() {
        "nothing".into()
    } else {
        names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
