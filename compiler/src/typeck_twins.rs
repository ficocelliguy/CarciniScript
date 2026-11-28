//! One trait implemented for both `str` and `String` (or `[T]` and `Vec<T>`), which are one type
//! in JS: each method call picks its impl the way rustc's method probing does, by the receiver's
//! Rust type, read off how the receiver is written (a literal, a conversion, a local's or
//! field's declared type, a function's return type). Where that doesn't say, the call is an
//! error rather than a guess. Through a type parameter the caller passes the owned type's class
//! (`κσ.$ownT`); likewise a reference's (`κσ.$wrapT`) where an impl for `&T` does more than
//! `T`'s, and the classes an impl's associated functions need when they are called by path. A
//! child module of `typeck`.

use ra_ap_syntax::{
    AstNode, SyntaxNode, TextRange, TextSize,
    ast::{self, HasArgList, HasLoopBody, HasName},
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
                    // (A box or pointer of it, into a `Box<dyn Tr>`.)
                    [.., "Box" | "Rc" | "Arc", "new"] => rec(&c.arg_list()?.args().next()?),
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
        let ident = self.decl_ident(at, decl)?;
        self.binding_owned(&ident, key, depth)
    }

    /// The binding a local declared at `decl` (absolute offsets) names.
    fn decl_ident(&self, at: &SyntaxNode, decl: (usize, usize)) -> Option<ast::IdentPat> {
        let mut root = at.ancestors().last()?;
        let mut base = self.base;
        // (A macro argument, re-parsed on its own, naming a local declared before it: the file's.)
        if decl.0 < base || decl.1 - base > usize::from(root.text_range().end()) {
            root = self
                .file_root
                .get_or_init(|| {
                    ast::SourceFile::parse(self.src, crate::macros::EDITION)
                        .tree()
                        .syntax()
                        .clone()
                })
                .clone();
            base = 0;
        }
        let start = decl.0.checked_sub(base)?;
        let end = decl.1.checked_sub(base)?;
        let range = TextRange::new(TextSize::try_from(start).ok()?, TextSize::try_from(end).ok()?);
        let name = root.covering_element(range);
        let node = match name {
            ra_ap_syntax::NodeOrToken::Node(n) => n,
            ra_ap_syntax::NodeOrToken::Token(t) => t.parent()?,
        };
        node.ancestors().find_map(ast::IdentPat::cast)
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
        if !self.adts.traits.get(tr).is_some_and(|d| d.methods.iter().any(|x| x.name == m)) {
            return false;
        }
        ["String", "Vec"].iter().any(|k| {
            self.adts.own_twins.contains(&format!("{k}::{m}"))
                && self.adts.twin_borrowed.contains(&(k.to_string(), tr.to_string()))
        })
    }

    /// `m` called through a type parameter or `dyn` (by its trait `tr`, on a `self_ty`): through
    /// a `T` whose class the function can take, by the class (the caller passes the owned type's
    /// as `κσ.$ownT(..)`); through a `dyn`, by the tag an owned value gets where it's coerced
    /// (`κσ.$dynNum(v, "$own")`). Elsewhere which impl it reaches isn't known where it's compiled.
    pub(super) fn twin_generic_call(
        &mut self,
        node: &SyntaxNode,
        tr: &str,
        m: &str,
        self_ty: &crate::ty::Ty,
    ) {
        if !self.twin_trait_method(tr, m) {
            return;
        }
        match self_ty {
            crate::ty::Ty::Dyn(_, _) => return,
            crate::ty::Ty::Param(p) if self.try_note_dict(p) => {
                let r = self.range(node);
                self.class_dispatch.insert(r);
                return;
            }
            _ => {}
        }
        {
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

impl Checker<'_> {
    /// Whether trait `tr` has impls for both a borrowed and an owned type (`str` and `String`,
    /// `[T]` and `Vec<T>`) whose methods differ.
    pub(super) fn twin_trait(&self, tr: &str) -> bool {
        self.adts.twin_borrowed.iter().any(|(_, t)| t == tr)
    }

    /// Whether trait `tr` has an impl for `&T`, `&mut T` or `Box<T>` that does more than `T`'s
    /// (`adt::wrapper_impl`).
    pub(super) fn wrapper_trait(&self, tr: &str) -> bool {
        ["ref", "refmut", "box"]
            .iter()
            .any(|k| self.adts.ext_types.contains(&format!("{tr}${k}")))
            || self.adts.ref_impl_traits.contains(tr)
    }

    /// A generic call `r`: for each type parameter bounded by a twin trait (`T: Enc` with
    /// `impl Enc for str` and `impl Enc for String`), whether the argument it's inferred from is
    /// the owned type, so the caller can pass the owned type's class; and for one bounded by a
    /// trait with a `&T` impl of its own, the references (or boxes) the argument adds to what the
    /// parameter is declared to take (`g(&x)` for `fn g<T: Tr>(t: T)`: `T` is `&X`), so the
    /// caller can pass a reference's class (`finish_twin_dicts`).
    pub(super) fn note_twin_targs(
        &mut self,
        r: (usize, usize),
        type_params: &[super::generic::TypeParam],
        params: &[crate::ty::Ty],
        ref_params: &[Option<String>],
        args: &[ast::Expr],
        turbofish: &[ast::Type],
    ) {
        if self.adts.twin_borrowed.is_empty()
            && !self.adts.ext_types.iter().any(|k| crate::adt::is_wrapper_key(k))
            && self.adts.ref_impl_traits.is_empty()
        {
            return;
        }
        let mut sides = Vec::new();
        for (pi, (p, bounds)) in type_params.iter().enumerate() {
            let twin = bounds.iter().any(|b| self.twin_trait(b));
            let wraps = bounds.iter().any(|b| self.wrapper_trait(b));
            if !twin && !wraps {
                continue;
            }
            let Some((i, a)) = params
                .iter()
                .zip(args)
                .enumerate()
                .find(|(_, (t, _))| matches!(t, crate::ty::Ty::Param(q) if q == p))
                .map(|(i, (_, a))| (i, a))
            else {
                // (`dep::<Box<u8>>()`: the wrappers the turbofish writes.)
                if wraps && let Some(t) = turbofish.get(pi) {
                    let kinds = written_wraps(t);
                    if !kinds.is_empty() {
                        sides.push((p.clone(), Some(false), self.range(t.syntax()), kinds));
                    }
                }
                continue;
            };
            let side = if twin { self.owned_expr(a, "", 0) } else { Some(false) };
            let kinds = if wraps {
                let declared = ref_params
                    .get(i)
                    .and_then(|r| r.as_deref())
                    .map_or(0, |r| r.matches('&').count());
                self.arg_wraps(a, 0).into_iter().skip(declared).collect()
            } else {
                Vec::new()
            };
            sides.push((p.clone(), side, self.range(a.syntax()), kinds));
        }
        if !sides.is_empty() {
            self.twin_targs.insert(r, sides);
        }
    }

    /// The references and boxes the expression `e` puts around its value, outermost first:
    /// `&x` is `["ref"]`, `&mut Box::new(x)` `["refmut", "box"]`, a `r` declared `&X` `["ref"]`.
    fn arg_wraps(&self, e: &ast::Expr, depth: usize) -> Vec<&'static str> {
        if depth > 8 {
            return Vec::new();
        }
        let mut out = Vec::new();
        match e {
            ast::Expr::ParenExpr(p) => {
                if let Some(x) = p.expr() {
                    out = self.arg_wraps(&x, depth + 1);
                }
            }
            ast::Expr::RefExpr(r) => {
                out.push(if r.mut_token().is_some() { "refmut" } else { "ref" });
                if let Some(x) = r.expr() {
                    out.extend(self.arg_wraps(&x, depth + 1));
                }
            }
            ast::Expr::CallExpr(c) => {
                if let Some(ast::Expr::PathExpr(p)) = c.expr()
                    && matches!(p.syntax().text().to_string().as_str(), "Box::new" | "Rc::new" | "Arc::new")
                    && let Some(x) = c.arg_list().and_then(|l| l.args().next())
                {
                    out.push("box");
                    out.extend(self.arg_wraps(&x, depth + 1));
                }
            }
            ast::Expr::PathExpr(p) => {
                let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
                    return out;
                };
                let decl = self.lookup(&name.text()).and_then(|l| l.decl);
                let Some(ident) = decl
                    .and_then(|d| self.decl_ident(e.syntax(), d))
                    .or_else(|| binding_before(e.syntax(), &name.text()))
                else {
                    return out;
                };
                let parent = ident.syntax().parent();
                let written = parent
                    .clone()
                    .and_then(ast::Param::cast)
                    .and_then(|p| p.ty())
                    .or_else(|| parent.clone().and_then(ast::LetStmt::cast).and_then(|l| l.ty()));
                if let Some(t) = written {
                    out = written_wraps(&t);
                } else if let Some(init) =
                    parent.and_then(ast::LetStmt::cast).and_then(|l| l.initializer())
                {
                    out = self.arg_wraps(&init, depth + 1);
                }
            }
            _ => {}
        }
        out
    }

    /// A string or vector coerced to `dyn tr` whose trait has twin impls: an owned one is tagged
    /// (`κσ.$dynNum(v, "$own")`), so a call through the `dyn` reaches the owned impl.
    pub(super) fn note_dyn_twin(&mut self, n: &SyntaxNode, found: &crate::ty::Ty, tr: &str) {
        if !matches!(self.shallow(found), crate::ty::Ty::Str | crate::ty::Ty::Vec(_))
            || !self.twin_trait(tr)
        {
            return;
        }
        let Some(e) = ast::Expr::cast(n.clone()) else {
            return;
        };
        match self.owned_expr(&e, "", 0) {
            Some(true) => {
                let r = self.range(n);
                self.dyn_twins.insert(r);
            }
            Some(false) => {}
            None => self.error(
                n,
                format!("can't tell whether this value is a borrowed `str` / `[T]` or an owned `String` / `Vec<T>`, whose impls of `{tr}` differ (they are one type in JS): give it a declared type, or call `.to_string()` / `.as_str()` (`.to_vec()` / `.as_slice()`) on it"),
            ),
        }
    }

    /// The classes `finish_needs` decided each generic call passes: the wrappers each one gets,
    /// innermost first (`own`: the owned type of a twin pair, `κσ.$ownT(..)`; `ref`, `refmut`,
    /// `box`: a reference's or box's, `κσ.$wrapT(..)`). A string or vector argument whose side of
    /// a twin pair nothing settles is an error.
    pub(super) fn finish_twin_dicts(
        &mut self,
        params: &std::collections::HashMap<String, Vec<String>>,
        args: &std::collections::HashMap<(usize, usize), Vec<crate::ty::Ty>>,
    ) -> std::collections::HashMap<(usize, usize), Vec<Vec<String>>> {
        let mut out = std::collections::HashMap::new();
        let callees: std::collections::HashMap<(usize, usize), String> = self
            .generic_calls
            .iter()
            .map(|(r, _, callee, _)| (*r, callee.clone()))
            .collect();
        let mut calls: Vec<_> = std::mem::take(&mut self.twin_targs).into_iter().collect();
        calls.sort_by_key(|(r, _)| *r);
        for (r, sides) in calls {
            let (Some(callee), Some(tys)) = (callees.get(&r), args.get(&r)) else {
                continue;
            };
            let Some(names) = params.get(callee) else {
                continue;
            };
            let mut wraps: Vec<Vec<String>> = vec![Vec::new(); names.len()];
            for (p, side, at, kinds) in sides {
                let Some(i) = names.iter().position(|n| *n == p) else {
                    continue;
                };
                let twin_ty = matches!(
                    tys.get(i).map(|t| self.zonk(t)),
                    Some(crate::ty::Ty::Str | crate::ty::Ty::Vec(_))
                );
                match side {
                    Some(true) if twin_ty => wraps[i].push("own".to_string()),
                    None if twin_ty => self.error_spans(
                        at,
                        format!("can't tell whether `{p}` is a borrowed `str` / `[T]` or an owned `String` / `Vec<T>` here, whose impls of its bound differ (they are one type in JS): give the argument a declared type, or call `.to_string()` / `.as_str()` (`.to_vec()` / `.as_slice()`) on it"),
                        None,
                        Vec::new(),
                    ),
                    _ => {}
                }
                wraps[i].extend(kinds.iter().rev().map(|k| k.to_string()));
            }
            if wraps.iter().any(|w| !w.is_empty()) {
                out.insert(r, wraps);
            }
        }
        out
    }
}

