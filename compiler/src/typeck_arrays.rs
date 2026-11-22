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

    /// `let [x, y] = a;` with `a` an array of another length: rustc's E0527 (or E0528 with a
    /// `..` that still asks for too many), as its message.
    pub(super) fn array_pat_len_error(&self, l: &ast::LetStmt, pat: &ast::Pat) -> Option<String> {
        let ast::Pat::SlicePat(sp) = super::strip_pat(pat) else {
            return None;
        };
        let len = l
            .ty()
            .and_then(|t| array_type_len(&t))
            .or_else(|| l.initializer().and_then(|e| array_len_in_file(&e, 0)))?;
        let is_rest = |p: &ast::Pat| match p {
            ast::Pat::RestPat(_) => true,
            ast::Pat::IdentPat(ip) => matches!(ip.pat(), Some(ast::Pat::RestPat(_))),
            _ => false,
        };
        let pats: Vec<ast::Pat> = sp.pats().collect();
        let rest = pats.iter().any(is_rest);
        let fixed = pats.iter().filter(|p| !is_rest(p)).count();
        if rest && fixed > len {
            Some(format!("pattern requires at least {fixed} elements but array has {len}"))
        } else if !rest && fixed != len {
            Some(format!("pattern requires {fixed} elements but array has {len}"))
        } else {
            None
        }
    }

    /// The dimensions of the array a `let` binds (`[[u8; 2]; 3]` → `[3, 2]`): from its type,
    /// else its initializer.
    pub(super) fn let_dims(&self, l: &ast::LetStmt) -> Vec<usize> {
        let dims = l.ty().map(|t| array_type_dims(&t)).unwrap_or_default();
        if !dims.is_empty() {
            return dims;
        }
        l.initializer()
            .map(|e| array_dims_in_file(&e, 0))
            .unwrap_or_default()
    }

    /// The dimensions of the items a `for` loop takes from `iterable` (`for [k, v] in pairs`,
    /// `pairs.iter()`, `&pairs`): the array's, less the outermost.
    pub(super) fn item_dims(&self, iterable: &ast::Expr) -> Vec<usize> {
        let inner = match super::strip_parens(iterable) {
            ast::Expr::MethodCallExpr(m)
                if m.name_ref().is_some_and(|n| {
                    matches!(
                        n.text().to_string().as_str(),
                        "iter" | "iter_mut" | "into_iter"
                    )
                }) =>
            {
                m.receiver()
            }
            other => Some(other),
        };
        let dims = inner.map(|e| array_dims_in_file(&e, 0)).unwrap_or_default();
        dims.get(1..).map(<[usize]>::to_vec).unwrap_or_default()
    }

    /// `ty` as the exhaustiveness check sees an array of dimensions `dims`: its length known at
    /// each level (see `Coll::FixedArray`).
    pub(super) fn with_dims(&self, ty: &Ty, dims: &[usize]) -> Ty {
        let Some((&n, inner)) = dims.split_first() else {
            return self.zonk(ty);
        };
        match self.zonk(ty) {
            Ty::Vec(e) => Ty::Coll(
                crate::ty::Coll::FixedArray(n),
                Box::new(Ty::Vec(Box::new(self.with_dims(&e, inner)))),
            ),
            other => other,
        }
    }

    /// Whether `pat` matches every value of `ty`, an array of dimensions `dims`
    /// (`let [a, [b, _]] = grid;`, `for [k, v] in pairs`).
    pub(super) fn array_pat_covers(&self, pat: &ast::Pat, ty: &Ty, dims: &[usize]) -> bool {
        let has_slice = pat
            .syntax()
            .descendants()
            .any(|n| ast::SlicePat::can_cast(n.kind()));
        if dims.is_empty() || !has_slice {
            return false;
        }
        let rows: Vec<Vec<super::Ap>> = self
            .abstract_pat(pat)
            .into_iter()
            .map(|ap| vec![ap])
            .collect();
        self.missing_steps.set(0);
        self.missing(&rows, &[self.with_dims(ty, dims)]).is_none()
    }
}

