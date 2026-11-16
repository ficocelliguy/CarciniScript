//! `jsx!` (the bundled `react` crate): JSX-shaped markup rewritten into calls of the react
//! prelude, before compiling, so the expressions in it are checked and compiled as ordinary Rust.
//!
//! ```text
//! jsx! { <div class="row" on_click={move |_| set.update(|n| n + 1)}>"clicks: " {n}</div> }
//! ```
//!
//! - A tag that names a fn or a closure in scope (`fn gauge(..)`, `let cat = move || ..`) is a
//!   component: its attributes are its arguments, by parameter name (a `children` parameter gets
//!   the children as a `Vec<Node>`). Any other tag is an HTML element. `<>..</>` is a fragment.
//! - Attributes: `name="text"`, `name={expr}`, or a bare `name` (`true`). `class` is `className`,
//!   `for` `htmlFor`, `snake_case` names camelCase (`on_click` is `onClick`), and `aria-label` /
//!   `data-x` stay as written. An `on*` attribute takes a closure of the event (`|e: JsValue|`).
//!   `style="color: red"` becomes a style object when it renders. `key` keys the element.
//! - Children: string literals (text is quoted: Rust's lexer drops whitespace and can't read
//!   `don't`), `{expr}` (a node, text, a number, or an `Option` / `Vec` of them), and elements.
//! - `jsx!` calls inside `{..}` are rewritten too.

