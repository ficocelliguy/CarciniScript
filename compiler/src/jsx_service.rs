//! The language service in `jsx!` markup, which isn't Rust the checker sees (the embedded
//! `{expr}`s are, and the service handles them as usual): hover on a tag (a component's
//! signature) or an attribute (a component's parameter, or what an HTML attribute takes), and
//! completions of tag names and attributes.

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxElement, SyntaxKind, SyntaxNode, TextRange, TextSize, ast,
};

use crate::jsx::{self, Def};
use crate::jsx_attrs::{self, Kind};
use crate::service::Completion;
use crate::typeck::Resolver;

/// Where in markup an offset is.
enum Spot {
    /// A tag's name (`<gauge`, `</div>`), or one being typed after `<`.
    Tag { name: String, range: TextRange },
    /// An attribute's name in `<tag ..>` (`given`: the tag's other attributes).
    Attr {
        tag: String,
        name: String,
        range: TextRange,
        given: Vec<String>,
    },
}

/// Whether `tt` is a `jsx!` call's tokens (also one written inside another's `{..}`).
fn is_body(tt: &ast::TokenTree) -> bool {
    if let Some(mc) = tt.syntax().parent().and_then(ast::MacroCall::cast) {
        return jsx::is_jsx_call(&mc);
    }
    let mut prev = Vec::new();
    let mut e = tt.syntax().prev_sibling_or_token();
    while let Some(x) = e {
        if !x.kind().is_trivia() {
            prev.push(x.to_string());
            if prev.len() == 2 {
                break;
            }
        }
        e = x.prev_sibling_or_token();
    }
    prev == ["!", "jsx"]
}

fn is_name(e: &SyntaxElement) -> bool {
    e.kind() == SyntaxKind::IDENT || e.kind().is_keyword(crate::macros::EDITION)
}

/// The start of the `-`/`.`-joined name ending at `items[k]`.
fn name_start(items: &[SyntaxElement], mut k: usize) -> usize {
    while k >= 2 && matches!(items[k - 1].to_string().as_str(), "-" | ".") && is_name(&items[k - 2])
    {
        k -= 2;
    }
    k
}

/// The `-`/`.`-joined name starting at `items[k]`, and the index after it.
fn name_from(items: &[SyntaxElement], mut k: usize) -> (String, usize) {
    let mut out = String::new();
    while k < items.len() && is_name(&items[k]) {
        out.push_str(&items[k].to_string());
        k += 1;
        if k + 1 < items.len()
            && matches!(items[k].to_string().as_str(), "-" | ".")
            && is_name(&items[k + 1])
        {
            out.push_str(&items[k].to_string());
            k += 1;
        } else {
            break;
        }
    }
    (out, k)
}

/// The markup spot at `offset`, and the `jsx!` call's tokens (where names are looked up).
fn spot(file: &SourceFile, offset: TextSize) -> Option<(Spot, SyntaxNode)> {
    let toks = file.syntax().token_at_offset(offset);
    // (A name the offset is in or just after; else the token before it, to complete after `<`.)
    let tok = toks
        .clone()
        .find(|t| is_name(&SyntaxElement::Token(t.clone())))
        .or_else(|| toks.left_biased())?;
    let tt = tok.parent().and_then(ast::TokenTree::cast)?;
    if !is_body(&tt) {
        return None;
    }
    let items = crate::bundled::tt_items(&tt);
    let on_name = is_name(&SyntaxElement::Token(tok.clone()));
    // The item at the offset (a name), or the number of items before it.
    let (j, range, name) = if on_name {
        let k = items
            .iter()
            .position(|e| e.text_range() == tok.text_range())?;
        let j = name_start(&items, k);
        let range = TextRange::new(items[j].text_range().start(), tok.text_range().end());
        let name: String = items[j..=k].iter().map(|e| e.to_string()).collect();
        (j, range, name)
    } else {
        let j = items
            .iter()
            .take_while(|e| e.text_range().end() <= offset)
            .count();
        (j, TextRange::empty(offset), String::new())
    };
    let prev = |k: usize| (j >= k).then(|| items[j - k].to_string());
    if prev(1).as_deref() == Some("<")
        || (prev(1).as_deref() == Some("/") && prev(2).as_deref() == Some("<"))
    {
        return Some((Spot::Tag { name, range }, tt.syntax().clone()));
    }
    if prev(1).as_deref() == Some("=") {
        return None;
    }
    // In an open tag: back to its `<` (a `>` first: among the children).
    let mut t = j;
    loop {
        if t == 0 {
            return None;
        }
        t -= 1;
        match items[t].to_string().as_str() {
            ">" => return None,
            "<" => break,
            _ => {}
        }
    }
    let (tag, mut k) = name_from(&items, t + 1);
    if tag.is_empty() || k > j {
        return None;
    }
    let mut given = Vec::new();
    while k < items.len() && !matches!(items[k].to_string().as_str(), ">" | "/" | "<") {
        if is_name(&items[k]) {
            let (n, next) = name_from(&items, k);
            if k != j {
                given.push(n);
            }
            k = next;
        } else {
            k += 1;
        }
    }
    Some((
        Spot::Attr {
            tag,
            name,
            range,
            given,
        },
        tt.syntax().clone(),
    ))
}

