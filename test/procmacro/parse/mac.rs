// Proc macros with their own `syn` parsers: `impl Parse` on a `ParseStream`, `Token![..]`,
// `peek` / `peek2` / `lookahead1`, `parenthesized!`, `parse_terminated`, `parse_macro_input!`
// with a parser function, and `parse_quote!`.
use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Ident, LitInt, LitStr, Result, Token, Type, parenthesized, parse_macro_input, parse_quote};

/// A value in `config!`: `"text"`, `3`, or `f(args, ..)`.
enum Value {
    Text(LitStr),
    Number(LitInt),
    Call(Ident, Punctuated<Expr, Token![,]>),
}

/// `key = value`
struct Entry {
    key: Ident,
    value: Value,
}

impl Parse for Entry {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let lookahead = input.lookahead1();
        let value = if lookahead.peek(LitStr) {
            Value::Text(input.parse()?)
        } else if lookahead.peek(LitInt) {
            Value::Number(input.parse()?)
        } else if lookahead.peek(Ident) {
            let f: Ident = input.parse()?;
            let content;
            parenthesized!(content in input);
            Value::Call(f, content.parse_terminated(Expr::parse, Token![,])?)
        } else {
            return Err(lookahead.error());
        };
        Ok(Entry { key, value })
    }
}

struct Config {
    entries: Punctuated<Entry, Token![,]>,
}

impl Parse for Config {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Config {
            entries: Punctuated::parse_terminated(input)?,
        })
    }
}

/// `config!(name = "home", retries = 3, mode = fast(1, 2 + 3))`: prints each entry.
#[proc_macro]
pub fn config(input: TokenStream) -> TokenStream {
    let config = parse_macro_input!(input as Config);
    let lines: Vec<_> = config
        .entries
        .iter()
        .map(|e| {
            let key = e.key.to_string();
            match &e.value {
                Value::Text(s) => {
                    let v = s.value();
                    quote! { println!("{} = {:?}", #key, #v); }
                }
                Value::Number(n) => {
                    let digits = n.base10_digits().to_string();
                    quote! { println!("{} = {} (number)", #key, #digits); }
                }
                Value::Call(f, args) => {
                    let name = f.to_string();
                    let count = args.len();
                    quote! { println!("{} = {}(..) with {} args", #key, #name, #count); }
                }
            }
        })
        .collect();
    quote! { { #(#lines)* } }.into()
}

/// `sum!(1, 2, 3)`: the sum, at compile time.
#[proc_macro]
pub fn sum(input: TokenStream) -> TokenStream {
    let nums = parse_macro_input!(input with Punctuated::<LitInt, Token![,]>::parse_terminated);
    let total: u64 = nums
        .iter()
        .map(|n| n.base10_digits().parse::<u64>().unwrap())
        .sum();
    quote! { #total }.into()
}

/// What each token is, as `peek` sees it.
fn classify(input: ParseStream) -> Result<Vec<String>> {
    let mut out = Vec::new();
    while !input.is_empty() {
        if input.peek(Token![fn]) {
            input.parse::<Token![fn]>()?;
            out.push("fn".to_string());
        } else if input.peek(Ident) && input.peek2(Token![::]) {
            let a: Ident = input.parse()?;
            input.parse::<Token![::]>()?;
            out.push(format!("path {}", a));
        } else if input.peek(Token![->]) {
            input.parse::<Token![->]>()?;
            out.push("arrow".to_string());
        } else if input.peek(Ident) {
            let i: Ident = input.parse()?;
            out.push(format!("ident {}", i));
        } else if input.peek(LitStr) {
            let s: LitStr = input.parse()?;
            out.push(format!("str {}", s.value()));
        } else {
            let t: proc_macro2::TokenTree = input.parse()?;
            out.push(format!("other {}", t));
        }
    }
    Ok(out)
}

/// `classify_tokens!(..)`: a `Vec<&str>` describing the tokens.
#[proc_macro]
pub fn classify_tokens(input: TokenStream) -> TokenStream {
    let labels = parse_macro_input!(input with classify);
    quote! { vec![#(#labels),*] }.into()
}

/// `empty_vec_of!(u8)`: `Vec::<u8>::new().len()`, through a type built with `parse_quote!`.
#[proc_macro]
pub fn empty_vec_of(input: TokenStream) -> TokenStream {
    let ty = syn::parse::<Type>(input).unwrap();
    let wrapped: Type = parse_quote!(Vec<#ty>);
    quote! {
        {
            let v: #wrapped = Vec::new();
            v.len()
        }
    }
    .into()
}