use ra_ap_syntax::{
    AstNode, SyntaxElement, SyntaxKind, SyntaxNode, TextRange, TextSize,
    ast::{self, HasName},
};

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
        roots.push(p.node()?);
    }
    match roots.len() {
        0 => Err((
            tt.syntax().text_range(),
            "`jsx!` needs an element: `jsx! { <div>..</div> }`".into(),
        )),
        1 => Ok(roots.pop().unwrap_or_default()),
        // (Several roots: a fragment.)
        _ => Ok(format!("__rx_h(\"\", vec![], vec![{}])", roots.join(", "))),
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

    /// One child: an element, a string literal, or `{expr}`.
    fn node(&mut self) -> Res<String> {
        let Some(e) = self.peek().cloned() else {
            return self.err("unexpected end of `jsx!`");
        };
        if self.at("<") {
            return self.element();
        }
        if e.kind() == SyntaxKind::STRING {
            self.i += 1;
            return Ok(format!("__rx_child({e})"));
        }
        if let Some(tt) = e.as_node().cloned().and_then(ast::TokenTree::cast)
            && tt
                .syntax()
                .first_token()
                .is_some_and(|t| t.kind() == SyntaxKind::L_CURLY)
        {
            self.i += 1;
            let inner = crate::bundled::tt_items(&tt);
            if inner.is_empty() {
                return Ok("__rx_child(())".into());
            }
            return Ok(format!("__rx_child({})", expr_text(&inner, self.text)?));
        }
        self.err(format!(
            "text in `jsx!` is written as a string literal: `\"{}\"` (Rust's lexer drops the spaces in bare text)",
            e
        ))
    }

    /// A tag's or attribute's name: idents and keywords joined by `-` (`aria-label`).
    fn name(&mut self) -> Res<String> {
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
            if self.at("-")
                && self
                    .peek_text(1)
                    .is_some_and(|t| t.chars().all(|c| c.is_alphanumeric() || c == '_'))
            {
                out.push('-');
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
            return Ok(format!(
                "__rx_h(\"\", vec![], vec![{}])",
                children.join(", ")
            ));
        }
        let tag = self.name()?;
        let mut attrs: Vec<(String, TextRange, String)> = Vec::new();
        let mut key: Option<String> = None;
        while !self.at(">") && !self.at("/") {
            if self.peek().is_none() {
                return Err((open, format!("`<{tag}` isn't closed")));
            }
            let at = self.range();
            let name = self.name()?;
            let value = if self.at("=") {
                self.i += 1;
                self.attr_value()?
            } else {
                "true".into()
            };
            if name == "key" {
                key = Some(value);
            } else {
                attrs.push((name, at, value));
            }
        }
        let children = if self.at("/") {
            self.i += 1;
            self.expect(">", "to end the self-closing tag")?;
            Vec::new()
        } else {
            self.i += 1;
            self.children(&tag)?
        };
        let key = key.map_or("__rx_attr(())".to_string(), |k| format!("__rx_attr({k})"));

        match component_params(&tag, &self.scope) {
            Lookup::Component(params) => {
                return self.component(&tag, open, params, attrs, key, children);
            }
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
        let mut props = vec![format!("(\"key\", {key})")];
        for (name, _, value) in attrs {
            let js = attr_name(&name);
            let value = if is_handler(&js) {
                format!("__rx_handler({value})")
            } else {
                format!("__rx_attr({value})")
            };
            props.push(format!("({js:?}, {value})"));
        }
        Ok(format!(
            "__rx_h({tag:?}, vec![{}], vec![{}])",
            props.join(", "),
            children.join(", ")
        ))
    }

    fn component(
        &self,
        tag: &str,
        open: TextRange,
        params: Vec<String>,
        attrs: Vec<(String, TextRange, String)>,
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
        for (name, at, _) in &attrs {
            if !params.contains(name) {
                return Err((
                    *at,
                    format!(
                        "`{tag}` has no parameter `{name}` (it takes: {})",
                        list(&params)
                    ),
                ));
            }
        }
        let mut args = Vec::new();
        for p in &params {
            if p == "children" {
                args.push(format!("__rx_children(vec![{}])", children.join(", ")));
            } else if let Some((_, _, v)) = attrs.iter().find(|(n, _, _)| n == p) {
                args.push(v.clone());
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
        let mut out = format!(
            "__rx_c{}({tag}, {key}, vec![{}]",
            params.len(),
            names.join(", ")
        );
        for a in args {
            out.push_str(", ");
            out.push_str(&a);
        }
        out.push(')');
        Ok(out)
    }

    fn attr_value(&mut self) -> Res<String> {
        let Some(e) = self.peek().cloned() else {
            return self.err("expected an attribute value");
        };
        self.i += 1;
        if let Some(tt) = e.as_node().cloned().and_then(ast::TokenTree::cast) {
            let inner = crate::bundled::tt_items(&tt);
            if inner.is_empty() {
                return Err((e.text_range(), "empty `{}` attribute".into()));
            }
            return Ok(format!("({})", expr_text(&inner, self.text)?));
        }
        match e.kind() {
            SyntaxKind::STRING | SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER => {
                Ok(e.to_string())
            }
            SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW => Ok(e.to_string()),
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
                    let close = self.name()?;
                    if close != tag {
                        return Err((at, format!("`</{close}>` closes `<{tag}>`")));
                    }
                    self.expect(">", "")?;
                }
                return Ok(out);
            }
            out.push(self.node()?);
        }
    }
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

enum Lookup {
    /// A fn or closure: its parameter names.
    Component(Vec<String>),
    /// Bound to something else.
    Other,
    None,
}

/// A closure, or a block or parentheses ending in one: its parameter names.
fn closure_params(e: ast::Expr) -> Option<Vec<String>> {
    match e {
        ast::Expr::ClosureExpr(c) => Some(fn_params(c.param_list())),
        ast::Expr::BlockExpr(b) => b.stmt_list()?.tail_expr().and_then(closure_params),
        ast::Expr::ParenExpr(p) => p.expr().and_then(closure_params),
        _ => None,
    }
}

/// A component's parameter names: a fn of the file or a block around `at`, or a closure bound
/// by a `let` before it.
fn component_params(tag: &str, at: &SyntaxNode) -> Lookup {
    if tag.contains("::") || tag.contains('-') {
        return Lookup::None;
    }
    let pos: TextSize = at.text_range().start();
    for anc in at.ancestors() {
        let stmts: Vec<SyntaxNode> = if let Some(sl) = ast::StmtList::cast(anc.clone()) {
            sl.syntax().children().collect()
        } else if let Some(sf) = ast::SourceFile::cast(anc.clone()) {
            sf.syntax().children().collect()
        } else {
            continue;
        };
        let mut found = None;
        for s in stmts {
            if let Some(f) = ast::Fn::cast(s.clone())
                && f.name().is_some_and(|n| n.text() == tag)
            {
                found = Some(Lookup::Component(fn_params(f.param_list())));
            }
            if s.text_range().start() >= pos {
                continue;
            }
            if let Some(l) = ast::LetStmt::cast(s.clone())
                && let Some(ast::Pat::IdentPat(ip)) = l.pat()
                && ip.name().is_some_and(|n| n.text() == tag)
            {
                found = Some(match l.initializer().and_then(closure_params) {
                    Some(params) => Lookup::Component(params),
                    None => Lookup::Other,
                });
            }
        }
        if let Some(found) = found {
            return found;
        }
    }
    // A `pub fn` component of a `mod` file (`use ui::Gauge;`).
    MOD_SOURCES.with(|m| {
        for src in m.borrow().iter() {
            let file = ra_ap_syntax::SourceFile::parse(src, crate::macros::EDITION).tree();
            if let Some(f) = file
                .syntax()
                .descendants()
                .filter_map(ast::Fn::cast)
                .find(|f| f.name().is_some_and(|n| n.text() == tag))
            {
                return Lookup::Component(fn_params(f.param_list()));
            }
        }
        Lookup::None
    })
}

thread_local! {
    /// The sources of the `mod` files of the file being expanded, where components are found too.
    static MOD_SOURCES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The `mod name;` files of `text` (read with `resolver`), for components defined in them.
pub(crate) fn set_mod_sources(text: &str, resolver: crate::typeck::Resolver) {
    let file = ra_ap_syntax::SourceFile::parse(text, crate::macros::EDITION).tree();
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

fn fn_params(pl: Option<ast::ParamList>) -> Vec<String> {
    let Some(pl) = pl else { return Vec::new() };
    pl.params()
        .enumerate()
        .map(|(i, p)| match p.pat() {
            Some(ast::Pat::IdentPat(ip)) => {
                ip.name().map_or(format!("_{i}"), |n| n.text().to_string())
            }
            _ => format!("_{i}"),
        })
        .collect()
}

/// An attribute's React name.
fn attr_name(name: &str) -> String {
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

/// `onClick`, `onChange`, ..
fn is_handler(js: &str) -> bool {
    js.len() > 2 && js.starts_with("on") && js[2..].starts_with(|c: char| c.is_uppercase())
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
