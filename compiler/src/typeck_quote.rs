//! `quote! { .. }` and `format_ident!(..)` in proc-macro crates. Each `#var` becomes tokens by
//! its type, like the `quote` crate's `ToTokens`: a `TokenStream`, `Ident` or `Literal` as its
//! tokens, a string as a string literal, a number as a suffixed literal (`3usize`), a `char` or
//! `bool` as itself (a token type is one with a `__tt` method, as the prelude's are). Inside
//! `#( .. ),*` the collections and iterators are iterated together, at any depth. A child module
//! of `typeck`.

use ra_ap_syntax::{AstNode, ast};

use super::Checker;
use crate::macros::{self, QuotePart};
use crate::ty::{IntKind, Ty};

/// How a `#var` becomes tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QuoteTok {
    /// A `TokenStream`, `Ident` or `Literal`: its text.
    Tokens,
    /// A string: a string literal.
    Str,
    Int(IntKind),
    Float {
        f32: bool,
    },
    Char,
    Bool,
}

impl Checker<'_> {
    /// `quote! { .. }`: a `TokenStream`.
    pub(super) fn quote_macro(&mut self, mc: &ast::MacroCall) -> Ty {
        if !self.adts.defs.contains_key("TokenStream") {
            self.error(
                mc.syntax(),
                "`quote!` works in proc-macro crates (files with `#[proc_macro]` functions)",
            );
            return Ty::Unknown;
        }
        let parts = macros::quote_parts(mc, self.base);
        self.quote_vars_in(&parts, &Default::default());
        Ty::Adt("TokenStream".into(), vec![])
    }

    /// Record how each `#var` in `parts` becomes tokens. `env` holds the types of the variables
    /// the enclosing repetitions iterate (their items).
    fn quote_vars_in(&mut self, parts: &[QuotePart], env: &std::collections::HashMap<String, Ty>) {
        for p in parts {
            match p {
                QuotePart::Text(_) => {}
                QuotePart::Var { name, start, end } => {
                    let Some(t) = self.quote_var_ty(name, env) else {
                        self.raw_error(
                            *start,
                            *end,
                            format!("cannot find value `{name}` in this scope"),
                        );
                        continue;
                    };
                    match self.quote_tok(&t) {
                        Some(tok) => {
                            self.quote_vars.insert((*start, *end), tok);
                        }
                        None => {
                            let msg = format!(
                                "the trait bound `{}: ToTokens` is not satisfied (`quote!` takes token streams, identifiers, literals, strings, numbers, chars and bools; repeat a collection with `#(..)*`)",
                                self.describe(&t)
                            );
                            self.raw_error(*start, *end, msg);
                        }
                    }
                }
                QuotePart::Rep { parts, at, .. } => {
                    // Like the `quote` crate: the variables inside that are collections or
                    // iterators are iterated (together); the others repeat as they are.
                    let mut names = Vec::new();
                    collect_var_names(parts, &mut names);
                    let mut inner = env.clone();
                    let mut iterated = Vec::new();
                    for (name, _, _) in &names {
                        if iterated.contains(name) {
                            continue;
                        }
                        let Some(t) = self.quote_var_ty(name, env) else {
                            continue;
                        };
                        let z = self.zonk(&t);
                        if matches!(z, Ty::Vec(_) | Ty::Iter { .. } | Ty::Set(_) | Ty::Coll(..))
                            && let Some(e) = self.iter_elem(&z)
                        {
                            inner.insert(name.clone(), self.zonk(&e));
                            iterated.push(name.clone());
                        }
                    }
                    if iterated.is_empty() {
                        let (s, e) = names.first().map(|n| (n.1, n.2)).unwrap_or((*at, *at + 1));
                        let msg = match names.first() {
                            Some((n, _, _)) => format!(
                                "`#(..)*` needs a collection or iterator to repeat over, and `{n}` is `{}`",
                                self.quote_var_ty(n, env)
                                    .map(|t| self.describe(&t))
                                    .unwrap_or_else(|| "unknown".into())
                            ),
                            None => "`#(..)*` needs a collection or iterator to repeat over".into(),
                        };
                        self.raw_error(s, e, msg);
                        continue;
                    }
                    self.quote_reps.insert(*at, iterated);
                    self.quote_vars_in(parts, &inner);
                }
            }
        }
    }

    /// A `#var`'s type: an item of an enclosing repetition, or the local's.
    fn quote_var_ty(&self, name: &str, env: &std::collections::HashMap<String, Ty>) -> Option<Ty> {
        env.get(name)
            .cloned()
            .or_else(|| self.lookup(name).map(|l| l.ty.clone()))
    }

    /// How a value of type `t` becomes tokens, if it can (`ToTokens`).
    pub(super) fn quote_tok(&self, t: &Ty) -> Option<QuoteTok> {
        Some(match self.zonk(t) {
            Ty::Adt(n, _) if self.adts.method(&n, "__tt").is_some() => QuoteTok::Tokens,
            // `Option<Ident>`: the tokens, or nothing.
            Ty::Option(inner) if matches!(&*inner, Ty::Adt(n, _) if self.adts.method(n, "__tt").is_some()) => {
                QuoteTok::Tokens
            }
            Ty::Str => QuoteTok::Str,
            Ty::Int(k) => QuoteTok::Int(k),
            Ty::IntLit => QuoteTok::Int(IntKind::I32),
            Ty::Float { f32 } => QuoteTok::Float { f32 },
            Ty::Char => QuoteTok::Char,
            Ty::Bool => QuoteTok::Bool,
            _ => return None,
        })
    }

    /// `stream.extend(tokens)` on the prelude's `TokenStream`.
    pub(super) fn token_extend(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        rt: &Ty,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        if !matches!(self.shallow(rt), Ty::Adt(n, _) if n == "TokenStream")
            || self.adts.method("TokenStream", "__push").is_none()
        {
            return None;
        }
        self.check_mutable(recv, super::Mutation::Borrow);
        self.arg_count(m.syntax(), "method", 1, args.len());
        if let Some(a) = args.first() {
            let at = self.infer(a, None);
            self.require_tokens(a.syntax(), &at);
        }
        Some(Ty::Unit)
    }

    /// Tokens, or something iterable over tokens (to extend or build a token stream from).
    pub(super) fn require_tokens(&mut self, node: &ra_ap_syntax::SyntaxNode, t: &Ty) {
        if self.tokens_like(t) {
            return;
        }
        let msg = format!(
            "expected tokens or an iterator over tokens, found `{}`",
            self.describe(t)
        );
        self.error(node, msg);
    }

    /// A token type, or an iterable of them (or not known yet).
    pub(super) fn tokens_like(&self, t: &Ty) -> bool {
        let z = self.zonk(t);
        if matches!(z, Ty::Var(_) | Ty::Unknown) || self.quote_tok(&z) == Some(QuoteTok::Tokens) {
            return true;
        }
        self.iter_elem(&z).is_some_and(|e| {
            let e = self.zonk(&e);
            matches!(e, Ty::Var(_) | Ty::Unknown) || self.quote_tok(&e) == Some(QuoteTok::Tokens)
        })
    }

    fn raw_error(&mut self, start: usize, end: usize, message: impl Into<String>) {
        self.diags.push(super::RawDiag {
            start,
            end,
            message: message.into(),
            severity: "error",
        });
    }
}

/// The `#var`s in `parts`, nested repetitions included: (name, start, end).
fn collect_var_names(parts: &[QuotePart], out: &mut Vec<(String, usize, usize)>) {
    for p in parts {
        match p {
            QuotePart::Var { name, start, end } => out.push((name.clone(), *start, *end)),
            QuotePart::Rep { parts, .. } => collect_var_names(parts, out),
            QuotePart::Text(_) => {}
        }
    }
}
