//! `const` and `static` items captured by a format string (`"{CYAN}text{RESET}"`): their JS
//! name, and their type from the checker's type of the item's value. A child module of
//! `codegen`.

use ra_ap_syntax::{
    AstNode, SourceFile,
    ast::{self, HasName},
};

use super::{Codegen, Frag};
use crate::ty::Ty;

impl Codegen<'_> {
    /// The value of a `const` / `static` named `name` (not shadowed by a local), for `{NAME}`.
    pub(super) fn captured_const(&mut self, name: &str) -> Option<(Frag, Ty)> {
        let js = self.consts.get(name)?.clone();
        // (The item's value has its type in the checker's map, by its range in this text.)
        let file = SourceFile::parse(self.src, crate::macros::EDITION).tree();
        let body = crate::adt::all_items(&file)
            .into_iter()
            .find_map(|item| match item {
                ast::Item::Const(k) if k.name().is_some_and(|n| n.text() == name) => k.body(),
                ast::Item::Static(s) if s.name().is_some_and(|n| n.text() == name) => s.body(),
                _ => None,
            });
        let ty = body
            .and_then(|b| self.ty_of(b.syntax()))
            .unwrap_or(Ty::Unknown)
            .default_lit();
        Some((Frag::text(js), ty))
    }
}
