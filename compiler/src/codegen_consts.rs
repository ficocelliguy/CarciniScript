//! `const` and `static` items captured by a format string (`"{CYAN}text{RESET}"`): their JS
//! name, and their type from the checker's type of the item's value. A child module of
//! `codegen`.

use super::{Codegen, Frag};
use crate::ty::Ty;

impl Codegen<'_> {
    /// The value of a `const` / `static` named `name` (not shadowed by a local), for `{NAME}`.
    pub(super) fn captured_const(&mut self, name: &str) -> Option<(Frag, Ty)> {
        let js = self.consts.get(name)?.clone();
        // (The item's value has its type in the checker's map, by its range in this text.)
        let ty = self
            .const_bodies
            .get(name)
            .and_then(|&(s, e)| self.types.get(s, e))
            .map(|t| t.view())
            .unwrap_or(Ty::Unknown)
            .default_lit();
        Some((Frag::text(js), ty))
    }
}
