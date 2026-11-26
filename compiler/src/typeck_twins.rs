//! One trait implemented for both `str` and `String` (or `[T]` and `Vec<T>`), which are one type
//! in JS: each method call picks its impl the way rustc's method probing does, by the receiver's
//! Rust type, read off how the receiver is written (a literal, a conversion, a local's or
//! field's declared type, a function's return type). Where that doesn't say, the call is an
//! error rather than a guess. A child module of `typeck`.

use ra_ap_syntax::{
    AstNode, SyntaxNode, TextRange, TextSize,
    ast::{self, HasLoopBody, HasName},
};

use super::Checker;

impl Checker<'_> {
    /// The key of `method` for receiver `recv` of a `String` / `Vec` (`key`) whose trait has impls
    /// for both the owned and the borrowed type: `m$own` for the owned one's, `m` otherwise.
    pub(super) fn pick_twin(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        key: &str,
        method: &str,
    ) -> String {
        match self.owned_expr(recv, key, 0) {
            Some(true) => {
                let k = format!("{method}$own");
                let r = self.range(m.syntax());
                self.method_keys.insert(r, k.clone());
                k
            }
            Some(false) => method.to_string(),
            None => {
                let (owned, borrowed) = twin_names(key);
                self.error(
                    recv.syntax(),
                    format!("can't tell whether this receiver is a `{borrowed}` or a `{owned}`, whose impls of `{method}` differ (they are one type in JS): give it a declared type, or call `.{}()` / `.{}()` on it", if key == "String" { "as_str" } else { "as_slice" }, if key == "String" { "to_string" } else { "to_vec" }),
                );
                method.to_string()
            }
        }
    }

    /// Whether `e` is the owned type (`String`, `Vec<T>`, or a reference to one: method probing
    /// finds the owned impl first), the borrowed one (`str` / `&str`, `[T]`, `[T; N]`), or
    /// unknown (None). `key` is `String` or `Vec`.
    fn owned_expr(&self, e: &ast::Expr, key: &str, depth: usize) -> Option<bool> {
        if depth > 8 {
            return None;
        }
        let rec = |x: &ast::Expr| self.owned_expr(x, key, depth + 1);
        match e {
            ast::Expr::ParenExpr(p) => rec(&p.expr()?),
            // (`&s` / `&mut v`: a reference to the same type; `*r` what `r` refers to.)
            ast::Expr::RefExpr(r) => rec(&r.expr()?),
            ast::Expr::PrefixExpr(p) if p.op_kind() == Some(ast::UnaryOp::Deref) => rec(&p.expr()?),
            ast::Expr::Literal(l) => match l.kind() {
                ast::LiteralKind::String(_) | ast::LiteralKind::ByteString(_) => Some(false),
                _ => None,
            },
            ast::Expr::ArrayExpr(_) => Some(false),
            // `s[..]`, `v[1..]`: a slice.
            ast::Expr::IndexExpr(i) => {
                matches!(i.index(), Some(ast::Expr::RangeExpr(_))).then_some(false)
            }
            ast::Expr::MacroExpr(mc) => {
                let name = mc.macro_call()?.path()?.segment()?.name_ref()?.text().to_string();
                matches!(name.as_str(), "format" | "vec").then_some(true)
            }
            ast::Expr::MethodCallExpr(mc) => {
                let name = mc.name_ref()?.text().to_string();
                match name.as_str() {
                    "to_string" | "to_owned" | "to_vec" | "to_uppercase" | "to_lowercase"
                    | "repeat" | "replace" | "replacen" | "into_owned" | "concat" | "join" => {
                        Some(true)
                    }
                    "as_str" | "as_slice" | "as_mut_slice" | "as_mut_str" | "trim" | "trim_start"
                    | "trim_end" | "as_bytes" => Some(false),
                    "clone" => rec(&mc.receiver()?),
                    _ => None,
                }
            }
            ast::Expr::CallExpr(c) => {
                let ast::Expr::PathExpr(p) = c.expr()? else {
                    return None;
                };
                let path = p.path()?;
                let segs: Vec<String> = path
                    .segments()
                    .filter_map(|s| s.name_ref().map(|n| n.text().to_string()))
                    .collect();
                match segs.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
                    [.., "String" | "Vec", "new" | "from" | "with_capacity" | "from_utf8_lossy"] => {
                        Some(true)
                    }
                    [f] => self.fn_returns_owned(c.syntax(), f, key),
                    _ => None,
                }
            }
            ast::Expr::PathExpr(p) => {
                let name = p.path()?.as_single_name_ref()?.text().to_string();
                if name == "self" {
                    return self.self_owned(e.syntax());
                }
                match self.lookup(&name).and_then(|l| l.decl) {
                    Some(decl) => self.decl_owned(e.syntax(), decl, key, depth),
                    // (After the body is checked, its scopes are gone: by the syntax.)
                    None => {
                        let ident = binding_before(e.syntax(), &name)?;
                        self.binding_owned(&ident, key, depth)
                    }
                }
            }
            ast::Expr::FieldExpr(f) => self.field_owned(f, key),
            _ => None,
        }
    }

    /// `self` in a method of `impl Tr for String` (owned) or `impl Tr for str` (borrowed).
    fn self_owned(&self, node: &SyntaxNode) -> Option<bool> {
        let imp = node.ancestors().find_map(ast::Impl::cast)?;
        crate::adt::twin_side(&imp).or_else(|| written_owned(&imp.self_ty()?))
    }

    /// A local declared at `decl`: its written type, or else its initializer's.
    fn decl_owned(&self, at: &SyntaxNode, decl: (usize, usize), key: &str, depth: usize) -> Option<bool> {
        let root = at.ancestors().last()?;
        let start = decl.0.checked_sub(self.base)?;
        let end = decl.1.checked_sub(self.base)?;
        let range = TextRange::new(TextSize::try_from(start).ok()?, TextSize::try_from(end).ok()?);
        let name = root.covering_element(range);
        let node = match name {
            ra_ap_syntax::NodeOrToken::Node(n) => n,
            ra_ap_syntax::NodeOrToken::Token(t) => t.parent()?,
        };
        let ident = node.ancestors().find_map(ast::IdentPat::cast)?;
        self.binding_owned(&ident, key, depth)
    }

    /// A `let` or parameter binding: its written type, or else its initializer's.
    fn binding_owned(&self, ident: &ast::IdentPat, key: &str, depth: usize) -> Option<bool> {
        let parent = ident.syntax().parent()?;
        if let Some(l) = ast::LetStmt::cast(parent.clone()) {
            if let Some(t) = l.ty() {
                return written_owned(&t);
            }
            return self.owned_expr(&l.initializer()?, key, depth + 1);
        }
        if let Some(p) = ast::Param::cast(parent.clone()) {
            return written_owned(&p.ty()?);
        }
        // `for w in ["a", "b"]` / `for v in vec![vec![1u8]]`: the literal's elements.
        if let Some(f) = ast::ForExpr::cast(parent) {
            let mut it = f.iterable()?;
            while let ast::Expr::RefExpr(r) = it {
                it = r.expr()?;
            }
            let first = match it {
                ast::Expr::ArrayExpr(a) => a.exprs().next()?,
                _ => return None,
            };
            return self.owned_expr(&first, key, depth + 1);
        }
        None
    }

    /// `x.f` with `f` declared `String` / `&str` / `Vec<T>` / `&[T]` in a struct of this file.
    fn field_owned(&self, f: &ast::FieldExpr, _key: &str) -> Option<bool> {
        let fname = f.name_ref()?.text().to_string();
        let recv_ty = self.raw.get(&self.range(f.expr()?.syntax()))?.clone();
        let crate::ty::Ty::Adt(sname, _) = self.zonk(&recv_ty) else {
            return None;
        };
        let root = f.syntax().ancestors().last()?;
        let st = root
            .descendants()
            .filter_map(ast::Struct::cast)
            .find(|s| s.name().is_some_and(|n| n.text() == sname.as_str()))?;
        let field = match st.field_list()? {
            ast::FieldList::RecordFieldList(l) => l
                .fields()
                .find(|x| x.name().is_some_and(|n| n.text() == fname.as_str()))?
                .ty()?,
            ast::FieldList::TupleFieldList(l) => {
                let i: usize = fname.parse().ok()?;
                l.fields().nth(i)?.ty()?
            }
        };
        written_owned(&field)
    }

    /// A call of the file's function `f`: its declared return type.
    fn fn_returns_owned(&self, at: &SyntaxNode, f: &str, _key: &str) -> Option<bool> {
        let root = at.ancestors().last()?;
        let func = root
            .descendants()
            .filter_map(ast::Fn::cast)
            .find(|x| x.name().is_some_and(|n| n.text() == f) && x.param_list().and_then(|p| p.self_param()).is_none())?;
        written_owned(&func.ret_type()?.ty()?)
    }
}