fn code(s: &str) -> String {
    format!("```rust\n{s}\n```")
}

/// A fn's or closure's header as written (`fn gauge(label: &str) -> Node`), and its doc comment.
fn header(name: &str, d: &Def) -> (String, String) {
    match d {
        Def::Fn(f) => {
            let text = f.syntax().text().to_string();
            let start = f.syntax().text_range().start();
            let cut = f
                .body()
                .map(|b| usize::from(b.syntax().text_range().start() - start))
                .unwrap_or(text.len());
            let sig = text[..cut]
                .lines()
                .skip_while(|l| {
                    let l = l.trim_start();
                    l.starts_with("///") || l.starts_with("#[")
                })
                .collect::<Vec<_>>()
                .join("\n");
            (sig.trim_end().to_string(), crate::service::docs_of(f))
        }
        Def::Let(l) => {
            let params = d
                .params()
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    if p.ty.is_empty() {
                        p.name.clone()
                    } else {
                        format!("{}: {}", p.name, p.ty)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            let ret = l
                .initializer()
                .and_then(closure_of)
                .and_then(|c| c.ret_type())
                .map(|r| format!(" {}", r.syntax().text()))
                .unwrap_or_default();
            (format!("let {name} = |{params}|{ret}"), String::new())
        }
    }
}

/// A closure, or the one a block or parentheses end in.
fn closure_of(e: ast::Expr) -> Option<ast::ClosureExpr> {
    match e {
        ast::Expr::ClosureExpr(c) => Some(c),
        ast::Expr::BlockExpr(b) => b.stmt_list()?.tail_expr().and_then(closure_of),
        ast::Expr::ParenExpr(p) => p.expr().and_then(closure_of),
        _ => None,
    }
}

/// What an HTML attribute of `kind` takes.
fn describe(kind: Kind) -> &'static str {
    match kind {
        Kind::Handler => "impl Fn(js::Event)",
        Kind::Text => "text",
        Kind::TextOrNum => "a number or text",
        Kind::Bool => "bool",
        Kind::Style => "a CSS string, or (name, value) pairs",
        Kind::Key => "text or a number",
        Kind::Ref => "react::Ref",
        Kind::Any => "any value",
    }
}

fn component<'a>(defs: &'a [(String, Def)], tag: &str) -> Option<&'a Def> {
    defs.iter()
        .find(|(n, _)| n == tag)
        .map(|(_, d)| d)
        .filter(|d| d.params().is_some())
}