/// The references and boxes a written type puts around its value, outermost first.
fn written_wraps(t: &ast::Type) -> Vec<&'static str> {
    use ra_ap_syntax::ast::HasGenericArgs;
    let mut out = Vec::new();
    let mut t = t.clone();
    loop {
        let next = match &t {
            ast::Type::RefType(r) => {
                out.push(if r.mut_token().is_some() { "refmut" } else { "ref" });
                r.ty()
            }
            ast::Type::PathType(p)
                if p.path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.name_ref())
                    .is_some_and(|n| matches!(n.text().to_string().as_str(), "Box" | "Rc" | "Arc")) =>
            {
                out.push("box");
                match p
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.generic_arg_list())
                    .and_then(|l| l.generic_args().next())
                {
                    Some(ast::GenericArg::TypeArg(ta)) => ta.ty(),
                    _ => None,
                }
            }
            _ => None,
        };
        match next {
            Some(n) => t = n,
            None => break,
        }
    }
    out
}

impl Checker<'_> {
    /// `recv.method(args)` of the file's `impl .. adt` with type parameters of its own: as
    /// `note_twin_targs`, with the bounds read off the method's declaration.
    pub(super) fn note_twin_method_targs(
        &mut self,
        r: (usize, usize),
        adt: &str,
        method: &str,
        params: &[crate::ty::Ty],
        args: &[ast::Expr],
    ) {
        if self.adts.twin_borrowed.is_empty()
            && !self.adts.ext_types.iter().any(|k| crate::adt::is_wrapper_key(k))
        {
            return;
        }
        let root = self
            .file_root
            .get_or_init(|| {
                ast::SourceFile::parse(self.src, crate::macros::EDITION)
                    .tree()
                    .syntax()
                    .clone()
            })
            .clone();
        let Some(f) = root
            .descendants()
            .filter_map(ast::Impl::cast)
            .filter(|i| {
                i.self_ty().is_some_and(|t| {
                    t.syntax().text().to_string().split('<').next().map(str::trim) == Some(adt)
                })
            })
            .filter_map(|i| i.assoc_item_list())
            .flat_map(|l| l.assoc_items())
            .find_map(|it| match it {
                ast::AssocItem::Fn(f) if f.name().is_some_and(|n| n.text() == method) => Some(f),
                _ => None,
            })
        else {
            return;
        };
        let bounds = super::generic::plain_bounds(&f);
        let ref_params = self.adts.method(adt, method).map(|m| m.ref_params.clone()).unwrap_or_default();
        self.note_twin_targs(r, &bounds, params, &ref_params, args, &[]);
    }
}