/// `array_expr_len`, also through what the file declares: a local's `let`, a parameter's type,
/// a function's or method's return type, a field's type, a `const`.
pub fn array_len_in_file(e: &ast::Expr, depth: usize) -> Option<usize> {
    array_dims_in_file(e, depth).first().copied()
}

/// The nested lengths of an array type (`[[u8; 2]; 3]` → `[3, 2]`), as far as they're written
/// with literals or a `const` of the file set to one.
pub fn array_type_dims(t: &ast::Type) -> Vec<usize> {
    match t {
        ast::Type::ArrayType(a) => {
            let Some(n) = array_type_len(t).or_else(|| const_len(a)) else {
                return Vec::new();
            };
            let mut dims = vec![n];
            if let Some(inner) = a.ty() {
                dims.extend(array_type_dims(&inner));
            }
            dims
        }
        ast::Type::RefType(r) => r.ty().map(|t| array_type_dims(&t)).unwrap_or_default(),
        ast::Type::ParenType(p) => p.ty().map(|t| array_type_dims(&t)).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `[T; N]` with `const N: usize = 4;` in the file: 4.
fn const_len(a: &ast::ArrayType) -> Option<usize> {
    use ast::HasName;
    let name = a.const_arg()?.syntax().text().to_string();
    let name = name.trim();
    let root = a.syntax().ancestors().last()?;
    let body = root
        .descendants()
        .filter_map(ast::Const::cast)
        .find(|c| c.name().is_some_and(|n| n.text() == name))?
        .body()?;
    body.syntax()
        .text()
        .to_string()
        .trim()
        .trim_end_matches("usize")
        .replace('_', "")
        .parse()
        .ok()
}

/// `array_len_in_file` with the lengths of nested arrays too (`[[1, 2], [3, 4]]` → `[2, 2]`).
pub fn array_dims_in_file(e: &ast::Expr, depth: usize) -> Vec<usize> {
    use ast::HasName;
    if depth > 4 {
        return array_expr_len(e).into_iter().collect();
    }
    if let Some(n) = array_expr_len(e) {
        let mut dims = vec![n];
        // (The elements' own lengths: the first element's, or the repeated one's.)
        if let ast::Expr::ArrayExpr(a) = strip_wrappers(e)
            && let Some(first) = a.exprs().next()
        {
            dims.extend(array_dims_in_file(&first, depth + 1));
        }
        return dims;
    }
    let Some(root) = e.syntax().ancestors().last() else {
        return Vec::new();
    };
    let name_of = |n: Option<ast::Name>| n.map(|n| n.text().to_string());
    let type_dims = |t: Option<ast::Type>| t.map(|t| array_type_dims(&t)).unwrap_or_default();
    match super::strip_parens(e) {
        ast::Expr::PathExpr(p) => {
            let Some(name) = p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .map(|n| n.text().to_string())
            else {
                return Vec::new();
            };
            match decl_of(e) {
                Some(Decl::Let(ty, init)) => {
                    let dims = type_dims(ty);
                    if !dims.is_empty() {
                        return dims;
                    }
                    return init
                        .map(|i| array_dims_in_file(&i, depth + 1))
                        .unwrap_or_default();
                }
                Some(Decl::Param(ty)) => return type_dims(ty),
                None => {}
            }
            // `const ARR: [T; N] = ..`
            type_dims(
                root.descendants()
                    .filter_map(ast::Const::cast)
                    .find(|c| name_of(c.name()).as_deref() == Some(name.as_str()))
                    .and_then(|c| c.ty()),
            )
        }
        ast::Expr::CallExpr(c) => match c.expr() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .map(|f| fn_ret_dims(&root, f.text()))
                .unwrap_or_default(),
            _ => Vec::new(),
        },
        ast::Expr::MethodCallExpr(m) => m
            .name_ref()
            .map(|n| fn_ret_dims(&root, n.text()))
            .unwrap_or_default(),
        ast::Expr::FieldExpr(fe) => {
            let Some(field) = fe.name_ref().map(|n| n.text().to_string()) else {
                return Vec::new();
            };
            let all: Vec<Vec<usize>> = root
                .descendants()
                .filter_map(ast::RecordField::cast)
                .filter(|rf| name_of(rf.name()).as_deref() == Some(field.as_str()))
                .map(|rf| type_dims(rf.ty()))
                .collect();
            agreed(all)
        }
        _ => Vec::new(),
    }
}

/// Where the local a path expression names is declared.
pub enum Decl {
    /// `let name: ty = init;`
    Let(Option<ast::Type>, Option<ast::Expr>),
    /// A parameter `name: ty`.
    Param(Option<ast::Type>),
}

/// The nearest `let name ..` before `e` in a block around it, else a parameter `name` of a
/// function around it.
pub fn decl_of(e: &ast::Expr) -> Option<Decl> {
    let root = e.syntax().ancestors().last()?;
    decl_at(&root, e, e.syntax().text_range().start())
}

/// `decl_of` for `e` found at `at` in the file `root` (`e` may be a macro argument parsed on
/// its own).
pub fn decl_at(
    root: &ra_ap_syntax::SyntaxNode,
    e: &ast::Expr,
    at: ra_ap_syntax::TextSize,
) -> Option<Decl> {
    use ast::HasName;
    let ast::Expr::PathExpr(p) = super::strip_parens(e) else {
        return None;
    };
    let name = p.path()?.as_single_name_ref()?.text().to_string();
    let name_of = |n: Option<ast::Name>| n.map(|n| n.text().to_string());
    let here = root.token_at_offset(at).right_biased()?.parent()?;
    let mut best: Option<ast::LetStmt> = None;
    for l in root.descendants().filter_map(ast::LetStmt::cast) {
        if l.syntax().text_range().end() > at {
            continue;
        }
        if let Some(ast::Pat::IdentPat(ip)) = l.pat()
            && name_of(ip.name()).as_deref() == Some(name.as_str())
            && here.ancestors().any(|a| {
                a.text_range().contains_range(l.syntax().text_range())
                    && ast::BlockExpr::can_cast(a.kind())
            })
        {
            best = Some(l);
        }
    }
    if let Some(l) = best {
        return Some(Decl::Let(l.ty(), l.initializer()));
    }
    for f in here.ancestors().filter_map(ast::Fn::cast) {
        for prm in f.param_list().into_iter().flat_map(|pl| pl.params()) {
            if let Some(ast::Pat::IdentPat(ip)) = prm.pat()
                && name_of(ip.name()).as_deref() == Some(name.as_str())
            {
                return Some(Decl::Param(prm.ty()));
            }
        }
    }
    None
}

/// `e` without the wrappers `array_expr_len` looks through.
fn strip_wrappers(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::ParenExpr(p) => p.expr().map(|i| strip_wrappers(&i)),
        ast::Expr::RefExpr(r) => r.expr().map(|i| strip_wrappers(&i)),
        ast::Expr::TryExpr(t) => t.expr().map(|i| strip_wrappers(&i)),
        ast::Expr::MethodCallExpr(m)
            if m.name_ref().is_some_and(|n| {
                matches!(n.text().to_string().as_str(), "unwrap" | "expect" | "ok")
            }) =>
        {
            m.receiver().map(|i| strip_wrappers(&i))
        }
        _ => None,
    }
    .unwrap_or_else(|| e.clone())
}

/// The dimensions all of `all` share, or none when they differ (or one is unknown).
fn agreed(all: Vec<Vec<usize>>) -> Vec<usize> {
    match all.split_first() {
        Some((first, rest)) if !first.is_empty() && rest.iter().all(|d| d == first) => {
            first.clone()
        }
        _ => Vec::new(),
    }
}

/// The array dimensions every function named `name` in the file returns, if they agree.
fn fn_ret_dims(root: &ra_ap_syntax::SyntaxNode, name: &str) -> Vec<usize> {
    use ast::HasName;
    let all: Vec<Vec<usize>> = root
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|f| f.name().is_some_and(|n| n.text() == name))
        .map(|f| {
            f.ret_type()
                .and_then(|r| r.ty())
                .map(|t| array_type_dims(&t))
                .unwrap_or_default()
        })
        .collect();
    agreed(all)
}
