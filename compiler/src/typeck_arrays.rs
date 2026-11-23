//! Fixed-size arrays where their length matters to the checker, which otherwise sees `[T; N]` as
//! a `Vec<T>`: `let [a, b] = arr;` is irrefutable when `arr` is known to hold two elements (its
//! annotation, an array literal, or `<[T; 2]>::try_from(..)`). A child module of `typeck`.

use ra_ap_syntax::{AstNode, ast, ast::HasArgList};

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

/// The length of the array `x.try_into()` (the call `m`) converts to, where the code around it
/// writes it: a `let` annotation (`let a: [u8; 4] = s.try_into().unwrap();`, `let r:
/// Result<[u8; 4], _> = s.try_into();`), one of the file's functions' parameters or its return
/// type, or `u32::from_le_bytes(..)` and the like, through `?`, `.unwrap()`, `.expect(..)`,
/// `.unwrap_or(..)` and `.ok()`.
pub fn try_into_array_len(m: &ast::MethodCallExpr) -> Option<usize> {
    use ra_ap_syntax::SyntaxKind;
    let mut n = m.syntax().clone();
    // Whether the `Result` (or the `Option` of `.ok()`) has been unwrapped.
    let mut unwrapped = false;
    loop {
        let p = n.parent()?;
        match p.kind() {
            SyntaxKind::PAREN_EXPR => {}
            SyntaxKind::TRY_EXPR => unwrapped = true,
            SyntaxKind::METHOD_CALL_EXPR => {
                let mc = ast::MethodCallExpr::cast(p.clone())?;
                if mc.receiver()?.syntax() != &n {
                    return None;
                }
                match &*mc.name_ref()?.text() {
                    "unwrap" | "expect" | "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" => {
                        unwrapped = true
                    }
                    "ok" if !unwrapped => {}
                    _ => return None,
                }
            }
            SyntaxKind::LET_STMT => {
                let l = ast::LetStmt::cast(p)?;
                if l.initializer()?.syntax() != &n {
                    return None;
                }
                let t = l.ty()?;
                return if unwrapped {
                    type_len(&t)
                } else {
                    type_len(&first_type_arg(&t)?)
                };
            }
            SyntaxKind::ARG_LIST if unwrapped => {
                let i = p.children().position(|c| c == n)?;
                let call = p.parent()?;
                let (name, i) = if let Some(c) = ast::CallExpr::cast(call.clone()) {
                    let ast::Expr::PathExpr(f) = c.expr()? else {
                        return None;
                    };
                    let path = f.path()?;
                    let name = path.segment()?.name_ref()?.text().to_string();
                    // `u32::from_le_bytes(..)`: the integer's size in bytes.
                    if matches!(name.as_str(), "from_le_bytes" | "from_be_bytes" | "from_ne_bytes") {
                        let ty = path.qualifier()?.segment()?.name_ref()?.text().to_string();
                        return int_bytes(&ty);
                    }
                    (name, i)
                } else {
                    let mc = ast::MethodCallExpr::cast(call)?;
                    (mc.name_ref()?.text().to_string(), i + 1)
                };
                let f = file_fn(&p, &name)?;
                let mut params: Vec<Option<ast::Type>> = Vec::new();
                let pl = f.param_list()?;
                if pl.self_param().is_some() {
                    params.push(None);
                } else if i > 0 && ast::MethodCallExpr::can_cast(p.parent()?.kind()) {
                    return None;
                }
                params.extend(pl.params().map(|p| p.ty()));
                return type_len(params.get(i)?.as_ref()?);
            }
            SyntaxKind::RETURN_EXPR => {
                let f = p.ancestors().find_map(ast::Fn::cast)?;
                let t = f.ret_type()?.ty()?;
                return if unwrapped {
                    type_len(&t)
                } else {
                    type_len(&first_type_arg(&t)?)
                };
            }
            SyntaxKind::STMT_LIST => {
                let sl = ast::StmtList::cast(p.clone())?;
                if sl.tail_expr()?.syntax() != &n {
                    return None;
                }
                let block = p.parent()?;
                if let Some(f) = block.parent().and_then(ast::Fn::cast) {
                    let t = f.ret_type()?.ty()?;
                    return if unwrapped {
                        type_len(&t)
                    } else {
                        type_len(&first_type_arg(&t)?)
                    };
                }
                n = block;
                continue;
            }
            _ => return None,
        }
        n = p;
    }
}