impl Checker<'_> {
    /// `<Vec<u16> as Codec>::name()` of `impl<T: Codec> Codec for Vec<T>`: the classes its body
    /// needs (`T::name()`), passed as for a generic function's (`dict_args`). `self_ty` is the
    /// written type.
    pub(super) fn note_ext_static_dicts(
        &mut self,
        c: &ast::CallExpr,
        self_ty: &crate::ty::Ty,
        method: &str,
    ) {
        use crate::ty::Ty;
        let Some(key) = crate::adt::ext_key_of(self_ty) else {
            return;
        };
        let Some(sig) = self.adts.method(key, method).cloned() else {
            return;
        };
        let names = sig.dict_names();
        if sig.receiver.is_some() || names.len() <= sig.type_params.len() {
            return;
        }
        let Some(caller) = self.cur_fn.clone() else {
            return;
        };
        let elems: Vec<Ty> = match self_ty {
            // (`Vec`, `Option`, tuples, `Cell` / `RefCell`, maps.)
            t => crate::adt::ext_args(t),
        };
        let mut targs: Vec<Ty> = Vec::new();
        for (j, n) in names.iter().enumerate() {
            let i = sig.impl_args.iter().position(|a| matches!(a, Ty::Param(p) if p == n));
            targs.push(match i.and_then(|i| elems.get(i)) {
                Some(e) if j >= sig.type_params.len() => e.clone(),
                _ => Ty::Unknown,
            });
        }
        let r = self.range(c.syntax());
        self.generic_calls.push((r, caller, format!("{key}::{method}"), targs));
    }
}

