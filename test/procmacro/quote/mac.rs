// Proc macros built with `quote!`: `#var` interpolation (identifiers, string and number
// literals, token streams), `#( .. ),*` repetitions, `format_ident!`, `Literal`, and
// `proc_macro2::TokenStream`.
use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};

/// The struct's name and its named fields, from the item's text.
fn parse_struct(input: &TokenStream) -> (String, Vec<String>) {
    let src = input.to_string();
    let words: Vec<&str> = src.split_whitespace().collect();
    let at = words.iter().position(|w| *w == "struct").unwrap();
    let name: String = words[at + 1]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let body = &src[src.find('{').unwrap() + 1..src.rfind('}').unwrap()];
    let fields = body
        .split(',')
        .filter_map(|f| f.split_once(':'))
        .map(|(n, _)| n.trim().trim_start_matches("pub ").to_string())
        .collect();
    (name, fields)
}

/// `#[derive(Fields)]`: `FIELD_COUNT`, `field_names()` and a `get_<field>()` per field.
#[proc_macro_derive(Fields)]
pub fn fields(input: TokenStream) -> TokenStream {
    let (name, fields) = parse_struct(&input);
    let ident = format_ident!("{}", name);
    let getters: Vec<_> = fields.iter().map(|f| format_ident!("get_{}", f)).collect();
    let members: Vec<_> = fields.iter().map(|f| format_ident!("{}", f)).collect();
    let count = fields.len();
    let expanded = quote! {
        impl #ident {
            pub const FIELD_COUNT: usize = #count;

            pub fn field_names() -> Vec<&'static str> {
                vec![#(#fields),*]
            }

            #(
                pub fn #getters(&self) -> String {
                    format!("{:?}", self.#members)
                }
            )*
        }
    };
    expanded.into()
}

/// `make_consts!(NAME = value, ..)`: a `const` per pair, typed by the value.
#[proc_macro]
pub fn make_consts(input: TokenStream) -> TokenStream {
    let mut out = TokenStream2::new();
    for pair in input.to_string().split(',') {
        let Some((name, value)) = pair.split_once('=') else { continue };
        let name = format_ident!("{}", name.trim());
        let value = value.trim();
        let item = if let Ok(n) = value.parse::<i64>() {
            quote! { pub const #name: i64 = #n; }
        } else if let Ok(x) = value.parse::<f64>() {
            quote! { pub const #name: f64 = #x; }
        } else {
            let text = value.trim_matches('"');
            quote! { pub const #name: &str = #text; }
        };
        out.extend(item);
    }
    out.into()
}

/// `#[logged("label")]`: prints the label and the arguments' count, then runs the body.
#[proc_macro_attribute]
pub fn logged(attr: TokenStream, item: TokenStream) -> TokenStream {
    let src = item.to_string();
    let open = src.find('{').unwrap();
    let head: TokenStream2 = src[..open].parse().unwrap();
    let body: TokenStream2 = src[open + 1..src.rfind('}').unwrap()].parse().unwrap();
    let label = attr.to_string();
    let label = Literal::string(label.trim_matches('"'));
    let args = src[src.find('(').unwrap() + 1..src.find(')').unwrap()]
        .split(',')
        .filter(|a| !a.trim().is_empty())
        .count() as u32;
    let flag = args > 1;
    let tag = format_ident!("tag_{}", args, span = Span::call_site());
    quote! {
        #head {
            let #tag = #args;
            println!("[{}] {} args, many: {}", #label, #tag, #flag);
            #body
        }
    }
    .into()
}

/// `tables!(name: 1 2 3; other: 4 5)`: a function per row returning its label and values,
/// through nested repetitions (`label` repeats as it is).
#[proc_macro]
pub fn tables(input: TokenStream) -> TokenStream {
    let mut names = Vec::new();
    let mut values: Vec<Vec<i64>> = Vec::new();
    for row in input.to_string().split(';') {
        let Some((name, nums)) = row.split_once(':') else { continue };
        names.push(format_ident!("{}", name.trim()));
        values.push(nums.split_whitespace().map(|n| n.parse().unwrap()).collect());
    }
    let label = "row";
    quote! {
        #(
            pub fn #names() -> (&'static str, Vec<i64>) {
                (#label, vec![#(#values),*])
            }
        )*
    }
    .into()
}