/// Hover at `offset` of `src` in markup: the range it describes, and markdown.
pub(crate) fn hover(
    src: &str,
    filename: &str,
    offset: usize,
    resolver: Resolver,
) -> Option<(TextRange, String)> {
    let file = crate::macros::parse_tree(src);
    let (spot, scope) = spot(&file, TextSize::from(offset as u32))?;
    jsx::set_mod_sources(src, filename, resolver);
    let defs = jsx::defs_in_scope(&scope);
    match spot {
        Spot::Tag { name, range } => {
            if let Some(d) = component(&defs, &name) {
                let (sig, docs) = header(&name, d);
                let mut out = format!("{}\n*component*", code(&sig));
                if !docs.is_empty() {
                    out.push_str("\n\n");
                    out.push_str(&docs);
                }
                return Some((range, out));
            }
            if let Some(ctx) = name.strip_suffix(".Provider") {
                return Some((
                    range,
                    format!(
                        "{}\n\nProvides `{ctx}` to the components inside (`use_context(&{ctx})`).",
                        code(&format!("<{name} value={{..}}>"))
                    ),
                ));
            }
            (!name.is_empty() && !name.starts_with(char::is_uppercase)).then(|| {
                (
                    range,
                    format!("{}\n\nAn HTML element.", code(&format!("<{name}>"))),
                )
            })
        }
        Spot::Attr {
            tag, name, range, ..
        } => {
            if let Some(d) = component(&defs, &tag) {
                let p = d.params()?.into_iter().find(|p| p.name == name)?;
                return Some((
                    range,
                    format!(
                        "{}\n\nA parameter of the component `{tag}`.",
                        code(&format!("{}: {}", p.name, p.ty))
                    ),
                ));
            }
            let js = jsx::attr_name(&name);
            let kind = jsx_attrs::kind(&tag, &js)?;
            let react = if js == name {
                String::new()
            } else {
                format!(" (React's `{js}`)")
            };
            Some((
                range,
                format!(
                    "{}\n\nAn attribute of `<{tag}>`{react}.",
                    code(&format!("{name}: {}", describe(kind)))
                ),
            ))
        }
    }
}

/// Completions at `offset` of `src` in markup: tag names after `<`, attributes in a tag.
pub(crate) fn completions(
    src: &str,
    filename: &str,
    offset: usize,
    resolver: Resolver,
) -> Option<Vec<Completion>> {
    let file = crate::macros::parse_tree(src);
    let at = TextSize::from(offset as u32);
    let (spot, scope) = spot(&file, at)?;
    jsx::set_mod_sources(src, filename, resolver);
    let defs = jsx::defs_in_scope(&scope);
    let mut out = Vec::new();
    let mut push = |label: String, kind: &'static str, detail: String| {
        out.push(Completion {
            label,
            kind,
            detail,
            documentation: String::new(),
        })
    };
    let prefix = |name: &str, range: TextRange| -> String {
        let n = usize::from(at.min(range.end()) - range.start());
        name.get(..n).unwrap_or(name).to_string()
    };
    match spot {
        Spot::Tag { name, range } => {
            let prefix = prefix(&name, range);
            let mut seen = std::collections::HashSet::new();
            for (n, d) in &defs {
                let is_component = match d {
                    Def::Fn(f) => f
                        .ret_type()
                        .is_some_and(|r| r.syntax().text().to_string().ends_with("Node")),
                    Def::Let(l) => l.initializer().and_then(closure_of).is_some(),
                };
                if is_component && n.starts_with(&prefix) && seen.insert(n.clone()) {
                    push(n.clone(), "function", header(n, d).0);
                }
            }
            for t in jsx_attrs::HTML_TAGS.split_whitespace() {
                if t.starts_with(&prefix) && seen.insert(t.to_string()) {
                    push(t.to_string(), "struct", format!("<{t}>"));
                }
            }
        }
        Spot::Attr {
            tag,
            name,
            range,
            given,
        } => {
            let prefix = prefix(&name, range);
            if let Some(d) = component(&defs, &tag) {
                for p in d.params().unwrap_or_default() {
                    if p.name != "children"
                        && !given.contains(&p.name)
                        && p.name.starts_with(&prefix)
                    {
                        push(p.name.clone(), "field", p.ty.clone());
                    }
                }
                if !given.iter().any(|g| g == "key") && "key".starts_with(&prefix) {
                    push("key".into(), "field", describe(Kind::Key).into());
                }
            } else if !tag.starts_with(char::is_uppercase) && !tag.contains('.') {
                let given: Vec<String> = given.iter().map(|g| jsx::attr_name(g)).collect();
                // (As written: `on_click`, or `onClick` once a capital is typed.)
                let camel = prefix.chars().any(|c| c.is_ascii_uppercase());
                for js in jsx_attrs::names().chain(jsx_attrs::events()) {
                    let label = if camel {
                        js.to_string()
                    } else {
                        jsx::snake(js)
                    };
                    if given.iter().any(|g| g == js) || !label.starts_with(&prefix) {
                        continue;
                    }
                    if let Some(kind) = jsx_attrs::kind(&tag, js) {
                        push(label, "field", describe(kind).into());
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    Some(out)
}