/// The length of an array type (or a reference to one) written with a literal or a `const` of
/// the file.
fn type_len(t: &ast::Type) -> Option<usize> {
    match t {
        ast::Type::RefType(r) => type_len(&r.ty()?),
        ast::Type::ParenType(p) => type_len(&p.ty()?),
        _ => array_type_dims(t).first().copied(),
    }
}

/// `Result<T, E>` / `Option<T>`: `T`.
fn first_type_arg(t: &ast::Type) -> Option<ast::Type> {
    use ra_ap_syntax::ast::HasGenericArgs;
    let ast::Type::PathType(p) = t else { return None };
    let seg = p.path()?.segment()?;
    match seg.generic_arg_list()?.generic_args().next()? {
        ast::GenericArg::TypeArg(a) => a.ty(),
        _ => None,
    }
}

/// The size in bytes of a number type (`from_le_bytes` takes that many).
fn int_bytes(ty: &str) -> Option<usize> {
    Some(match ty {
        "u8" | "i8" => 1,
        "u16" | "i16" => 2,
        "u32" | "i32" | "f32" => 4,
        "u64" | "i64" | "f64" => 8,
        "u128" | "i128" => 16,
        "usize" | "isize" => 8,
        _ => return None,
    })
}

/// The one function (or method) named `name` in the file that holds `at`.
fn file_fn(at: &ra_ap_syntax::SyntaxNode, name: &str) -> Option<ast::Fn> {
    use ra_ap_syntax::ast::HasName;
    let root = at.ancestors().last()?;
    let mut fs = root
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|f| f.name().is_some_and(|n| n.text() == name));
    let f = fs.next()?;
    fs.next().is_none().then_some(f)
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

    /// What a `let` binds, as far as array lengths go: a tuple's fields from its annotation or
    /// a tuple expression, else the array's dimensions (`let_dims`).
    pub(super) fn let_shape(&self, l: &ast::LetStmt) -> PatShape {
        if let Some(t @ ast::Type::TupleType(_)) = l.ty() {
            return type_shape(&t);
        }
        if let Some(e @ ast::Expr::TupleExpr(_)) = l.initializer().map(|e| super::strip_parens(&e)) {
            return expr_shape(&e);
        }
        // `let ([a, b], c) = t;`: the tuple local's own `let` or parameter type.
        let mut init = l.initializer().map(|e| super::strip_parens(&e));
        for _ in 0..4 {
            let Some(e @ ast::Expr::PathExpr(_)) = &init else { break };
            match decl_of(e) {
                Some(Decl::Let(Some(t @ ast::Type::TupleType(_)), _))
                | Some(Decl::Param(Some(t @ ast::Type::TupleType(_)))) => return type_shape(&t),
                Some(Decl::Let(None, Some(i))) => match super::strip_parens(&i) {
                    e @ ast::Expr::TupleExpr(_) => return expr_shape(&e),
                    e => init = Some(e),
                },
                _ => break,
            }
        }
        dims_shape(self.let_dims(l))
    }

    /// The items a `for` loop takes from `iterable`: `enumerate()` and `zip(..)` pair them up,
    /// else the array's dimensions less the outermost (`item_dims`).
    pub(super) fn item_shape(&self, iterable: &ast::Expr) -> PatShape {
        if let ast::Expr::MethodCallExpr(m) = super::strip_parens(iterable)
            && let Some(recv) = m.receiver()
        {
            let arg = m.arg_list().and_then(|l| l.args().next());
            match m.name_ref().map(|n| n.text().to_string()).as_deref() {
                Some("enumerate") => {
                    return PatShape::Tuple(vec![PatShape::Unknown, self.item_shape(&recv)]);
                }
                Some("zip") => {
                    if let Some(a) = arg {
                        return PatShape::Tuple(vec![self.item_shape(&recv), self.item_shape(&a)]);
                    }
                }
                _ => {}
            }
        }
        dims_shape(self.item_dims(iterable))
    }

    /// Whether `pat` matches every value of `shape`: `let ([x, ..], y) = ([7, 8], 9);`,
    /// `for (i, [first, rest @ ..]) in rows.iter().enumerate()` over `[[u8; 3]; 2]`.
    pub(super) fn pat_fits(&self, pat: &ast::Pat, shape: &PatShape) -> bool {
        let pat = super::strip_pat(pat);
        let is_rest = |p: &ast::Pat| match p {
            ast::Pat::RestPat(_) => true,
            ast::Pat::IdentPat(ip) => matches!(ip.pat(), Some(ast::Pat::RestPat(_))),
            _ => false,
        };
        match (&pat, shape) {
            (ast::Pat::TuplePat(tp), PatShape::Tuple(fields)) => {
                let pats: Vec<ast::Pat> = tp.fields().collect();
                let rest = pats.iter().position(|p| matches!(p, ast::Pat::RestPat(_)));
                pats.iter().enumerate().all(|(i, p)| {
                    let at = match rest {
                        Some(r) if i == r => return true,
                        Some(r) if i > r => (fields.len() + i).checked_sub(pats.len()),
                        _ => Some(i),
                    };
                    let field = at.and_then(|k| fields.get(k)).unwrap_or(&PatShape::Unknown);
                    self.pat_fits(p, field)
                })
            }
            (ast::Pat::SlicePat(sp), PatShape::Array(dims)) => {
                let Some((&len, inner)) = dims.split_first() else {
                    return false;
                };
                let pats: Vec<ast::Pat> = sp.pats().collect();
                let fixed: Vec<&ast::Pat> = pats.iter().filter(|p| !is_rest(p)).collect();
                let count_ok = if fixed.len() < pats.len() {
                    fixed.len() <= len
                } else {
                    fixed.len() == len
                };
                let inner = dims_shape(inner.to_vec());
                count_ok && fixed.iter().all(|p| self.pat_fits(p, &inner))
            }
            (ast::Pat::IdentPat(ip), _) if ip.pat().is_some() && !is_rest(&pat) => {
                ip.pat().is_some_and(|sub| self.pat_fits(&sub, shape))
            }
            _ => self.irrefutable_pat(&pat),
        }
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

/// What's known of a value's array lengths, for a pattern over it (`Checker::pat_fits`).
pub enum PatShape {
    /// An array's nested lengths (`[[u8; 3]; 2]` → `[2, 3]`).
    Array(Vec<usize>),
    Tuple(Vec<PatShape>),
    Unknown,
}

fn dims_shape(dims: Vec<usize>) -> PatShape {
    if dims.is_empty() {
        PatShape::Unknown
    } else {
        PatShape::Array(dims)
    }
}

fn type_shape(t: &ast::Type) -> PatShape {
    match t {
        ast::Type::TupleType(tt) => PatShape::Tuple(tt.fields().map(|f| type_shape(&f)).collect()),
        ast::Type::ParenType(p) => p.ty().map(|t| type_shape(&t)).unwrap_or(PatShape::Unknown),
        t => dims_shape(array_type_dims(t)),
    }
}

fn expr_shape(e: &ast::Expr) -> PatShape {
    match super::strip_parens(e) {
        ast::Expr::TupleExpr(te) => PatShape::Tuple(te.fields().map(|f| expr_shape(&f)).collect()),
        e => dims_shape(array_dims_in_file(&e, 0)),
    }
}

