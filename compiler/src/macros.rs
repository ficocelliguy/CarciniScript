//! Built-in macro argument handling, shared by the type checker and codegen.
//!
//! rust-analyzer leaves macro arguments as an unparsed token tree. Both passes split it at
//! top-level commas and re-parse each argument from its source text, so the positions of the
//! re-parsed nodes (offset by the argument's absolute start) agree between the two passes.

use ra_ap_syntax::{
    AstNode, AstToken, Edition, NodeOrToken, SyntaxElement, SyntaxKind, SyntaxToken, T, ast,
};

pub const EDITION: Edition = Edition::Edition2024;

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
    for el in inner_elements(mc) {
        if el.kind() == T![,] {
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
    token.value().ok().map(|v| v.into_owned())
}
