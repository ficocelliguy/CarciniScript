// A proc-macro crate written with `TokenStream` as text: `to_string()` to read the input,
// `format!(..).parse()` to produce the output.
use proc_macro::TokenStream;

/// `square!(x)`: `x * x`, with `x` evaluated once.
#[proc_macro]
pub fn square(input: TokenStream) -> TokenStream {
    format!("{{ let v = {input}; v * v }}").parse().unwrap()
}

/// `make_getters!(name: type, ..)`: a function returning each value's default.
#[proc_macro]
pub fn make_defaults(input: TokenStream) -> TokenStream {
    let mut out = String::new();
    for part in input.to_string().split(',') {
        let Some((name, ty)) = part.split_once(':') else { continue };
        let (name, ty) = (name.trim(), ty.trim());
        out.push_str(&format!("fn default_{name}() -> {ty} {{ {ty}::default() }}\n"));
    }
    out.parse().unwrap()
}

/// `#[derive(Describe)]`: `T::describe()` names the type and its fields.
#[proc_macro_derive(Describe)]
pub fn describe(input: TokenStream) -> TokenStream {
    let src = input.to_string();
    let words: Vec<&str> = src.split_whitespace().collect();
    let at = words.iter().position(|w| *w == "struct" || *w == "enum").unwrap();
    let name: String = words[at + 1]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let body = &src[src.find('{').unwrap() + 1..src.rfind('}').unwrap()];
    let fields: Vec<String> = body
        .split(',')
        .filter_map(|f| f.split_once(':'))
        .map(|(n, _)| n.trim().trim_start_matches("pub ").to_string())
        .collect();
    format!(
        "impl {name} {{ pub fn describe() -> String {{ String::from({:?}) }} }}",
        format!("{} {{ {} }}", name, fields.join(", "))
    )
    .parse()
    .unwrap()
}

/// `#[traced]` on a function: prints its name when it's called.
#[proc_macro_attribute]
pub fn traced(attr: TokenStream, item: TokenStream) -> TokenStream {
    let src = item.to_string();
    let name = src.split("fn ").nth(1).unwrap().split('(').next().unwrap().trim().to_string();
    let open = src.find('{').unwrap();
    let label = if attr.to_string().is_empty() { name.clone() } else { attr.to_string() };
    format!("{} {{ println!(\"-> {}\"); {}", &src[..open], label, &src[open + 1..])
        .parse()
        .unwrap()
}