/// A written type: `String` / `Vec<T>` (or a reference to one) is owned; `str`, `[T]`, `[T; N]`
/// (or a reference to one) is borrowed.
pub(super) fn written_owned(t: &ast::Type) -> Option<bool> {
    match t {
        ast::Type::RefType(r) => written_owned(&r.ty()?),
        ast::Type::ParenType(p) => written_owned(&p.ty()?),
        ast::Type::SliceType(_) | ast::Type::ArrayType(_) => Some(false),
        ast::Type::PathType(p) => {
            let seg = p.path()?.segment()?;
            match seg.name_ref()?.text().as_ref() {
                "String" | "Vec" => Some(true),
                "str" => Some(false),
                _ => None,
            }
        }
        _ => None,
    }
}

fn twin_names(key: &str) -> (&'static str, &'static str) {
    if key == "String" {
        ("String", "str")
    } else {
        ("Vec<T>", "[T]")
    }
}

impl Checker<'_> {
    /// Whether `tr`'s method `m` has different impls for `str` and `String` (or `[T]` and
    /// `Vec<T>`).
    pub(super) fn twin_trait_method(&self, tr: &str, m: &str) -> bool {
        ["String", "Vec"].iter().any(|k| {
            self.adts.own_twins.contains(&format!("{k}::{m}"))
                && self.adts.twin_borrowed.contains(&(k.to_string(), tr.to_string()))
        })
    }

    /// `m` called through a type parameter or `dyn` (by its trait `tr`): which impl it reaches
    /// isn't known where it's compiled.
    pub(super) fn twin_generic_call(&mut self, node: &SyntaxNode, tr: &str, m: &str) {
        if self.twin_trait_method(tr, m) {
            self.error(
                node,
                format!("`{tr}::{m}` can't be called through a type parameter or `dyn` here: `{tr}` has different impls for a borrowed and an owned type that are one type in JS (`str` and `String`, or `[T]` and `Vec<T>`), so which one runs depends on a type erased at run time"),
            );
        }
    }

    /// Whether some trait's `m` has impls for both `str` and `String` (or `[T]` and `Vec<T>`).
    pub(super) fn twin_name(&self, m: &str) -> bool {
        ["String", "Vec"]
            .iter()
            .any(|k| self.adts.own_twins.contains(&format!("{k}::{m}")))
    }

    /// The first argument of `Tr::m(&s, ..)`: owned, borrowed, or unknown, whichever pair it is.
    pub(super) fn twin_arg_side(&self, a: &ast::Expr) -> Option<bool> {
        self.owned_expr(a, "", 0)
    }

    /// Path calls of a method whose trait has both impls (`String::decode(d)`, `<&str as
    /// Decode>::decode(d)`, `Value::record(&s, ..)`), as recorded where they were checked
    /// (`twin_sides`): the owned impl's key, or an error when it can't be told.
    pub(super) fn check_twin_paths(&mut self) {
        let mut sides: Vec<((usize, usize), (String, Option<bool>))> =
            std::mem::take(&mut self.twin_sides).into_iter().collect();
        sides.sort_by_key(|(r, _)| *r);
        for (r, (m, side)) in sides {
            let Some(crate::adt::Conv::Ext { key }) = self.conversions.get(&r).cloned() else {
                continue;
            };
            if !self.adts.own_twins.contains(&format!("{key}::{m}")) {
                continue;
            }
            match side {
                Some(true) => {
                    self.method_keys.insert(r, format!("{m}$own"));
                }
                Some(false) => {}
                None => {
                    let (owned, borrowed) = twin_names(&key);
                    let anchor = if key == "String" { "&str" } else { "&[T]" };
                    self.error_spans(
                        r,
                        format!("can't tell whether this calls the `{borrowed}` or the `{owned}` impl of `{m}` (they are one type in JS): write the type, `{owned}::{m}(..)` or `<{anchor} as ..>::{m}(..)`"),
                        None,
                        Vec::new(),
                    );
                }
            }
        }
    }

    /// `String` / `<Vec<u8>>` / `<String as Tr>` (owned), `str` / `<&str>` / `<[T]>` (borrowed),
    /// `Self` in an impl for one of them.
    pub(super) fn qualifier_owned(&self, q: &ast::Path, at: &SyntaxNode) -> Option<bool> {
        let seg = q.segment()?;
        if let Some(t) = seg.type_anchor().and_then(|a| a.ty()) {
            return written_owned(&t);
        }
        match seg.name_ref()?.text().as_ref() {
            "String" | "Vec" => Some(true),
            "str" => Some(false),
            _ if seg.self_type_token().is_some() => self.self_owned(at),
            _ => None,
        }
    }
}

