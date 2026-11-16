//! Fixed-size arrays where their length matters to the checker, which otherwise sees `[T; N]` as
//! a `Vec<T>`: `let [a, b] = arr;` is irrefutable when `arr` is known to hold two elements (its
//! annotation, an array literal, or `<[T; 2]>::try_from(..)`). A child module of `typeck`.

use ra_ap_syntax::{AstNode, ast};

use super::Checker;
use crate::ty::Ty;

/// `<[T; N]>::try_from`: N.
pub fn array_try_from_len(path: &ast::Path) -> Option<usize> {
    let f = path.segment()?.name_ref()?;
    if f.text() != "try_from" {
        return None;
    }
    let anchor = path.qualifier()?.segment()?.type_anchor()?;
    if anchor.path_type().is_some() {
        return None;
    }
    array_type_len(&anchor.ty()?)
}

/// The length of an array type written with a literal (`[f64; 4]`).
pub fn array_type_len(t: &ast::Type) -> Option<usize> {
    match t {
        ast::Type::ArrayType(a) => {
            let text = a.const_arg()?.syntax().text().to_string();
            text.trim()
                .trim_end_matches("usize")
                .replace('_', "")
                .parse()
                .ok()
        }
        ast::Type::RefType(r) => array_type_len(&r.ty()?),
        ast::Type::ParenType(p) => array_type_len(&p.ty()?),
        _ => None,
    }
}

/// The length of the array `e` evaluates to, when its syntax says: `[x; 3]`, `[a, b]`,
/// `<[T; 2]>::try_from(v)` (through `?`, `.unwrap()`, `.expect(..)`, `.ok()?`).
pub fn array_expr_len(e: &ast::Expr) -> Option<usize> {
    match e {
        ast::Expr::ParenExpr(p) => array_expr_len(&p.expr()?),
        ast::Expr::RefExpr(r) => array_expr_len(&r.expr()?),
        ast::Expr::TryExpr(t) => array_expr_len(&t.expr()?),
        ast::Expr::MethodCallExpr(m)
            if m.name_ref().is_some_and(|n| {
                matches!(n.text().to_string().as_str(), "unwrap" | "expect" | "ok")
            }) =>
        {
            array_expr_len(&m.receiver()?)
        }
        ast::Expr::ArrayExpr(a) => {
            if a.semicolon_token().is_some() {
                let mut exprs = a.exprs();
                let _ = exprs.next();
                let n = exprs.next()?;
                n.syntax()
                    .text()
                    .to_string()
                    .trim()
                    .trim_end_matches("usize")
                    .replace('_', "")
                    .parse()
                    .ok()
            } else {
                Some(a.exprs().count())
            }
        }
        ast::Expr::CallExpr(c) => {
            let ast::Expr::PathExpr(p) = c.expr()? else {
                return None;
            };
            let path = p.path()?;
            let f = path.segment()?.name_ref()?;
            if !matches!(f.text().to_string().as_str(), "try_from" | "from") {
                return None;
            }
            let anchor = path.qualifier()?.segment()?.type_anchor()?;
            array_type_len(&anchor.ty()?)
        }
        _ => None,
    }
}

impl Checker<'_> {
    /// `<[i64; 2]>::try_from(v)`: `Result<[i64; 2], Vec<i64>>` from a `Vec`, or with a
    /// `TryFromSliceError` from a slice (`&v[..]`).
    pub(super) fn array_try_from(
        &mut self,
        c: &ast::CallExpr,
        path: &ast::Path,
        args: &[ast::Expr],
    ) -> Option<Ty> {
        array_try_from_len(path)?;
        let anchor = path.qualifier()?.segment()?.type_anchor()?.ty()?;
        let target = self.lower_type(&anchor);
        self.arg_count(c.syntax(), "function", 1, args.len());
        let Some(a) = args.first() else {
            return Some(Ty::Unknown);
        };
        self.check(a, &target);
        let err = if matches!(super::strip_parens(a), ast::Expr::RefExpr(_)) {
            Ty::Extern("TryFromSliceError".into())
        } else {
            target.clone()
        };
        Some(Ty::Result(Box::new(target), Box::new(err)))
    }

    /// `let [a, b, ..] = arr;` when `arr` has a length the pattern always matches.
    pub(super) fn array_pat_fits(&self, l: &ast::LetStmt, pat: &ast::Pat) -> bool {
        let ast::Pat::SlicePat(sp) = super::strip_pat(pat) else {
            return false;
        };
        let len = l
            .ty()
            .and_then(|t| array_type_len(&t))
            .or_else(|| l.initializer().and_then(|e| array_len_in_file(&e, 0)));
        let Some(len) = len else {
            return false;
        };
        let pats: Vec<ast::Pat> = sp.pats().collect();
        // `..` or `rest @ ..`
        let is_rest = |p: &ast::Pat| match p {
            ast::Pat::RestPat(_) => true,
            ast::Pat::IdentPat(ip) => matches!(ip.pat(), Some(ast::Pat::RestPat(_))),
            _ => false,
        };
        let rest = pats.iter().any(is_rest);
        let fixed: Vec<&ast::Pat> = pats.iter().filter(|p| !is_rest(p)).collect();
        let count_ok = if rest {
            fixed.len() <= len
        } else {
            fixed.len() == len
        };
        count_ok && fixed.iter().all(|p| self.irrefutable_pat(p))
    }
}