impl Checker<'_> {
    /// `<&A as Tr>::f()` where `Tr` has an impl for `&T` of its own (`adt::wrapper_impl`): that
    /// impl's function, handed `A`'s class (the referent, `self_ty` with the reference erased).
    pub(super) fn note_wrapper_static(
        &mut self,
        c: &ast::CallExpr,
        written: &ast::Type,
        tr: &str,
        method: &str,
        self_ty: &crate::ty::Ty,
    ) {
        let kind = match written_wraps(written).first() {
            Some(k) => *k,
            None => return,
        };
        let key = format!("{tr}${kind}");
        let Some(sig) = self.adts.method(&key, method).cloned() else {
            return;
        };
        if sig.receiver.is_some() {
            return;
        }
        self.record_ext(c.syntax(), &key);
        let Some(caller) = self.cur_fn.clone() else {
            return;
        };
        let targs = sig
            .dict_names()
            .iter()
            .map(|n| {
                if sig.impl_args.iter().any(|a| matches!(a, crate::ty::Ty::Param(p) if p == n)) {
                    self_ty.clone()
                } else {
                    crate::ty::Ty::Unknown
                }
            })
            .collect();
        let r = self.range(c.syntax());
        self.generic_calls.push((r, caller, format!("{key}::{method}"), targs));
    }
}

