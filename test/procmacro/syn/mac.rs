// Proc macros with `syn`: `parse_macro_input!` into `DeriveInput`, `ItemFn` and `LitStr`,
// matching `Data` / `Fields` / `FnArg` / `ReturnType`, `split_for_impl`, `syn::Error`, and
// `quote!` over the syntax tree.
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, FnArg, ItemFn, LitStr, ReturnType, parse_macro_input};

/// `#[derive(Builder)]`: `T::builder()`, a setter per field, and `build()` that reports the
/// first missing field.
#[proc_macro_derive(Builder)]
pub fn builder(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let builder = format_ident!("{}Builder", name);
    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f.named.clone(),
            _ => {
                return syn::Error::new_spanned(&input.ident, "Builder needs named fields")
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            return syn::Error::new(input.ident.span(), "Builder needs a struct")
                .to_compile_error()
                .into();
        }
    };
    let names: Vec<_> = fields.iter().map(|f| f.ident.clone().unwrap()).collect();
    let types: Vec<_> = fields.iter().map(|f| f.ty.clone()).collect();
    let labels: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    quote! {
        #[derive(Default)]
        pub struct #builder {
            #( #names: Option<#types>, )*
        }

        impl #builder {
            #(
                pub fn #names(mut self, value: #types) -> Self {
                    self.#names = Some(value);
                    self
                }
            )*

            pub fn build(self) -> Result<#name, String> {
                Ok(#name {
                    #( #names: self.#names.ok_or_else(|| format!("missing {}", #labels))?, )*
                })
            }
        }

        impl #name {
            pub fn builder() -> #builder {
                #builder::default()
            }
        }
    }
    .into()
}

/// `#[derive(VariantNames)]`: `variant_name(&self)` and `variant_count()`.
#[proc_macro_derive(VariantNames)]
pub fn variant_names(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let Data::Enum(e) = &input.data else {
        return syn::Error::new(input.ident.span(), "VariantNames needs an enum")
            .to_compile_error()
            .into();
    };
    let arms: Vec<TokenStream2> = e
        .variants
        .iter()
        .map(|v| {
            let id = &v.ident;
            let label = id.to_string();
            match &v.fields {
                Fields::Unit => quote! { Self::#id => #label },
                Fields::Unnamed(_) => quote! { Self::#id(..) => #label },
                Fields::Named(_) => quote! { Self::#id { .. } => #label },
            }
        })
        .collect();
    let count = e.variants.len();
    quote! {
        impl #name {
            pub fn variant_name(&self) -> &'static str {
                match self {
                    #( #arms, )*
                }
            }

            pub fn variant_count() -> usize {
                #count
            }
        }
    }
    .into()
}

/// `#[derive(Describe)]`: `T::describe()`, for generic types too.
#[proc_macro_derive(Describe)]
pub fn describe(input: TokenStream) -> TokenStream {
    // (Typed by the `let`.)
    let input: DeriveInput = parse_macro_input!(input);
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let fields: Vec<String> = match &input.data {
        Data::Struct(s) => s
            .fields
            .iter()
            .map(|f| f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default())
            .collect(),
        _ => Vec::new(),
    };
    let params = input.generics.params.len();
    let summary = format!("{} with {} fields: {}", name, fields.len(), fields.join(", "));
    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn describe() -> String {
                format!("{} ({} type parameters)", #summary, #params)
            }
        }
    }
    .into()
}

/// `#[timed("label")]` on a function: prints the label, its name and its arguments first.
#[proc_macro_attribute]
pub fn timed(attr: TokenStream, item: TokenStream) -> TokenStream {
    let label = parse_macro_input!(attr as LitStr).value();
    let func = parse_macro_input!(item as ItemFn);
    let ItemFn {
        attrs,
        vis,
        sig,
        block,
    } = func;
    let name = sig.ident.to_string();
    let count = sig.inputs.len();
    let args: Vec<_> = sig
        .inputs
        .iter()
        .filter_map(|a| match a {
            FnArg::Typed(t) => Some(t.pat.clone()),
            FnArg::Receiver(_) => None,
        })
        .collect();
    let returns = matches!(sig.output, ReturnType::Type(..));
    quote! {
        #(#attrs)*
        #vis #sig {
            println!("[{}] {} ({} args, returns: {})", #label, #name, #count, #returns);
            #( println!("  arg {:?}", #args); )*
            #block
        }
    }
    .into()
}