/// `array_expr_len`, also through what the file declares: a local's `let`, a parameter's type,
/// a function's or method's return type, a field's type, a `const`.
pub fn array_len_in_file(e: &ast::Expr, depth: usize) -> Option<usize> {
    use ast::HasName;
    if let Some(n) = array_expr_len(e) {
        return Some(n);
    }
    if depth > 4 {
        return None;
    }
    let root = e.syntax().ancestors().last()?;
    let name_of = |n: Option<ast::Name>| n.map(|n| n.text().to_string());
    match super::strip_parens(e) {
        ast::Expr::PathExpr(p) => {
            let name = p.path()?.as_single_name_ref()?.text().to_string();
            // The nearest `let name ..` before it, else a parameter `name: [T; N]`.
            let at = e.syntax().text_range().start();
            let mut best: Option<ast::LetStmt> = None;
            for l in root.descendants().filter_map(ast::LetStmt::cast) {
                if l.syntax().text_range().end() > at {
                    continue;
                }
                if let Some(ast::Pat::IdentPat(ip)) = l.pat()
                    && name_of(ip.name()).as_deref() == Some(name.as_str())
                    && e.syntax().ancestors().any(|a| {
                        a.text_range().contains_range(l.syntax().text_range())
                            && ast::BlockExpr::can_cast(a.kind())
                    })
                {
                    best = Some(l);
                }
            }
            if let Some(l) = best {
                return l.ty().and_then(|t| array_type_len(&t)).or_else(|| {
                    l.initializer()
                        .and_then(|i| array_len_in_file(&i, depth + 1))
                });
            }
            for f in e.syntax().ancestors().filter_map(ast::Fn::cast) {
                for prm in f.param_list().into_iter().flat_map(|pl| pl.params()) {
                    if let Some(ast::Pat::IdentPat(ip)) = prm.pat()
                        && name_of(ip.name()).as_deref() == Some(name.as_str())
                    {
                        return prm.ty().and_then(|t| array_type_len(&t));
                    }
                }
            }
            // `const ARR: [T; N] = ..`
            root.descendants()
                .filter_map(ast::Const::cast)
                .find(|c| name_of(c.name()).as_deref() == Some(name.as_str()))
                .and_then(|c| c.ty())
                .and_then(|t| array_type_len(&t))
        }
        ast::Expr::CallExpr(c) => {
            let ast::Expr::PathExpr(p) = c.expr()? else {
                return None;
            };
            let f = p.path()?.segment()?.name_ref()?.text().to_string();
            fn_ret_len(&root, &f)
        }
        ast::Expr::MethodCallExpr(m) => fn_ret_len(&root, m.name_ref()?.text()),
        ast::Expr::FieldExpr(fe) => {
            let field = fe.name_ref()?.text().to_string();
            let lens: Vec<Option<usize>> = root
                .descendants()
                .filter_map(ast::RecordField::cast)
                .filter(|rf| name_of(rf.name()).as_deref() == Some(field.as_str()))
                .map(|rf| rf.ty().and_then(|t| array_type_len(&t)))
                .collect();
            match lens.as_slice() {
                [Some(n)] => Some(*n),
                [Some(n), rest @ ..] if rest.iter().all(|x| *x == Some(*n)) => Some(*n),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The array length every function named `name` in the file returns, if they agree.
fn fn_ret_len(root: &ra_ap_syntax::SyntaxNode, name: &str) -> Option<usize> {
    use ast::HasName;
    let lens: Vec<Option<usize>> = root
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|f| f.name().is_some_and(|n| n.text() == name))
        .map(|f| {
            f.ret_type()
                .and_then(|r| r.ty())
                .and_then(|t| array_type_len(&t))
        })
        .collect();
    let first = (*lens.first()?)?;
    lens.iter().all(|x| *x == Some(first)).then_some(first)
}
