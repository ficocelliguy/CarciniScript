// Proc macros on token trees with the plain `proc_macro` API: iterating a `TokenStream`,
// matching `TokenTree`s, `Group`s and their delimiters, `Punct` spacing, and building output
// from trees (`collect`, `extend`, `TokenStream::from`).
use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// A description of each token tree, recursively.
fn describe(stream: TokenStream, out: &mut Vec<String>) {
    for tree in stream {
        match tree {
            TokenTree::Group(g) => {
                let d = match g.delimiter() {
                    Delimiter::Parenthesis => "()",
                    Delimiter::Brace => "{}",
                    Delimiter::Bracket => "[]",
                    Delimiter::None => "none",
                };
                out.push(format!("group{d}"));
                describe(g.stream(), out);
                out.push("end".to_string());
            }
            TokenTree::Ident(i) => out.push(format!("ident:{i}")),
            TokenTree::Punct(p) => {
                let joint = if p.spacing() == Spacing::Joint { "+" } else { "" };
                out.push(format!("punct:{}{joint}", p.as_char()));
            }
            TokenTree::Literal(l) => out.push(format!("lit:{l}")),
        }
    }
}

/// `tokens!(..)`: a string literal describing the input's token trees.
#[proc_macro]
pub fn tokens(input: TokenStream) -> TokenStream {
    let mut out = Vec::new();
    describe(input, &mut out);
    let lit = Literal::string(&out.join(" "));
    TokenStream::from(TokenTree::Literal(lit))
}

/// `reversed!(a, b, c)`: the tuple `(c, b, a)`, built from token trees.
#[proc_macro]
pub fn reversed(input: TokenStream) -> TokenStream {
    let mut items: Vec<Vec<TokenTree>> = vec![Vec::new()];
    for tree in input {
        match &tree {
            TokenTree::Punct(p) if p.as_char() == ',' => items.push(Vec::new()),
            _ => items.last_mut().unwrap().push(tree),
        }
    }
    items.retain(|i| !i.is_empty());
    items.reverse();
    let mut inner = TokenStream::new();
    for (k, item) in items.into_iter().enumerate() {
        if k > 0 {
            inner.extend(vec![TokenTree::Punct(Punct::new(',', Spacing::Alone))]);
        }
        inner.extend(item);
    }
    let group = Group::new(Delimiter::Parenthesis, inner);
    TokenStream::from(TokenTree::Group(group))
}

/// `#[derive(FieldCount)]`: `T::field_count()`, counting `name:` pairs in the braces.
#[proc_macro_derive(FieldCount)]
pub fn field_count(input: TokenStream) -> TokenStream {
    let trees: Vec<TokenTree> = input.into_iter().collect();
    let mut name = None;
    let mut count = 0;
    for (k, tree) in trees.iter().enumerate() {
        match tree {
            TokenTree::Ident(i) if i.to_string() == "struct" => {
                if let Some(TokenTree::Ident(n)) = trees.get(k + 1) {
                    name = Some(n.to_string());
                }
            }
            TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
                let inner: Vec<TokenTree> = g.stream().into_iter().collect();
                for (j, t) in inner.iter().enumerate() {
                    let is_field = matches!(t, TokenTree::Ident(_))
                        && matches!(inner.get(j + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':' && p.spacing() == Spacing::Alone);
                    if is_field {
                        count += 1;
                    }
                }
            }
            _ => {}
        }
    }
    let name = Ident::new(&name.unwrap(), Span::call_site());
    // `impl Name { pub fn field_count() -> usize { N } }`, tree by tree.
    let body: TokenStream = vec![TokenTree::Literal(Literal::usize_unsuffixed(count))]
        .into_iter()
        .collect();
    let mut sig = TokenStream::new();
    for word in ["pub", "fn", "field_count"] {
        sig.extend(vec![TokenTree::Ident(Ident::new(word, Span::call_site()))]);
    }
    sig.extend(vec![
        TokenTree::Group(Group::new(Delimiter::Parenthesis, TokenStream::new())),
        TokenTree::Punct(Punct::new('-', Spacing::Joint)),
        TokenTree::Punct(Punct::new('>', Spacing::Alone)),
        TokenTree::Ident(Ident::new("usize", Span::call_site())),
        TokenTree::Group(Group::new(Delimiter::Brace, body)),
    ]);
    let mut out = TokenStream::new();
    out.extend(vec![
        TokenTree::Ident(Ident::new("impl", Span::call_site())),
        TokenTree::Ident(name),
        TokenTree::Group(Group::new(Delimiter::Brace, sig)),
    ]);
    out
}