impl Checker<'_> {
    /// `x.inv()` where `impl Inv for &f32` sits next to `impl Inv for f32` (one type in JS; see
    /// `adt::concrete_ref_impl`): the reference's impl (`inv$ref`) when the receiver is written
    /// as such a reference and the method takes `self` by value, which is where rustc's probe
    /// finds it first, or when the type has no impl of its own. Recorded for codegen.
    pub(super) fn pick_ref_impl(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        rt: &crate::ty::Ty,
        method: String,
    ) -> String {
        let Some(key) = crate::adt::ext_key_of(&self.shallow(rt)) else {
            return method;
        };
        for kind in ["refmut", "ref"] {
            let k = format!("{method}${kind}");
            let Some(sig) = self.adts.method(key, &k) else {
                continue;
            };
            let take = self.adts.method(key, &method).is_none()
                || sig.receiver == Some(crate::adt::Receiver::Value)
                    && self.written_ref(recv) == Some(kind == "refmut");
            if take {
                let r = self.range(m.syntax());
                self.method_keys.insert(r, k.clone());
                return k;
            }
        }
        method
    }

    /// Whether `e` is written as a reference: `&x` / `&mut x`, or a local or parameter declared
    /// as one (`r: &f32`, `let r = &mut s;`). Some(true) for `&mut`; None when it isn't one.
    fn written_ref(&self, e: &ast::Expr) -> Option<bool> {
    match e {
        ast::Expr::ParenExpr(p) => self.written_ref(&p.expr()?),
        ast::Expr::RefExpr(r) => Some(r.mut_token().is_some()),
        ast::Expr::PathExpr(p) => {
            let n = p.path()?.as_single_name_ref()?.text().to_string();
            // (In a macro's re-parsed arguments, by the offset in the file.)
            let (f, at) = match e.syntax().ancestors().find_map(ast::Fn::cast) {
                Some(f) => (f, e.syntax().text_range().start()),
                None => (
                    self.enclosing_fn(e.syntax())?,
                    TextSize::try_from(self.range(e.syntax()).0).ok()?,
                ),
            };
            let named = |pat: Option<ast::Pat>| {
                matches!(pat, Some(ast::Pat::IdentPat(ip)) if ip.name().is_some_and(|x| x.text() == n.as_str()))
            };
            let of_type = |t: Option<ast::Type>| match t {
                Some(ast::Type::RefType(r)) => Some(r.mut_token().is_some()),
                _ => None,
            };
            // (The last `let` of that name before the use, else a parameter.)
            let mut found = None;
            for l in f.syntax().descendants().filter_map(ast::LetStmt::cast) {
                if l.syntax().text_range().start() >= at || !named(l.pat()) {
                    continue;
                }
                found = Some(of_type(l.ty()).or_else(|| match l.initializer() {
                    Some(ast::Expr::RefExpr(r)) => Some(r.mut_token().is_some()),
                    _ => None,
                }));
            }
            if let Some(f) = found {
                return f;
            }
            f.param_list()?
                .params()
                .find(|p| named(p.pat()))
                .and_then(|p| of_type(p.ty()))
        }
        _ => None,
    }
    }
}
