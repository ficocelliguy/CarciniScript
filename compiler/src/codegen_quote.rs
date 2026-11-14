//! `quote! { .. }` and `format_ident!(..)`: the template's text with each `#var` as tokens (by
//! the type the checker recorded), as a new `TokenStream`; `#( .. ),*` through
//! `κσ.$quoteRep`. A child module of `codegen`.

use std::collections::HashMap;

use ra_ap_syntax::ast;

use super::{Codegen, Frag, js_string};
use crate::macros::{self, QuotePart};
use crate::ty::{IntKind, Ty};
use crate::typeck::QuoteTok;

impl Codegen<'_> {
    pub(super) fn emit_quote(&mut self, mc: &ast::MacroCall) -> (Frag, Ty) {
        let parts = macros::quote_parts(mc, self.base);
        let text = self.quote_text(&parts, &HashMap::new());
        let ty = Ty::Adt("TokenStream".into(), vec![]);
        if !self.types.adts.defs.contains_key("TokenStream") {
            return (text, ty);
        }
        (self.construct("TokenStream", 0, vec![text]), ty)
    }

    /// The template as a JS string expression. `params` names the variables a repetition
    /// iterates (they're its callback's parameters).
    fn quote_text(&mut self, parts: &[QuotePart], params: &HashMap<String, String>) -> Frag {
        let mut pieces: Vec<Frag> = Vec::new();
        for p in parts {
            match p {
                QuotePart::Text(t) => pieces.push(Frag::text(js_string(t))),
                QuotePart::Var { name, start, end } => {
                    let js = params
                        .get(name)
                        .cloned()
                        .or_else(|| self.lookup(name).map(|l| l.js.clone()))
                        .unwrap_or_else(|| "undefined".into());
                    let tok = self
                        .types
                        .quote_vars
                        .get(&(*start, *end))
                        .copied()
                        .unwrap_or(QuoteTok::Tokens);
                    pieces.push(Frag::text(tokens_js(tok, &js)));
                }
                QuotePart::Rep { parts, sep, at } => {
                    // The iterated variables become the callback's parameters; the others keep
                    // their values.
                    let names = self.types.quote_reps.get(at).cloned().unwrap_or_default();
                    let depth = params.values().filter(|v| v.starts_with("$q")).count();
                    let mut inner = params.clone();
                    let mut args = Vec::new();
                    let mut lists = Vec::new();
                    for (i, n) in names.iter().enumerate() {
                        lists.push(
                            params
                                .get(n)
                                .cloned()
                                .or_else(|| self.lookup(n).map(|l| l.js.clone()))
                                .unwrap_or_else(|| "[]".into()),
                        );
                        let arg = format!("$q{depth}_{i}");
                        inner.insert(n.clone(), arg.clone());
                        args.push(arg);
                    }
                    let body = self.quote_text(parts, &inner);
                    let mut f = Frag::text(format!(
                        "κσ.$quoteRep([{}], ({}) => ",
                        lists.join(", "),
                        args.join(", ")
                    ));
                    f.append(body).push(format!(", {})", js_string(sep)));
                    pieces.push(f);
                }
            }
        }
        if pieces.is_empty() {
            return Frag::text("\"\"");
        }
        Frag::join(pieces, " + ").wrap("(", ")")
    }
}

/// A value as tokens, like `ToTokens` in the `quote` crate.
fn tokens_js(tok: QuoteTok, js: &str) -> String {
    match tok {
        QuoteTok::Tokens => format!("κσ.$tt({js})"),
        QuoteTok::Str | QuoteTok::Char => format!("κσ.$dbg({js})"),
        QuoteTok::Bool => format!("String({js})"),
        QuoteTok::Int(k) => format!("(String({js}) + \"{}\")", int_suffix(k)),
        QuoteTok::Float { f32 } => {
            format!("(κσ.$fmtF({js}) + \"{}\")", if f32 { "f32" } else { "f64" })
        }
    }
}

fn int_suffix(k: IntKind) -> &'static str {
    match k {
        IntKind::I8 => "i8",
        IntKind::I16 => "i16",
        IntKind::I32 => "i32",
        IntKind::I64 => "i64",
        IntKind::I128 => "i128",
        IntKind::Isize => "isize",
        IntKind::U8 => "u8",
        IntKind::U16 => "u16",
        IntKind::U32 => "u32",
        IntKind::U64 => "u64",
        IntKind::U128 => "u128",
        IntKind::Usize => "usize",
    }
}