/// The binding `name` refers to at `at`, by the syntax: the last `let name` before it in an
/// enclosing block, or a parameter of the enclosing function.
fn binding_before(at: &SyntaxNode, name: &str) -> Option<ast::IdentPat> {
    let pos = at.text_range().start();
    let named = |p: &ast::Pat| match p {
        ast::Pat::IdentPat(ip) if ip.name().is_some_and(|n| n.text() == name) => Some(ip.clone()),
        _ => None,
    };
    for anc in at.ancestors() {
        if let Some(list) = ast::StmtList::cast(anc.clone()) {
            let found = list
                .statements()
                .filter(|s| s.syntax().text_range().end() <= pos)
                .filter_map(|s| match s {
                    ast::Stmt::LetStmt(l) => l.pat().and_then(|p| named(&p)),
                    _ => None,
                })
                .last();
            if found.is_some() {
                return found;
            }
        }
        if let Some(f) = ast::Fn::cast(anc.clone()) {
            return f
                .param_list()?
                .params()
                .find_map(|p| p.pat().and_then(|p| named(&p)));
        }
        // (A closure's parameter, a `for`, `match` or `if let` binding of the name: not followed.)
        if let Some(f) = ast::ForExpr::cast(anc.clone())
            && let Some(p) = f.pat()
            && let Some(ip) = named(&p)
            && f.loop_body().is_some_and(|b| b.syntax().text_range().contains_range(at.text_range()))
        {
            return Some(ip);
        }
        let pats: Vec<ast::Pat> = if let Some(c) = ast::ClosureExpr::cast(anc.clone()) {
            c.param_list().into_iter().flat_map(|l| l.params()).filter_map(|p| p.pat()).collect()
        } else if let Some(f) = ast::ForExpr::cast(anc.clone()) {
            f.pat().into_iter().collect()
        } else if let Some(a) = ast::MatchArm::cast(anc.clone()) {
            a.pat().into_iter().collect()
        } else if let Some(cond) = ast::IfExpr::cast(anc.clone())
            .and_then(|i| i.condition())
            .or_else(|| ast::WhileExpr::cast(anc.clone()).and_then(|w| w.condition()))
        {
            cond.syntax().descendants().filter_map(ast::LetExpr::cast).filter_map(|l| l.pat()).collect()
        } else {
            Vec::new()
        };
        if pats.iter().any(|p| {
            p.syntax()
                .descendants()
                .filter_map(ast::IdentPat::cast)
                .any(|ip| ip.name().is_some_and(|n| n.text() == name))
        }) {
            return None;
        }
    }
    None
}
