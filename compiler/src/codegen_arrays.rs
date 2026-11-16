//! Fixed-size arrays where their length matters, which the types (`[T; N]` is a `Vec<T>`) don't
//! carry: `Default` of `[T; N]` is N default elements, read from the written type (a field of a
//! `#[derive(Default)]` struct, or a `let` annotation). A child module of `codegen`.

use ra_ap_syntax::{
    AstNode,
    ast::{self, HasArgList},
};

use super::{Codegen, Frag, default_value};
use crate::ty::Ty;

impl Codegen<'_> {
    /// The default of an array type with a known length (`[u32; 4]`, `[[u8; 2]; SIZE]`): N
    /// default elements, each made fresh. `resolved` is its type (a `Vec`). None for other types.
    pub(super) fn array_default(&mut self, t: &ast::Type, resolved: &Ty) -> Option<String> {
        let ast::Type::ArrayType(a) = t else {
            return None;
        };
        let len_text = a.const_arg()?.syntax().text().to_string();
        let len_text = len_text.trim();
        let len = match len_text
            .trim_end_matches("usize")
            .replace('_', "")
            .parse::<usize>()
        {
            Ok(n) => n.to_string(),
            // A `const` item.
            Err(_) => self.consts.get(len_text)?.clone(),
        };
        let elem_ty = a.ty()?;
        let elem_resolved = match resolved {
            Ty::Vec(e) => (**e).clone(),
            _ => Ty::Unknown,
        };
        let elem = match self.array_default(&elem_ty, &elem_resolved) {
            Some(e) => e,
            None => default_value(&elem_resolved)?,
        };
        Some(format!("Array.from({{ length: {len} }}, () => {elem})"))
    }

    /// The default of each field of a struct with a `[T; N]` field, by the written types
    /// (None for a field that isn't such an array).
    pub(super) fn field_array_defaults(
        &mut self,
        node: &ra_ap_syntax::SyntaxNode,
        resolved: &[Ty],
    ) -> Vec<Option<String>> {
        let types: Vec<Option<ast::Type>> =
            match ast::Struct::cast(node.clone()).and_then(|s| s.field_list()) {
                Some(ast::FieldList::RecordFieldList(l)) => l.fields().map(|f| f.ty()).collect(),
                Some(ast::FieldList::TupleFieldList(l)) => l.fields().map(|f| f.ty()).collect(),
                None => Vec::new(),
            };
        types
            .iter()
            .zip(resolved)
            .map(|(t, r)| t.as_ref().and_then(|t| self.array_default(t, r)))
            .collect()
    }

    /// `<[i64; 2]>::try_from(v)`: `Ok` of `v` (a copy of a slice) when it has 2 elements, else
    /// `Err` of `v` (a `TryFromSliceError` for a slice).
    pub(super) fn emit_array_try_from(
        &mut self,
        c: &ast::CallExpr,
        path: &ast::Path,
    ) -> Option<(Frag, Ty)> {
        let n = crate::typeck::array_try_from_len(path)?;
        let a = c.arg_list()?.args().next()?;
        let slice = matches!(crate::typeck::strip_parens(&a), ast::Expr::RefExpr(_));
        let (v, t) = self.emit_expr(&a, None);
        let v = self.by_value(&a, v, &t);
        let mut f = Frag::mapped(self.off(c.syntax()), "κσ.$arrayTryFrom(");
        f.append(v).push(format!(", {n}, {slice})"));
        Some((f, self.ty_of(c.syntax()).unwrap_or(Ty::Unknown)))
    }

    /// `let x: [u8; 2] = Default::default();`: the annotation's array default, when the call is
    /// a `let`'s initializer.
    pub(super) fn let_array_default(&mut self, c: &ast::CallExpr) -> Option<String> {
        let l = c.syntax().parent().and_then(ast::LetStmt::cast)?;
        if l.initializer()?.syntax() != c.syntax() {
            return None;
        }
        let resolved = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
        self.array_default(&l.ty()?, &resolved)
    }
}
