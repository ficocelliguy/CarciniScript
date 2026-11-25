//! A trait's methods can only be called where the trait is in scope: declared in the module, or
//! brought in by a `use` (also `as _` and `*`). Without it the call is rustc's E0599 ("items from
//! traits can only be used if the trait is in scope"). Inline modules are flattened before type
//! checking, so the scopes come from the program as written (`modflat::trait_scopes`); calls
//! through a type parameter's bound or a `dyn Trait` don't need the import, and aren't recorded.
//! A child module of `typeck`.

use std::collections::HashSet;

use ra_ap_syntax::{
    AstNode, TextRange, TextSize,
    ast::{self, HasName},
};

use super::{Checker, Origin, RawDiag};
use crate::lineindex::LineIndex;
use crate::ty::Ty;

/// A method call that an impl of one of the file's traits may answer.
pub(super) struct TraitCall {
    /// The call's range.
    call: (usize, usize),
    /// The receiver's type key (`Adts::methods`: a struct's name, `f64`, `Shout$all`).
    key: String,
    method: String,
    /// The receiver's type, as the message says it.
    recv: String,
}

impl Checker<'_> {
    /// Record `m`, a call of `method` on a `key` receiver of type `rt`, when a trait of this file
    /// has a method of that name.
    pub(super) fn note_trait_call(
        &mut self,
        m: &ast::MethodCallExpr,
        key: &str,
        method: &str,
        rt: &Ty,
    ) {
        let declared = self
            .adts
            .traits
            .values()
            .any(|t| t.module.is_none() && t.methods.iter().any(|x| x.name == method));
        if !declared {
            return;
        }
        let recv = self.describe(rt);
        self.trait_calls.push(TraitCall {
            call: self.range(m.syntax()),
            key: key.to_string(),
            method: method.to_string(),
            recv,
        });
    }

    /// rustc's E0599 for each recorded call that only a trait not in scope answers.
    pub(super) fn check_trait_scopes(&mut self, file: &ast::SourceFile, origin: &Origin) {
        if self.trait_calls.is_empty() {
            return;
        }
        let calls = std::mem::take(&mut self.trait_calls);
        let Some(scopes) = crate::modflat::trait_scopes(&origin.src) else {
            return;
        };
        // Inherent methods win over trait methods, in scope or not.
        let mut inherent: HashSet<(String, String)> = HashSet::new();
        for imp in file.syntax().descendants().filter_map(ast::Impl::cast) {
            let (None, Some(ty)) = crate::adt::impl_parts(&imp) else {
                continue;
            };
            for item in imp.assoc_item_list().into_iter().flat_map(|l| l.assoc_items()) {
                if let ast::AssocItem::Fn(f) = item
                    && let Some(n) = f.name()
                {
                    inherent.insert((ty.clone(), n.text().to_string()));
                }
            }
        }
        let idx = LineIndex::new(&origin.src);
        let mut reported = HashSet::new();
        for c in calls {
            if origin.generated(c.call.0)
                || inherent.contains(&(c.key.clone(), c.method.clone()))
                || !reported.insert(c.call)
            {
                continue;
            }
            let candidates: Vec<String> = match c.key.strip_suffix("$all") {
                Some(tr) => vec![tr.to_string()],
                None => self
                    .adts
                    .trait_impls
                    .get(&c.key)
                    .into_iter()
                    .flatten()
                    .filter(|t| {
                        self.adts
                            .traits
                            .get(*t)
                            .is_some_and(|d| d.methods.iter().any(|x| x.name == c.method))
                    })
                    .cloned()
                    .collect(),
            };
            // (Each one the file's own, declared in one of its modules.)
            let paths: Option<Vec<(Vec<String>, String)>> =
                candidates.iter().map(|t| scopes.trait_path(t)).collect();
            let Some(paths) = paths.filter(|p| !p.is_empty()) else {
                continue;
            };
            // Inside an impl of one of them (`self.a()` in `impl T for S`), it's in scope.
            let range = TextRange::new(
                TextSize::new(c.call.0 as u32),
                TextSize::new(c.call.1 as u32),
            );
            let in_impl = file
                .syntax()
                .covering_element(range)
                .ancestors()
                .filter_map(ast::Impl::cast)
                .any(|i| {
                    crate::adt::impl_parts(&i)
                        .0
                        .is_some_and(|t| candidates.contains(&t))
                });
            if in_impl {
                continue;
            }
            let at = origin.map.as_ref().map_or(c.call.0, |m| m.orig(c.call.0, false));
            let Some(in_scope) = scopes.at(at) else {
                continue;
            };
            if candidates.iter().any(|t| in_scope.contains(t)) {
                continue;
            }
            let (path, name) = &paths[0];
            let import: Vec<&str> = std::iter::once("crate")
                .chain(path.iter().map(String::as_str))
                .chain([name.as_str()])
                .collect();
            let site = scopes
                .use_site(at)
                .map(|s| {
                    let (l, col) = idx.line_col(s);
                    format!(" on line {}:{}", l + 1, col + 1)
                })
                .unwrap_or_default();
            self.diags.push(RawDiag {
                start: c.call.0,
                end: c.call.1,
                message: format!(
                    "no method named `{}` found for `{}` (items from traits can only be used if the trait is in scope: trait `{name}` provides it; add `use {};`{site})",
                    c.method,
                    c.recv,
                    import.join("::"),
                ),
                severity: "error",
                help: None,
                spans: Vec::new(),
            });
        }
    }
}
