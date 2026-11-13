//! Type inference and checking.
//!
//! Runs over the whole file before codegen, so a local's type can be decided by a later use:
//! `let mut v = Vec::new(); v.push(1u8);` makes `v` a `Vec<u8>`, and `let mut n = 0; n += x_u64;`
//! makes `n` a `u64`. Inference is per function body, using type variables and unification;
//! unsuffixed literals get integer/float variables that default to `i32`/`f64`, like rustc.
//!
//! The result is a table of fully resolved types keyed by absolute source range, covering every
//! expression and every binding name. Codegen reads types from it instead of guessing. This pass
//! reports name and type errors; codegen reports unsupported constructs.

use std::collections::HashMap;
use std::rc::Rc;

#[path = "typeck_ext.rs"]
mod ext;
pub use ext::{char_predicate, generic_bounds, is_fn_bound, is_option_closure_method};

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{
        self, ArithOp, BinaryOp, CmpOp, ElseBranch, HasArgList, HasGenericArgs, HasLoopBody,
        HasModuleItem, HasName, LiteralKind, RangeItem, UnaryOp,
    },
};

use crate::bindings::{self, Bindings, bare};
use crate::fmt::{self, ArgRef, Piece};
use crate::macros::{self, MacroArg};
use crate::ty::{IntKind, Ty, is_float_method, std_const, std_tail};

pub struct RawDiag {
    pub start: usize,
    pub end: usize,
    pub message: String,
    pub severity: &'static str,
}

/// Resolved types keyed by absolute `(start, end)` byte range: every expression, and the name of
/// every binding (`let x`, parameters, `for` variables).
#[derive(Default)]
pub struct Types {
    map: HashMap<(usize, usize), Ty>,
}

impl Types {
    pub fn get(&self, start: usize, end: usize) -> Option<&Ty> {
        self.map.get(&(start, end))
    }
}

pub struct FnSig {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub is_async: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum VarKind {
    General,
    Int,
    Float,
}

enum VarState {
    Unbound(VarKind),
    Bound(Ty),
}

#[derive(Clone)]
struct Local {
    ty: Ty,
    mutable: bool,
    /// `let x;`: assignable once per path even without `mut`.
    deferred: bool,
    /// `Some(true)` if the binding holds a `&mut` reference, `Some(false)` for `&`.
    reference: Option<bool>,
}

/// What a mutation of a place is doing, for rustc-style error messages.
#[derive(Clone, Copy, PartialEq)]
enum Mutation {
    /// `x = value`
    Assign,
    /// `v[i] = value`, `*r = value`, `v.push(..)`, `&mut v`
    Borrow,
}

pub fn check(file: &ast::SourceFile, src: &str) -> (Types, Vec<RawDiag>) {
    let mut c = Checker {
        src,
        base: 0,
        vars: Vec::new(),
        raw: HashMap::new(),
        diags: Vec::new(),
        fns: HashMap::new(),
        scopes: Vec::new(),
        ret_ty: Ty::Unit,
        in_async: false,
        bindings: bindings::current(),
        pending_collects: Vec::new(),
        generics: HashMap::new(),
    };
    for item in file.items() {
        if let ast::Item::Fn(f) = &item
            && let Some(name) = f.name()
        {
            c.generics = c.fn_generics(f);
            let params = f
                .param_list()
                .map(|pl| {
                    pl.params()
                        .map(|p| p.ty().map(|t| c.lower_type(&t)).unwrap_or(Ty::Unknown))
                        .collect()
                })
                .unwrap_or_default();
            let ret = f
                .ret_type()
                .and_then(|r| r.ty())
                .map(|t| c.lower_type(&t))
                .unwrap_or(Ty::Unit);
            c.generics.clear();
            let sig = FnSig {
                params,
                ret,
                is_async: f.async_token().is_some(),
            };
            c.fns.insert(name.text().to_string(), sig);
        }
    }
    for item in file.items() {
        if let ast::Item::Fn(f) = &item {
            c.check_fn(f);
        }
    }
    let map: HashMap<(usize, usize), Ty> = c.raw.iter().map(|(k, t)| (*k, c.zonk(t))).collect();
    let float_set = |t: &Ty| -> bool {
        fn walk(t: &Ty) -> bool {
            match t {
                Ty::Set(e) => e.contains_float() || walk(e),
                Ty::Map(k, v) => k.contains_float() || walk(k) || walk(v),
                Ty::Vec(e) | Ty::Option(e) | Ty::Future(e) => walk(e),
                Ty::Tuple(es) => es.iter().any(walk),
                _ => false,
            }
        }
        walk(t)
    };
    if let Some((&(start, end), ty)) = map
        .iter()
        .filter(|(_, t)| float_set(t))
        .min_by_key(|(k, _)| **k)
    {
        let message = format!(
            "the trait bound `f64: Hash` is not satisfied in `{}` (floats can't be hashed; store whole numbers or strings)",
            ty.describe()
        );
        c.diags.push(RawDiag {
            start,
            end,
            message,
            severity: "error",
        });
    }
    if let Some((&(start, end), ty)) = map
        .iter()
        .filter(|(_, t)| t.has_ambiguous_option())
        .min_by_key(|(k, _)| **k)
    {
        let message = format!(
            "`{}` is not supported yet: `None` and `Some(None)`/`Some(())` would be indistinguishable",
            ty.describe()
        );
        c.diags.push(RawDiag {
            start,
            end,
            message,
            severity: "error",
        });
    }
    (Types { map }, c.diags)
}

struct Checker<'a> {
    src: &'a str,
    /// Absolute offset of the text the current AST was parsed from (non-zero inside re-parsed
    /// macro arguments).
    base: usize,
    vars: Vec<VarState>,
    raw: HashMap<(usize, usize), Ty>,
    diags: Vec<RawDiag>,
    fns: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, Local>>,
    ret_ty: Ty,
    in_async: bool,
    bindings: Rc<Bindings>,
    /// `collect()` calls whose target type wasn't known yet: (target, item type, call node).
    pending_collects: Vec<(Ty, Ty, SyntaxNode)>,
    /// The current signature's `F: Fn(..)` type parameters, as closure types.
    generics: HashMap<String, Ty>,
}

const USIZE: Ty = Ty::Int(IntKind::Usize);

impl<'a> Checker<'a> {
    // ------------------------------------------------------------ positions & diagnostics

    fn range(&self, n: &SyntaxNode) -> (usize, usize) {
        let r = n.text_range();
        (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        )
    }

    fn error(&mut self, n: &SyntaxNode, message: impl Into<String>) {
        let (start, end) = self.range(n);
        self.diags.push(RawDiag {
            start,
            end,
            message: message.into(),
            severity: "error",
        });
    }

    fn record(&mut self, n: &SyntaxNode, ty: Ty) -> Ty {
        let key = self.range(n);
        self.raw.insert(key, ty.clone());
        ty
    }

    // ------------------------------------------------------------ inference variables

    fn fresh(&mut self, kind: VarKind) -> Ty {
        self.vars.push(VarState::Unbound(kind));
        Ty::Var(self.vars.len() as u32 - 1)
    }

    /// Follow bound variables until reaching a concrete type or an unbound variable.
    fn shallow(&self, t: &Ty) -> Ty {
        let mut t = t.clone();
        while let Ty::Var(v) = t {
            match &self.vars[v as usize] {
                VarState::Bound(b) => t = b.clone(),
                VarState::Unbound(_) => break,
            }
        }
        t
    }

    fn var_kind(&self, t: &Ty) -> Option<VarKind> {
        match self.shallow(t) {
            Ty::Var(v) => match self.vars[v as usize] {
                VarState::Unbound(k) => Some(k),
                VarState::Bound(_) => None,
            },
            _ => None,
        }
    }

    /// Fully resolve a type. Leftover literal variables take rustc's defaults; anything else
    /// unconstrained becomes Unknown.
    fn zonk(&self, t: &Ty) -> Ty {
        match self.shallow(t) {
            Ty::Var(v) => match self.vars[v as usize] {
                VarState::Unbound(VarKind::Int) => Ty::Int(IntKind::I32),
                VarState::Unbound(VarKind::Float) => Ty::Float { f32: false },
                _ => Ty::Unknown,
            },
            Ty::Vec(e) => Ty::Vec(Box::new(self.zonk(&e))),
            Ty::Option(e) => Ty::Option(Box::new(self.zonk(&e))),
            Ty::Map(k, v) => Ty::Map(Box::new(self.zonk(&k)), Box::new(self.zonk(&v))),
            Ty::Entry(k, v) => Ty::Entry(Box::new(self.zonk(&k)), Box::new(self.zonk(&v))),
            Ty::Closure { params, ret } => Ty::Closure {
                params: params.iter().map(|p| self.zonk(p)).collect(),
                ret: Box::new(self.zonk(&ret)),
            },
            Ty::Set(e) => Ty::Set(Box::new(self.zonk(&e))),
            Ty::Tuple(es) => Ty::Tuple(es.iter().map(|e| self.zonk(e)).collect()),
            Ty::Future(e) => Ty::Future(Box::new(self.zonk(&e))),
            Ty::Iter { elem, mutable } => Ty::Iter {
                elem: Box::new(self.zonk(&elem)),
                mutable,
            },
            other => other,
        }
    }

    fn describe(&self, t: &Ty) -> String {
        match self.shallow(t) {
            Ty::Var(_) => match self.var_kind(t) {
                Some(VarKind::Int) => "{integer}".into(),
                Some(VarKind::Float) => "{float}".into(),
                _ => "_".into(),
            },
            Ty::Vec(e) => format!("Vec<{}>", self.describe(&e)),
            Ty::Option(e) => format!("Option<{}>", self.describe(&e)),
            Ty::Set(e) => format!("HashSet<{}>", self.describe(&e)),
            Ty::Map(k, v) => format!("HashMap<{}, {}>", self.describe(&k), self.describe(&v)),
            Ty::Entry(k, v) => format!("Entry<'_, {}, {}>", self.describe(&k), self.describe(&v)),
            Ty::Closure { params, ret } => crate::ty::describe_closure(
                params.iter().map(|p| self.describe(p)).collect(),
                self.describe(&ret),
            ),
            Ty::Tuple(es) => {
                crate::ty::describe_tuple(es.iter().map(|e| self.describe(e)).collect())
            }
            Ty::Future(e) => format!("impl Future<Output = {}>", self.describe(&e)),
            Ty::Iter { elem, mutable } => {
                format!(
                    "std::slice::Iter{}<'_, {}>",
                    if mutable { "Mut" } else { "" },
                    self.describe(&elem)
                )
            }
            other => other.describe(),
        }
    }

    fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.shallow(a), self.shallow(b));
        match (&a, &b) {
            (Ty::Never, _) | (_, Ty::Never) | (Ty::Unknown, _) | (_, Ty::Unknown) => true,
            (Ty::Var(x), Ty::Var(y)) if x == y => true,
            (Ty::Var(x), Ty::Var(y)) => {
                let (kx, ky) = (self.var_kind(&a).unwrap(), self.var_kind(&b).unwrap());
                match (kx, ky) {
                    (VarKind::General, _) => self.vars[*x as usize] = VarState::Bound(b.clone()),
                    (_, VarKind::General) => self.vars[*y as usize] = VarState::Bound(a.clone()),
                    (k1, k2) if k1 == k2 => self.vars[*x as usize] = VarState::Bound(b.clone()),
                    _ => return false,
                }
                true
            }
            (Ty::Var(v), t) | (t, Ty::Var(v)) => {
                let ok = match self.var_kind(&Ty::Var(*v)).unwrap() {
                    VarKind::General => true,
                    VarKind::Int => matches!(t, Ty::Int(_)),
                    VarKind::Float => matches!(t, Ty::Float { .. }),
                };
                if ok {
                    self.vars[*v as usize] = VarState::Bound(t.clone());
                }
                ok
            }
            (Ty::Vec(x), Ty::Vec(y))
            | (Ty::Future(x), Ty::Future(y))
            | (Ty::Option(x), Ty::Option(y))
            | (Ty::Set(x), Ty::Set(y)) => self.unify(x, y),
            (Ty::Map(k1, v1), Ty::Map(k2, v2)) | (Ty::Entry(k1, v1), Ty::Entry(k2, v2)) => {
                let (k1, v1, k2, v2) = (k1.clone(), v1.clone(), k2.clone(), v2.clone());
                self.unify(&k1, &k2) && self.unify(&v1, &v2)
            }
            (
                Ty::Closure {
                    params: p1,
                    ret: r1,
                },
                Ty::Closure {
                    params: p2,
                    ret: r2,
                },
            ) => {
                let (p1, p2, r1, r2) = (p1.clone(), p2.clone(), r1.clone(), r2.clone());
                p1.len() == p2.len()
                    && p1.iter().zip(p2.iter()).all(|(x, y)| self.unify(x, y))
                    && self.unify(&r1, &r2)
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) => {
                xs.len() == ys.len()
                    && xs
                        .clone()
                        .iter()
                        .zip(ys.clone().iter())
                        .all(|(x, y)| self.unify(x, y))
            }
            (
                Ty::Iter {
                    elem: x,
                    mutable: m1,
                },
                Ty::Iter {
                    elem: y,
                    mutable: m2,
                },
            ) => m1 == m2 && self.unify(x, y),
            _ => a == b,
        }
    }

    /// Unify `found` with `expected`, reporting rustc's "mismatched types" on failure.
    fn expect(&mut self, n: &SyntaxNode, found: &Ty, expected: &Ty) {
        if !self.unify(found, expected) {
            let hint = match (self.shallow(expected), self.var_kind(found)) {
                (Ty::Float { .. }, Some(VarKind::Int)) => {
                    " (write `1.0`, or convert with `as f64`)"
                }
                (Ty::Float { .. }, _) if matches!(self.shallow(found), Ty::Int(_)) => {
                    " (convert with `as f64`)"
                }
                (Ty::Int(_), _) if matches!(self.shallow(found), Ty::Float { .. }) => {
                    " (NS numbers are `f64`; convert with `as`)"
                }
                (_, _) if matches!(self.shallow(found), Ty::Future(_)) => {
                    " (did you forget `.await`?)"
                }
                _ => "",
            };
            let msg = format!(
                "mismatched types: expected `{}`, found `{}`{hint}",
                self.describe(expected),
                self.describe(found)
            );
            self.error(n, msg);
        }
    }

    // ------------------------------------------------------------ scopes

    fn declare(&mut self, name: &ast::Name, local: Local) {
        let ty = local.ty.clone();
        self.record(name.syntax(), ty);
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.text().to_string(), local);
    }

    fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    // ------------------------------------------------------------ types

    fn lower_type(&mut self, t: &ast::Type) -> Ty {
        match t {
            ast::Type::RefType(r) => r.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::ParenType(p) => p.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::TupleType(tt) if tt.fields().next().is_none() => Ty::Unit,
            ast::Type::TupleType(tt) => {
                Ty::Tuple(tt.fields().map(|t| self.lower_type(&t)).collect())
            }
            ast::Type::NeverType(_) => Ty::Never,
            ast::Type::InferType(_) => self.fresh(VarKind::General),
            ast::Type::ImplTraitType(_) | ast::Type::DynTraitType(_) | ast::Type::FnPtrType(_)
                if self.lower_fn_type(t).is_some() =>
            {
                self.lower_fn_type(t).unwrap()
            }
            ast::Type::SliceType(s) => Ty::Vec(Box::new(
                s.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            )),
            ast::Type::ArrayType(a) => Ty::Vec(Box::new(
                a.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            )),
            ast::Type::PathType(p) => {
                let seg = p.path().and_then(|p| p.segment());
                let name = seg
                    .as_ref()
                    .and_then(|s| s.name_ref())
                    .map(|n| n.text().to_string())
                    .unwrap_or_default();
                if name == "HashMap" {
                    let mut args = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .map(|l| l.generic_args().collect::<Vec<_>>())
                        .unwrap_or_default()
                        .into_iter();
                    let mut next = |c: &mut Self| match args.next() {
                        Some(ast::GenericArg::TypeArg(ta)) => {
                            ta.ty().map(|t| c.lower_type(&t)).unwrap_or(Ty::Unknown)
                        }
                        _ => c.fresh(VarKind::General),
                    };
                    let k = next(self);
                    let v = next(self);
                    return Ty::Map(Box::new(k), Box::new(v));
                }
                if name == "HashSet" {
                    let arg = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let elem = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                        _ => None,
                    };
                    return Ty::Set(Box::new(
                        elem.unwrap_or_else(|| self.fresh(VarKind::General)),
                    ));
                }
                if name == "Option" {
                    let arg = seg
                        .clone()
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let inner = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                        _ => None,
                    };
                    return Ty::Option(Box::new(
                        inner.unwrap_or_else(|| self.fresh(VarKind::General)),
                    ));
                }
                if name == "Vec" {
                    let arg = seg
                        .and_then(|s| s.generic_arg_list())
                        .and_then(|l| l.generic_args().next());
                    let elem = match arg {
                        Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                        _ => None,
                    };
                    return Ty::Vec(Box::new(
                        elem.unwrap_or_else(|| self.fresh(VarKind::General)),
                    ));
                }
                if let Some(g) = self.generics.get(&name) {
                    return g.clone();
                }
                let ty = Ty::from_name(&name);
                match ty {
                    Ty::Int(k) if k.is_wide_unsupported() => {
                        self.error(
                            t.syntax(),
                            "128-bit integers are not supported yet (planned for phase 4)",
                        );
                    }
                    Ty::Unknown => self.error(
                        t.syntax(),
                        format!(
                            "type `{}` is not supported yet (planned for phase 1)",
                            t.syntax().text()
                        ),
                    ),
                    _ => {}
                }
                ty
            }
            _ => {
                self.error(
                    t.syntax(),
                    format!(
                        "type `{}` is not supported yet (planned for phase 1)",
                        t.syntax().text()
                    ),
                );
                Ty::Unknown
            }
        }
    }

    /// `Some(is_mut)` when a declared type is a reference.
    fn ref_kind(t: &ast::Type) -> Option<bool> {
        match t {
            ast::Type::RefType(r) => Some(r.mut_token().is_some()),
            _ => None,
        }
    }

    // ------------------------------------------------------------ items & statements

    fn check_fn(&mut self, f: &ast::Fn) {
        let Some(name) = f.name() else { return };
        let Some(sig) = self.fns.get(name.text()) else {
            return;
        };
        let (params, ret, is_async) = (sig.params.clone(), sig.ret.clone(), sig.is_async);
        self.ret_ty = ret.clone();
        self.in_async = is_async;
        self.scopes = vec![HashMap::new()];
        if let Some(pl) = f.param_list() {
            for (p, ty) in pl.params().zip(params) {
                if let Some(ast::Pat::IdentPat(ip)) = p.pat()
                    && let Some(n) = ip.name()
                {
                    let reference = p.ty().as_ref().and_then(Self::ref_kind);
                    let local = Local {
                        ty,
                        mutable: ip.mut_token().is_some(),
                        deferred: false,
                        reference,
                    };
                    self.declare(&n, local);
                }
            }
        }
        let Some(body) = f.body() else { return };
        let found = self.block(&body, Some(&ret));
        if !self.unify(&found, &ret) {
            let tail = body.stmt_list().and_then(|l| l.tail_expr());
            let node = tail
                .map(|t| t.syntax().clone())
                .unwrap_or_else(|| body.syntax().clone());
            let msg = format!(
                "mismatched types: expected `{}`, found `{}`",
                self.describe(&ret),
                self.describe(&found)
            );
            self.error(&node, msg);
        }
        self.resolve_collects();
    }

    fn block(&mut self, b: &ast::BlockExpr, expected: Option<&Ty>) -> Ty {
        if b.modifier().is_some() {
            return self.record(b.syntax(), Ty::Unknown); // async/unsafe blocks: codegen reports
        }
        let Some(list) = b.stmt_list() else {
            return self.record(b.syntax(), Ty::Unit);
        };
        self.scopes.push(HashMap::new());
        let mut diverged = false;
        for stmt in list.statements() {
            diverged |= self.stmt(&stmt);
        }
        let ty = match list.tail_expr() {
            Some(tail) => self.infer(&tail, expected),
            None if diverged => Ty::Never,
            None => Ty::Unit,
        };
        self.scopes.pop();
        self.record(b.syntax(), ty)
    }

    /// Returns true if the statement always diverges.
    fn stmt(&mut self, stmt: &ast::Stmt) -> bool {
        match stmt {
            ast::Stmt::LetStmt(l) => {
                self.let_stmt(l);
                false
            }
            ast::Stmt::ExprStmt(es) => match es.expr() {
                Some(e) => {
                    let t = self.infer(&e, None);
                    self.shallow(&t) == Ty::Never
                }
                None => false,
            },
            ast::Stmt::Item(_) => false,
        }
    }

    fn let_stmt(&mut self, l: &ast::LetStmt) {
        let declared = l.ty().map(|t| self.lower_type(&t));
        let declared_ref = l.ty().as_ref().and_then(Self::ref_kind);
        let init_ty = l.initializer().map(|init| {
            let t = self.infer(&init, declared.as_ref());
            if let Some(d) = &declared {
                self.expect(init.syntax(), &t, d);
            }
            t
        });
        let init_ref = match l.initializer() {
            Some(ast::Expr::RefExpr(r)) => Some(r.mut_token().is_some()),
            // `let v = map.entry(k).or_insert(0);` is a `&mut V` into the map.
            Some(ast::Expr::MethodCallExpr(m))
                if m.name_ref().is_some_and(|n| {
                    matches!(
                        n.text().to_string().as_str(),
                        "or_insert" | "or_default" | "or_insert_with" | "or_insert_with_key"
                    )
                }) =>
            {
                Some(true)
            }
            _ => None,
        };
        let Some(pat) = l.pat() else { return };
        let simple_binding = match &pat {
            ast::Pat::IdentPat(ip) => ip.pat().is_none() && !is_none_pat(ip),
            _ => false,
        };
        if let (ast::Pat::IdentPat(ip), true, None) = (&pat, simple_binding, l.let_else()) {
            let Some(name) = ip.name() else { return };
            let deferred = l.initializer().is_none();
            let ty = match (declared, init_ty) {
                (Some(d), _) => d,
                (None, Some(t)) => t,
                (None, None) => self.fresh(VarKind::General),
            };
            let local = Local {
                ty,
                mutable: ip.mut_token().is_some(),
                deferred,
                reference: declared_ref.or(init_ref),
            };
            self.declare(&name, local);
            return;
        }
        // A pattern: `let Some(x) = opt else { ... };`, `let x @ 1..=5 = ...`.
        let ty = declared.or(init_ty).unwrap_or(Ty::Unknown);
        // The `else` block runs without the pattern's bindings, so check it first.
        if let Some(le) = l.let_else() {
            if let Some(block) = le.block_expr() {
                let t = self.block(&block, None);
                if self.shallow(&t) != Ty::Never {
                    self.error(
                        block.syntax(),
                        "`else` clause of `let...else` does not diverge (it must `return`, `break`, `continue` or panic)",
                    );
                }
            }
        } else if !irrefutable(&pat) {
            self.error(
                pat.syntax(),
                "refutable pattern in local binding (use `let ... else { ... }` or `if let`)",
            );
        }
        self.pat(&pat, &ty);
    }

    // ------------------------------------------------------------ patterns

    /// Bindings from a pattern matched against `map.get_mut(..)`, `v.last_mut()`, `&mut x` and
    /// the like hold `&mut` references, so they can be mutated through without `mut`.
    fn mark_mut_ref_bindings(&mut self, scrutinee: Option<&ast::Expr>) {
        let mut_source = match scrutinee.map(strip_parens) {
            Some(ast::Expr::RefExpr(r)) => r.mut_token().is_some(),
            Some(ast::Expr::MethodCallExpr(m)) => m.name_ref().is_some_and(|n| {
                matches!(
                    n.text(),
                    "get_mut" | "first_mut" | "last_mut" | "as_mut" | "values_mut" | "iter_mut"
                )
            }),
            _ => false,
        };
        if mut_source && let Some(scope) = self.scopes.last_mut() {
            for local in scope.values_mut() {
                local.reference = Some(true);
            }
        }
    }

    fn let_condition(&mut self, le: &ast::LetExpr) {
        let st = le
            .expr()
            .map(|e| self.infer(&e, None))
            .unwrap_or(Ty::Unknown);
        if let Some(p) = le.pat() {
            self.pat(&p, &st);
            self.mark_mut_ref_bindings(le.expr().as_ref());
        }
        self.record(le.syntax(), Ty::Bool);
    }

    /// Check a pattern against the type of the value it matches, declaring its bindings in the
    /// current scope. Records the matched type on the pattern node for codegen.
    fn pat(&mut self, p: &ast::Pat, expected: &Ty) {
        self.record(p.syntax(), expected.clone());
        match p {
            ast::Pat::WildcardPat(_) => {}
            ast::Pat::ParenPat(pp) => {
                if let Some(inner) = pp.pat() {
                    self.pat(&inner, expected);
                }
            }
            ast::Pat::RefPat(rp) => {
                if let Some(inner) = rp.pat() {
                    self.pat(&inner, expected);
                }
            }
            ast::Pat::IdentPat(ip) if is_none_pat(ip) => self.expect_option(p, expected),
            ast::Pat::IdentPat(ip) => {
                if let Some(sub) = ip.pat() {
                    self.pat(&sub, expected);
                }
                if let Some(name) = ip.name() {
                    let reference = ip.ref_token().map(|_| ip.mut_token().is_some());
                    let local = Local {
                        ty: expected.clone(),
                        mutable: ip.mut_token().is_some(),
                        deferred: false,
                        reference,
                    };
                    self.declare(&name, local);
                }
            }
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => self.expect_option(p, expected),
            ast::Pat::PathPat(pp) if ordering_variant(pp.path()).is_some() => {
                if !self.unify(expected, &Ty::Ordering) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `Ordering`",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            ast::Pat::TupleStructPat(tp) if path_is(tp.path(), "Some") => {
                let inner = self.fresh(VarKind::General);
                if !self.unify(expected, &Ty::Option(Box::new(inner.clone()))) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `Option<_>`",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
                let fields: Vec<ast::Pat> = tp.fields().collect();
                if fields.len() != 1 {
                    self.error(
                        p.syntax(),
                        format!("`Some` has 1 field, but this pattern has {}", fields.len()),
                    );
                }
                if let Some(f) = fields.first() {
                    self.pat(f, &inner);
                }
            }
            ast::Pat::LiteralPat(lp) => {
                let t = match lp.literal() {
                    Some(l) => self.literal(&l),
                    None => Ty::Unknown,
                };
                if !self.unify(expected, &t) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `{}`",
                        self.describe(expected),
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            ast::Pat::RangePat(rp) => {
                for bound in [rp.start(), rp.end()].into_iter().flatten() {
                    self.pat(&bound, expected);
                }
                let ok = matches!(self.shallow(expected), Ty::Int(_) | Ty::Char | Ty::Unknown)
                    || self.var_kind(expected) == Some(VarKind::Int);
                if !ok {
                    let msg = format!(
                        "range patterns need an integer or `char`, found `{}`",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
            }
            ast::Pat::OrPat(op) => {
                if op
                    .syntax()
                    .descendants()
                    .filter_map(ast::IdentPat::cast)
                    .any(|ip| !is_none_pat(&ip))
                {
                    self.error(
                        p.syntax(),
                        "bindings inside `|` patterns are not supported yet",
                    );
                }
                for alt in op.pats() {
                    self.pat(&alt, expected);
                }
            }
            ast::Pat::TuplePat(tp) => self.tuple_pat(p, tp, expected),
            // Slices, structs and enum variants: codegen reports them.
            _ => {}
        }
    }

    fn tuple_pat(&mut self, p: &ast::Pat, tp: &ast::TuplePat, expected: &Ty) {
        let fields: Vec<ast::Pat> = tp.fields().collect();
        let rest = fields
            .iter()
            .position(|f| matches!(f, ast::Pat::RestPat(_)));
        let elems = match (self.shallow(expected), rest) {
            (Ty::Tuple(es), _) => es,
            (Ty::Unknown, _) => vec![Ty::Unknown; fields.len()],
            (_, None) => {
                let es: Vec<Ty> = fields
                    .iter()
                    .map(|_| self.fresh(VarKind::General))
                    .collect();
                if !self.unify(expected, &Ty::Tuple(es.clone())) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found a tuple",
                        self.describe(expected)
                    );
                    self.error(p.syntax(), msg);
                }
                es
            }
            (_, Some(_)) => {
                self.error(
                    p.syntax(),
                    "type annotations needed for a tuple pattern with `..`",
                );
                return;
            }
        };
        let n = elems.len();
        let named = fields.len() - usize::from(rest.is_some());
        if (rest.is_none() && named != n) || named > n {
            let msg = format!(
                "mismatched types: expected a tuple with {n} element{}, found one with {named}",
                if n == 1 { "" } else { "s" }
            );
            self.error(p.syntax(), msg);
            return;
        }
        for (i, f) in fields.iter().enumerate() {
            let index = match rest {
                Some(r) if i == r => continue,
                Some(r) if i > r => n - (fields.len() - i),
                _ => i,
            };
            self.pat(f, &elems[index]);
        }
    }

    fn expect_option(&mut self, p: &ast::Pat, expected: &Ty) {
        let fresh = self.fresh(VarKind::General);
        if !self.unify(expected, &Ty::Option(Box::new(fresh))) {
            let msg = format!(
                "mismatched types: expected `{}`, found `Option<_>`",
                self.describe(expected)
            );
            self.error(p.syntax(), msg);
        }
    }

    fn match_expr(&mut self, m: &ast::MatchExpr, expected: Option<&Ty>) -> Ty {
        let st = m
            .expr()
            .map(|e| self.infer(&e, None))
            .unwrap_or(Ty::Unknown);
        let arms: Vec<ast::MatchArm> = m
            .match_arm_list()
            .map(|l| l.arms().collect())
            .unwrap_or_default();
        let mut result: Option<Ty> = None;
        for arm in &arms {
            self.scopes.push(HashMap::new());
            if let Some(p) = arm.pat() {
                self.pat(&p, &st);
                self.mark_mut_ref_bindings(m.expr().as_ref());
            }
            if let Some(g) = arm.guard().and_then(|g| g.condition()) {
                self.check(&g, &Ty::Bool);
            }
            if let Some(body) = arm.expr() {
                let hint = result.clone().or_else(|| expected.cloned());
                let t = self.infer(&body, hint.as_ref());
                if self.shallow(&t) != Ty::Never {
                    match result.clone() {
                        None => result = Some(t),
                        Some(r) => {
                            if !self.unify(&r, &t) {
                                let msg = format!(
                                    "`match` arms have incompatible types: expected `{}`, found `{}`",
                                    self.describe(&r),
                                    self.describe(&t)
                                );
                                self.error(body.syntax(), msg);
                            }
                        }
                    }
                }
            }
            self.scopes.pop();
        }
        self.check_exhaustive(m, &arms, &st);
        result.unwrap_or(Ty::Never)
    }

    /// Exhaustiveness via the classic pattern-matrix "usefulness" algorithm over the constructors
    /// CarciniScript knows: `bool`, `Option` and tuples. Integers, chars and strings have
    /// effectively infinite domains, so covering them takes a catch-all arm.
    fn check_exhaustive(&mut self, m: &ast::MatchExpr, arms: &[ast::MatchArm], st: &Ty) {
        let mut rows = Vec::new();
        for arm in arms.iter().filter(|a| a.guard().is_none()) {
            if let Some(p) = arm.pat() {
                rows.extend(self.abstract_pat(&p).into_iter().map(|ap| vec![ap]));
            }
        }
        if let Some(witness) = self.missing(&rows, std::slice::from_ref(st)) {
            let node = m
                .expr()
                .map(|e| e.syntax().clone())
                .unwrap_or_else(|| m.syntax().clone());
            self.error(
                &node,
                format!("non-exhaustive patterns: `{}` not covered", witness[0]),
            );
        }
    }

    /// A pattern as constructors, one entry per `|` alternative.
    fn abstract_pat(&self, p: &ast::Pat) -> Vec<Ap> {
        match strip_pat(p) {
            ast::Pat::WildcardPat(_) => vec![Ap::Wild],
            ast::Pat::IdentPat(ip) if is_none_pat(&ip) => vec![Ap::None],
            ast::Pat::IdentPat(ip) => ip
                .pat()
                .map(|sub| self.abstract_pat(&sub))
                .unwrap_or_else(|| vec![Ap::Wild]),
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => vec![Ap::None],
            ast::Pat::PathPat(pp) if ordering_variant(pp.path()).is_some() => {
                vec![Ap::Ord(ordering_variant(pp.path()).unwrap())]
            }
            ast::Pat::TupleStructPat(tp) if path_is(tp.path(), "Some") => {
                match tp.fields().next() {
                    Some(f) => self
                        .abstract_pat(&f)
                        .into_iter()
                        .map(|a| Ap::Some(Box::new(a)))
                        .collect(),
                    None => vec![Ap::Other],
                }
            }
            ast::Pat::LiteralPat(lp) => match lp.literal().map(|l| l.kind()) {
                Some(LiteralKind::Bool(b)) => vec![Ap::Bool(b)],
                _ => vec![Ap::Other],
            },
            ast::Pat::OrPat(op) => op.pats().flat_map(|alt| self.abstract_pat(&alt)).collect(),
            ast::Pat::TuplePat(tp) => {
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let arity = match self.raw.get(&self.range(p.syntax())).map(|t| self.zonk(t)) {
                    Some(Ty::Tuple(es)) => es.len(),
                    _ => fields.len(),
                };
                // Expand `..` into wildcards, then take the product of each element's alternatives.
                let mut cols: Vec<Vec<Ap>> = Vec::new();
                for f in &fields {
                    if matches!(f, ast::Pat::RestPat(_)) {
                        let missing = arity.saturating_sub(fields.len() - 1);
                        cols.extend(std::iter::repeat_n(vec![Ap::Wild], missing));
                    } else {
                        cols.push(self.abstract_pat(f));
                    }
                }
                let mut product: Vec<Vec<Ap>> = vec![vec![]];
                for col in cols {
                    product = product
                        .into_iter()
                        .flat_map(|prefix| {
                            col.iter()
                                .map(move |c| [prefix.clone(), vec![c.clone()]].concat())
                        })
                        .collect();
                    if product.len() > 512 {
                        return vec![Ap::Wild]; // too many combinations to check; assume covered
                    }
                }
                product.into_iter().map(Ap::Tuple).collect()
            }
            _ => vec![Ap::Other],
        }
    }

    /// A value not matched by any row (one rendered pattern per column), or None if every value
    /// is matched.
    fn missing(&self, rows: &[Vec<Ap>], tys: &[Ty]) -> Option<Vec<String>> {
        let Some(ty) = tys.first() else {
            return if rows.is_empty() { Some(vec![]) } else { None };
        };
        let rest = &tys[1..];
        // The constructor family of this column: from its type, or from its patterns when the type
        // is unknown.
        let head_kind = rows
            .iter()
            .map(|r| &r[0])
            .find(|a| !matches!(a, Ap::Wild | Ap::Other));
        let family = match (self.zonk(ty), head_kind) {
            (Ty::Bool, _) | (Ty::Unknown, Some(Ap::Bool(_))) => Family::Bool,
            (Ty::Ordering, _) | (Ty::Unknown, Some(Ap::Ord(_))) => Family::Ordering,
            (Ty::Option(inner), _) => Family::Option(*inner),
            (Ty::Unknown, Some(Ap::None | Ap::Some(_))) => Family::Option(Ty::Unknown),
            (Ty::Tuple(es), _) => Family::Tuple(es),
            (Ty::Unknown, Some(Ap::Tuple(ps))) => Family::Tuple(vec![Ty::Unknown; ps.len()]),
            _ => Family::Infinite,
        };
        let specialize = |ctor: &Ap, arity: usize| -> Vec<Vec<Ap>> {
            rows.iter()
                .filter_map(|r| {
                    let fields = match (&r[0], ctor) {
                        (Ap::Wild, _) => vec![Ap::Wild; arity],
                        (Ap::Bool(a), Ap::Bool(b)) if a == b => vec![],
                        (Ap::Ord(a), Ap::Ord(b)) if a == b => vec![],
                        (Ap::None, Ap::None) => vec![],
                        (Ap::Some(x), Ap::Some(_)) => vec![(**x).clone()],
                        (Ap::Tuple(xs), Ap::Tuple(_)) if xs.len() == arity => xs.clone(),
                        _ => return None,
                    };
                    Some([fields, r[1..].to_vec()].concat())
                })
                .collect()
        };
        match family {
            Family::Tuple(es) => {
                let n = es.len();
                let w = self.missing(
                    &specialize(&Ap::Tuple(vec![]), n),
                    &[es, rest.to_vec()].concat(),
                )?;
                let head = crate::ty::describe_tuple(w[..n].to_vec());
                Some([vec![head], w[n..].to_vec()].concat())
            }
            Family::Ordering => [-1i8, 0, 1].into_iter().find_map(|o| {
                let w = self.missing(&specialize(&Ap::Ord(o), 0), rest)?;
                let name =
                    ["Ordering::Less", "Ordering::Equal", "Ordering::Greater"][(o + 1) as usize];
                Some([vec![name.to_string()], w].concat())
            }),
            Family::Bool => [true, false].into_iter().find_map(|b| {
                let w = self.missing(&specialize(&Ap::Bool(b), 0), rest)?;
                Some([vec![b.to_string()], w].concat())
            }),
            Family::Option(inner) => {
                if let Some(w) = self.missing(&specialize(&Ap::None, 0), rest) {
                    return Some([vec!["None".to_string()], w].concat());
                }
                let w = self.missing(
                    &specialize(&Ap::Some(Box::new(Ap::Wild)), 1),
                    &[vec![inner], rest.to_vec()].concat(),
                )?;
                Some([vec![format!("Some({})", w[0])], w[1..].to_vec()].concat())
            }
            Family::Infinite => {
                let defaults: Vec<Vec<Ap>> = rows
                    .iter()
                    .filter(|r| matches!(r[0], Ap::Wild))
                    .map(|r| r[1..].to_vec())
                    .collect();
                let w = self.missing(&defaults, rest)?;
                Some([vec!["_".to_string()], w].concat())
            }
        }
    }

    // ------------------------------------------------------------ expressions

    fn infer(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> Ty {
        let ty = self.infer_inner(e, expected);
        self.record(e.syntax(), ty)
    }

    /// Infer `e` and require it to have type `expected`.
    fn check(&mut self, e: &ast::Expr, expected: &Ty) -> Ty {
        let t = self.infer(e, Some(expected));
        self.expect(e.syntax(), &t, expected);
        t
    }

    fn infer_inner(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> Ty {
        match e {
            ast::Expr::Literal(l) => self.literal(l),
            ast::Expr::PathExpr(p) => self.path(p),
            ast::Expr::ParenExpr(p) => p
                .expr()
                .map(|i| self.infer(&i, expected))
                .unwrap_or(Ty::Unit),
            ast::Expr::RefExpr(r) => {
                let Some(inner) = r.expr() else {
                    return Ty::Unknown;
                };
                if r.mut_token().is_some() {
                    self.check_mutable(&inner, Mutation::Borrow);
                }
                self.infer(&inner, expected)
            }
            ast::Expr::PrefixExpr(p) => self.prefix(p, expected),
            ast::Expr::BinExpr(b) => self.bin(b, expected),
            ast::Expr::CastExpr(c) => {
                if let Some(inner) = c.expr() {
                    self.infer(&inner, None);
                }
                c.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown)
            }
            ast::Expr::CallExpr(c) => self.call(c),
            ast::Expr::MethodCallExpr(m) => self.method(m, expected),
            ast::Expr::FieldExpr(f) => {
                let Some(recv) = f.expr() else {
                    return Ty::Unknown;
                };
                let rt = self.infer(&recv, None);
                match self.shallow(&rt) {
                    Ty::Extern(name) => self.extern_field(f, &name),
                    Ty::Unknown | Ty::Var(_) => Ty::Unknown,
                    Ty::Tuple(es) if f.name_ref().is_some_and(|n| n.as_tuple_field().is_some()) => {
                        let i = f.name_ref().and_then(|n| n.as_tuple_field()).unwrap_or(0);
                        match es.get(i) {
                            Some(t) => t.clone(),
                            None => {
                                let msg = format!(
                                    "no field `{i}` on type `{}`",
                                    self.describe(&Ty::Tuple(es.clone()))
                                );
                                self.error(f.syntax(), msg);
                                Ty::Unknown
                            }
                        }
                    }
                    other => {
                        let name = f
                            .name_ref()
                            .map(|n| n.text().to_string())
                            .unwrap_or_default();
                        let msg = format!("no field `{name}` on type `{}`", self.describe(&other));
                        self.error(f.syntax(), msg);
                        Ty::Unknown
                    }
                }
            }
            ast::Expr::AwaitExpr(a) => {
                if !self.in_async {
                    self.error(
                        a.syntax(),
                        "`await` is only allowed inside `async` functions and blocks",
                    );
                }
                let Some(inner) = a.expr() else {
                    return Ty::Unknown;
                };
                let t = self.infer(&inner, None);
                match self.shallow(&t) {
                    Ty::Future(t) => *t,
                    Ty::Unknown | Ty::Var(_) => Ty::Unknown,
                    other => {
                        let msg = format!("`{}` is not a future", self.describe(&other));
                        self.error(a.syntax(), msg);
                        Ty::Unknown
                    }
                }
            }
            ast::Expr::MacroExpr(m) => match m.macro_call() {
                Some(mc) => self.macro_call(&mc, expected),
                None => Ty::Unknown,
            },
            ast::Expr::IfExpr(i) => self.if_expr(i, expected),
            ast::Expr::MatchExpr(m) => self.match_expr(m, expected),
            ast::Expr::ClosureExpr(c) => self.closure(c, expected),
            ast::Expr::RangeExpr(r) => {
                let elem = self.range_elem(r);
                Ty::Iter {
                    elem: Box::new(elem),
                    mutable: false,
                }
            }
            ast::Expr::LetExpr(le) => {
                // A `let` that isn't the whole condition of an `if`/`while`: a let-chain.
                self.error(
                    le.syntax(),
                    "`let` chains (`if let ... && ...`) are not supported yet",
                );
                if let Some(e) = le.expr() {
                    self.infer(&e, None);
                }
                Ty::Bool
            }
            ast::Expr::BlockExpr(b) => self.block(b, expected),
            ast::Expr::LoopExpr(l) => {
                if let Some(body) = l.loop_body() {
                    self.block(&body, None);
                }
                Ty::Unit
            }
            ast::Expr::WhileExpr(w) => {
                self.scopes.push(HashMap::new());
                match w.condition() {
                    Some(ast::Expr::LetExpr(le)) => self.let_condition(&le),
                    Some(c) => {
                        self.check(&c, &Ty::Bool);
                    }
                    None => {}
                }
                if let Some(body) = w.loop_body() {
                    self.block(&body, None);
                }
                self.scopes.pop();
                Ty::Unit
            }
            ast::Expr::ForExpr(f) => {
                self.for_expr(f);
                Ty::Unit
            }
            ast::Expr::ReturnExpr(r) => {
                let ret = self.ret_ty.clone();
                match r.expr() {
                    Some(v) => {
                        self.check(&v, &ret);
                    }
                    None if !self.unify(&Ty::Unit, &ret) => {
                        let msg = format!(
                            "mismatched types: expected `{}`, found `()`",
                            self.describe(&ret)
                        );
                        self.error(r.syntax(), msg);
                    }
                    None => {}
                }
                Ty::Never
            }
            ast::Expr::BreakExpr(b) => {
                if let Some(v) = b.expr() {
                    self.infer(&v, None);
                }
                Ty::Never
            }
            ast::Expr::ContinueExpr(_) => Ty::Never,
            ast::Expr::TupleExpr(t) => {
                let fields: Vec<_> = t.fields().collect();
                if fields.is_empty() {
                    return Ty::Unit;
                }
                let hints = match expected.map(|e| self.shallow(e)) {
                    Some(Ty::Tuple(es)) if es.len() == fields.len() => es,
                    _ => vec![],
                };
                Ty::Tuple(
                    fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| self.infer(f, hints.get(i)))
                        .collect(),
                )
            }
            ast::Expr::IndexExpr(ix) => self.index(ix),
            ast::Expr::TryExpr(t) => self.try_expr(t),
            ast::Expr::ArrayExpr(a) => {
                let elem = match expected.map(|t| self.shallow(t)) {
                    Some(Ty::Vec(e)) => *e,
                    _ => self.fresh(VarKind::General),
                };
                match a.kind() {
                    ast::ArrayExprKind::ElementList(items) => {
                        for item in items {
                            self.check(&item, &elem);
                        }
                    }
                    ast::ArrayExprKind::Repeat {
                        initializer,
                        repeat,
                    } => {
                        if let Some(init) = initializer {
                            self.check(&init, &elem);
                        }
                        if let Some(count) = repeat {
                            self.check(&count, &USIZE);
                        }
                    }
                }
                Ty::Vec(Box::new(elem))
            }
            _ => {
                // Unsupported expressions (codegen reports them). Still visit sub-expressions so
                // their names resolve and their errors surface.
                for child in e.syntax().children().filter_map(ast::Expr::cast) {
                    self.infer(&child, None);
                }
                Ty::Unknown
            }
        }
    }

    fn literal(&mut self, l: &ast::Literal) -> Ty {
        match l.kind() {
            LiteralKind::IntNumber(n) => match n.suffix() {
                Some(s) => Ty::from_name(s),
                None => self.fresh(VarKind::Int),
            },
            LiteralKind::FloatNumber(n) => match n.suffix() {
                Some(s) => Ty::from_name(s),
                None => self.fresh(VarKind::Float),
            },
            LiteralKind::Bool(_) => Ty::Bool,
            LiteralKind::String(_) => Ty::Str,
            LiteralKind::Char(_) => Ty::Char,
            LiteralKind::Byte(_) => Ty::Int(IntKind::U8),
            _ => Ty::Unknown,
        }
    }

    fn path(&mut self, p: &ast::PathExpr) -> Ty {
        let Some(path) = p.path() else {
            return Ty::Unknown;
        };
        if let Some(name) = path.as_single_name_ref() {
            if let Some(local) = self.lookup(name.text()) {
                return local.ty.clone();
            }
            if name.text() == "None" {
                return Ty::Option(Box::new(self.fresh(VarKind::General)));
            }
            if let Some(sig) = self.fns.get(name.text()) {
                let ret = if sig.is_async {
                    Ty::Future(Box::new(sig.ret.clone()))
                } else {
                    sig.ret.clone()
                };
                return Ty::Closure {
                    params: sig.params.clone(),
                    ret: Box::new(ret),
                };
            }
            self.error(
                p.syntax(),
                format!("cannot find value `{}` in this scope", name.text()),
            );
            return Ty::Unknown;
        }
        let segs: Vec<String> = path
            .segments()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect();
        let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
        if let [.., "Ordering", "Less" | "Equal" | "Greater"] = segs.as_slice() {
            return Ty::Ordering;
        }
        if let Some((_, ty)) = std_const(std_tail(&segs)) {
            return ty;
        }
        self.error(
            p.syntax(),
            format!(
                "cannot find value `{}` (paths are resolved in phase 2)",
                path.syntax().text()
            ),
        );
        Ty::Unknown
    }

    fn prefix(&mut self, p: &ast::PrefixExpr, expected: Option<&Ty>) -> Ty {
        let Some(inner) = p.expr() else {
            return Ty::Unknown;
        };
        let t = self.infer(&inner, expected);
        let resolved = self.shallow(&t);
        match p.op_kind() {
            Some(UnaryOp::Deref) | None => t,
            Some(UnaryOp::Neg) => {
                let ok = match &resolved {
                    Ty::Int(k) => k.is_signed(),
                    Ty::Float { .. } | Ty::Var(_) | Ty::Unknown | Ty::Never => true,
                    _ => false,
                };
                if !ok {
                    let msg = format!(
                        "cannot apply unary operator `-` to type `{}`",
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
                t
            }
            Some(UnaryOp::Not) => {
                let ok = matches!(resolved, Ty::Bool | Ty::Int(_) | Ty::Unknown | Ty::Never)
                    || self.var_kind(&t).is_some_and(|k| k != VarKind::Float);
                if !ok {
                    let msg = format!(
                        "cannot apply unary operator `!` to type `{}`",
                        self.describe(&t)
                    );
                    self.error(p.syntax(), msg);
                }
                t
            }
        }
    }

    /// Root local of a place expression (`v`, `v[i]`, `v[i][j]`, `*r`), plus whether the place is
    /// reached through indexing or a deref (a mutation *through* the binding, not of it).
    fn place_root(e: &ast::Expr) -> Option<(ast::NameRef, bool)> {
        match e {
            ast::Expr::PathExpr(p) => Some((p.path()?.as_single_name_ref()?, false)),
            ast::Expr::ParenExpr(p) => Self::place_root(&p.expr()?),
            ast::Expr::IndexExpr(ix) => Some((Self::place_root(&ix.base()?)?.0, true)),
            ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => {
                Some((Self::place_root(&p.expr()?)?.0, true))
            }
            ast::Expr::RefExpr(r) => Self::place_root(&r.expr()?),
            ast::Expr::FieldExpr(f) => Some((Self::place_root(&f.expr()?)?.0, true)),
            _ => None,
        }
    }

    /// Whether `e` is `map[..]` on a `HashMap`, which can be read but not assigned through.
    fn indexes_a_map(&mut self, e: &ast::Expr) -> bool {
        let ast::Expr::IndexExpr(ix) = strip_parens(e) else {
            return false;
        };
        let Some(b) = ix.base() else { return false };
        let t = self.infer(&b, None);
        matches!(self.shallow(&t), Ty::Map(..))
    }

    /// rustc's mutability rules for assigning to or mutably borrowing a place.
    fn check_mutable(&mut self, e: &ast::Expr, how: Mutation) {
        let Some((name, through)) = Self::place_root(e) else {
            return;
        };
        let Some(local) = self.lookup(name.text()).cloned() else {
            return;
        };
        let n = name.text();
        if how == Mutation::Assign && !through {
            if !local.mutable && !local.deferred {
                self.error(e.syntax(), format!("cannot assign twice to immutable variable `{n}` (declare it with `let mut`)"));
            }
            return;
        }
        match local.reference {
            Some(true) => {}
            Some(false) => self.error(
                e.syntax(),
                format!("cannot borrow `*{n}` as mutable, as it is behind a `&` reference"),
            ),
            None if !local.mutable => self.error(
                e.syntax(),
                format!("cannot borrow `{n}` as mutable, as it is not declared as mutable"),
            ),
            None => {}
        }
    }

    fn bin(&mut self, b: &ast::BinExpr, expected: Option<&Ty>) -> Ty {
        let (Some(lhs), Some(rhs), Some(op)) = (b.lhs(), b.rhs(), b.op_kind()) else {
            return Ty::Unknown;
        };
        match op {
            BinaryOp::Assignment { op: None } if matches!(lhs, ast::Expr::TupleExpr(_)) => {
                let ast::Expr::TupleExpr(t) = &lhs else {
                    unreachable!()
                };
                let mut tys = Vec::new();
                for f in t.fields() {
                    let through = Self::place_root(&f).is_some_and(|(_, t)| t);
                    self.check_mutable(
                        &f,
                        if through {
                            Mutation::Borrow
                        } else {
                            Mutation::Assign
                        },
                    );
                    tys.push(self.infer(&f, None));
                }
                self.record(lhs.syntax(), Ty::Tuple(tys.clone()));
                self.check(&rhs, &Ty::Tuple(tys));
                Ty::Unit
            }
            BinaryOp::Assignment { .. } if self.indexes_a_map(&lhs) => {
                self.infer(&lhs, None);
                self.infer(&rhs, None);
                self.error(
                    lhs.syntax(),
                    "cannot assign to data in an index of a `HashMap` (use `map.insert(k, v)` or `*map.entry(k).or_insert(..) = ..`)",
                );
                Ty::Unit
            }
            BinaryOp::Assignment { op } => {
                let through = Self::place_root(&lhs).is_some_and(|(_, t)| t);
                self.check_mutable(
                    &lhs,
                    if through {
                        Mutation::Borrow
                    } else {
                        Mutation::Assign
                    },
                );
                let lt = self.infer(&lhs, None);
                match op {
                    Some(ArithOp::Shl | ArithOp::Shr) => {
                        self.infer(&rhs, None);
                    }
                    Some(op) => {
                        self.arith_operands(b, op, &lt, &rhs);
                    }
                    None => {
                        self.check(&rhs, &lt);
                    }
                }
                Ty::Unit
            }
            BinaryOp::LogicOp(_) => {
                self.check(&lhs, &Ty::Bool);
                self.check(&rhs, &Ty::Bool);
                Ty::Bool
            }
            BinaryOp::CmpOp(cmp) => {
                let lt = self.infer(&lhs, None);
                let rt = self.infer(&rhs, Some(&lt));
                if !self.unify(&lt, &rt) {
                    let msg = format!(
                        "mismatched types: expected `{}`, found `{}`",
                        self.describe(&lt),
                        self.describe(&rt)
                    );
                    self.error(rhs.syntax(), msg);
                }
                if matches!(cmp, CmpOp::Ord { .. })
                    && matches!(self.shallow(&lt), Ty::Set(_) | Ty::Option(_) | Ty::Map(..))
                {
                    let msg = format!("`{}` can't be compared with `<`/`>`", self.describe(&lt));
                    self.error(b.syntax(), msg);
                }
                Ty::Bool
            }
            BinaryOp::ArithOp(op @ (ArithOp::Shl | ArithOp::Shr)) => {
                let lt = self.infer(&lhs, expected);
                let rt = self.infer(&rhs, None);
                for (t, side) in [(&lt, &lhs), (&rt, &rhs)] {
                    if !self.is_integer(t) {
                        let msg = format!(
                            "no implementation for `{} {op} {}`",
                            self.describe(&lt),
                            self.describe(&rt)
                        );
                        self.error(side.syntax(), msg);
                        break;
                    }
                }
                lt
            }
            BinaryOp::ArithOp(op) => {
                let lt = self.infer(&lhs, expected);
                self.arith_operands(b, op, &lt, &rhs)
            }
        }
    }

    fn is_integer(&self, t: &Ty) -> bool {
        matches!(self.shallow(t), Ty::Int(_) | Ty::Unknown | Ty::Never)
            || self.var_kind(t) == Some(VarKind::Int)
    }

    /// Check the right operand of `lhs op rhs` (or `lhs op= rhs`) and the operator's validity.
    fn arith_operands(&mut self, b: &ast::BinExpr, op: ArithOp, lt: &Ty, rhs: &ast::Expr) -> Ty {
        if self.shallow(lt) == Ty::Str && op == ArithOp::Add {
            self.check(rhs, &Ty::Str);
            return Ty::Str;
        }
        let rt = self.infer(rhs, Some(lt));
        if !self.unify(lt, &rt) {
            let (l, r) = (self.describe(lt), self.describe(&rt));
            let msg = match op {
                ArithOp::Add => format!("cannot add `{r}` to `{l}`"),
                ArithOp::Sub => format!("cannot subtract `{r}` from `{l}`"),
                ArithOp::Mul => format!("cannot multiply `{l}` by `{r}`"),
                ArithOp::Div => format!("cannot divide `{l}` by `{r}`"),
                _ => format!("no implementation for `{l} {op} {r}`"),
            };
            self.error(b.syntax(), msg);
            return lt.clone();
        }
        let resolved = self.shallow(lt);
        let bitwise = matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor);
        let ok = match &resolved {
            Ty::Int(_) | Ty::Unknown | Ty::Never => true,
            Ty::Float { .. } => !bitwise,
            Ty::Bool => bitwise,
            Ty::Var(_) => !(bitwise && self.var_kind(lt) == Some(VarKind::Float)),
            _ => false,
        };
        if !ok {
            let msg = format!(
                "cannot apply binary operator `{op}` to type `{}`",
                self.describe(lt)
            );
            self.error(b.syntax(), msg);
        }
        lt.clone()
    }

    fn arg_count(&mut self, n: &SyntaxNode, what: &str, expected: usize, found: usize) {
        if expected != found {
            let s = if expected == 1 { "" } else { "s" };
            self.error(
                n,
                format!("this {what} takes {expected} argument{s} but {found} were supplied"),
            );
        }
    }

    fn call(&mut self, c: &ast::CallExpr) -> Ty {
        let args: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let Some(ast::Expr::PathExpr(callee)) = c.expr() else {
            // `make_adder(1)(2)`: call whatever the expression evaluates to.
            let ft = c
                .expr()
                .map(|e| self.infer(&e, None))
                .unwrap_or(Ty::Unknown);
            if let Ty::Closure { params, ret } = self.shallow(&ft) {
                self.arg_count(c.syntax(), "closure", params.len(), args.len());
                for (a, p) in args.iter().zip(params.iter()) {
                    self.check(a, p);
                }
                return *ret;
            }
            for a in &args {
                self.infer(a, None);
            }
            return Ty::Unknown;
        };
        let Some(path) = callee.path() else {
            return Ty::Unknown;
        };
        if let Some(name) = path.as_single_name_ref() {
            if name.text() == "Some"
                && self.lookup("Some").is_none()
                && !self.fns.contains_key("Some")
            {
                return self.some_call(c, &args);
            }
            if let Some(local) = self.lookup(name.text())
                && let Ty::Closure { params, ret } = self.shallow(&local.ty.clone())
            {
                self.arg_count(c.syntax(), "closure", params.len(), args.len());
                for (a, p) in args.iter().zip(params.iter()) {
                    self.check(a, p);
                }
                return *ret;
            }
            if let Some(local) = self.lookup(name.text()) {
                let lt = self.shallow(&local.ty.clone());
                let numeric = matches!(self.var_kind(&lt), Some(VarKind::Int | VarKind::Float));
                if !matches!(lt, Ty::Var(_) | Ty::Unknown) || numeric {
                    let msg = format!("expected function, found `{}`", self.describe(&lt));
                    self.error(callee.syntax(), msg);
                }
                for a in &args {
                    self.infer(a, None);
                }
                return Ty::Unknown;
            }
            let Some(sig) = self.fns.get(name.text()) else {
                self.error(
                    callee.syntax(),
                    format!("cannot find function `{}` in this scope", name.text()),
                );
                for a in &args {
                    self.infer(a, None);
                }
                return Ty::Unknown;
            };
            let (params, ret, is_async) = (sig.params.clone(), sig.ret.clone(), sig.is_async);
            self.arg_count(c.syntax(), "function", params.len(), args.len());
            for (i, a) in args.iter().enumerate() {
                match params.get(i) {
                    Some(p) => {
                        self.check(a, p);
                    }
                    None => {
                        self.infer(a, None);
                    }
                }
            }
            return if is_async {
                Ty::Future(Box::new(ret))
            } else {
                ret
            };
        }

        let segs: Vec<ast::PathSegment> = path.segments().collect();
        let names: Vec<String> = segs
            .iter()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        match std_tail(&names) {
            ["String", "new"] => {
                self.arg_count(c.syntax(), "function", 0, args.len());
                Ty::Str
            }
            ["String", "from"] => {
                self.arg_count(c.syntax(), "function", 1, args.len());
                if let Some(a) = args.first() {
                    self.check(a, &Ty::Str);
                }
                Ty::Str
            }
            ["Vec" | "vec", ctor @ ("new" | "with_capacity")] => {
                self.arg_count(
                    c.syntax(),
                    "function",
                    usize::from(*ctor == "with_capacity"),
                    args.len(),
                );
                if let Some(a) = args.first() {
                    self.check(a, &USIZE);
                }
                // `Vec::<T>::new()` names the element type.
                let named = segs
                    .iter()
                    .find(|s| s.name_ref().is_some_and(|n| n.text() == "Vec"))
                    .and_then(|s| s.generic_arg_list())
                    .and_then(|l| l.generic_args().next());
                let elem = match named {
                    Some(ast::GenericArg::TypeArg(ta)) => ta.ty().map(|t| self.lower_type(&t)),
                    _ => None,
                };
                Ty::Vec(Box::new(
                    elem.unwrap_or_else(|| self.fresh(VarKind::General)),
                ))
            }
            [.., "HashMap", ctor @ ("new" | "with_capacity" | "from")] => {
                let (k, v) = (self.fresh(VarKind::General), self.fresh(VarKind::General));
                if *ctor == "from" {
                    self.arg_count(c.syntax(), "function", 1, args.len());
                    if let Some(a) = args.first() {
                        let pairs = Ty::Vec(Box::new(Ty::Tuple(vec![k.clone(), v.clone()])));
                        self.check(a, &pairs);
                    }
                } else {
                    self.arg_count(
                        c.syntax(),
                        "function",
                        usize::from(*ctor == "with_capacity"),
                        args.len(),
                    );
                    if let Some(a) = args.first() {
                        self.check(a, &USIZE);
                    }
                }
                Ty::Map(Box::new(k), Box::new(v))
            }
            [.., "HashSet", ctor @ ("new" | "with_capacity" | "from")] => {
                let ctor = *ctor;
                let elem = self.fresh(VarKind::General);
                match ctor {
                    "from" => {
                        self.arg_count(c.syntax(), "function", 1, args.len());
                        if let Some(a) = args.first() {
                            self.check(a, &Ty::Vec(Box::new(elem.clone())));
                        }
                    }
                    _ => {
                        self.arg_count(
                            c.syntax(),
                            "function",
                            usize::from(ctor == "with_capacity"),
                            args.len(),
                        );
                        if let Some(a) = args.first() {
                            self.check(a, &USIZE);
                        }
                    }
                }
                Ty::Set(Box::new(elem))
            }
            ["cmp", "max" | "min"] => {
                self.arg_count(c.syntax(), "function", 2, args.len());
                let t = args
                    .first()
                    .map(|a| self.infer(a, None))
                    .unwrap_or(Ty::Unknown);
                if let Some(b) = args.get(1) {
                    self.check(b, &t);
                }
                t
            }
            [ty_name, "from"] if Ty::from_name(ty_name).is_numeric() => {
                for a in &args {
                    self.infer(a, None);
                }
                Ty::from_name(ty_name)
            }
            _ => {
                self.error(
                    callee.syntax(),
                    format!(
                        "cannot find function `{}` (paths are resolved in phase 2)",
                        path.syntax().text()
                    ),
                );
                for a in &args {
                    self.infer(a, None);
                }
                Ty::Unknown
            }
        }
    }

    /// A field of a JS interface from the bindings.
    fn extern_field(&mut self, f: &ast::FieldExpr, name: &str) -> Ty {
        let field = f.name_ref().map(|n| bare(n.text())).unwrap_or_default();
        let b = self.bindings.clone();
        let Some(et) = b.types.get(name) else {
            return Ty::Extern(name.to_string()); // untyped NS: namespaces stay untyped too
        };
        if let Some(fd) = et.fields.get(&field) {
            return fd.ty.clone();
        }
        let msg = if et.methods.contains_key(&field) {
            format!("`{field}` is a method on `{name}`, not a field (call it: `.{field}()`)")
        } else {
            let hint = bindings::suggest(&field, et.fields.keys())
                .map(|s| format!("; did you mean `{s}`?"))
                .unwrap_or_default();
            format!("no field `{field}` on type `{name}`{hint}")
        };
        self.error(f.syntax(), msg);
        Ty::Unknown
    }

    /// A method call on a JS interface: arguments and return type from the bindings.
    fn extern_method(
        &mut self,
        m: &ast::MethodCallExpr,
        name: &str,
        method: &str,
        args: &[ast::Expr],
    ) -> Ty {
        let b = self.bindings.clone();
        let loose = |c: &mut Self| {
            for a in args {
                c.infer(a, None);
            }
        };
        let Some(et) = b.types.get(name) else {
            loose(self);
            return Ty::Unknown; // untyped NS
        };
        let Some(sig) = et.methods.get(&bare(method)) else {
            let msg = if et.fields.contains_key(&bare(method)) {
                format!("`{method}` is a field of `{name}`, not a method (drop the parentheses)")
            } else {
                let hint = bindings::suggest(method, et.methods.keys())
                    .map(|s| format!("; did you mean `{s}`?"))
                    .unwrap_or_default();
                format!("no method named `{method}` found for `{name}`{hint}")
            };
            self.error(m.syntax(), msg);
            loose(self);
            return Ty::Unknown;
        };
        let required = sig.params.iter().filter(|p| !p.optional).count();
        let max = if sig.variadic.is_some() {
            usize::MAX
        } else {
            sig.params.len()
        };
        if args.len() < required || args.len() > max {
            let plural = |n: usize| if n == 1 { "" } else { "s" };
            let range = match (required, max) {
                (r, m) if r == m => format!("{r} argument{}", plural(r)),
                (r, usize::MAX) => format!("at least {r} argument{}", plural(r)),
                (r, m) => format!("{r} to {m} arguments"),
            };
            self.error(
                m.syntax(),
                format!(
                    "`{name}::{method}` takes {range} but {} were supplied",
                    args.len()
                ),
            );
        }
        for (i, a) in args.iter().enumerate() {
            let ty = match sig.params.get(i) {
                Some(p) => p.ty.clone(),
                None => sig.variadic.clone().unwrap_or(Ty::Unknown),
            };
            if ty == Ty::Unknown {
                self.infer(a, None);
            } else if matches!(ty, Ty::Float { .. }) && is_int_literal(a) {
                // Deliberate extension: every NS number is `f64`, and `ns.sleep(100)` is how every
                // Bitburner script reads, so unsuffixed integer literals are accepted here.
                self.record(a.syntax(), ty);
            } else {
                self.check(a, &ty);
            }
        }
        if sig.is_async {
            Ty::Future(Box::new(sig.ret.clone()))
        } else {
            sig.ret.clone()
        }
    }

    /// `expr?` on an `Option`: the value, or an early `return None`.
    fn try_expr(&mut self, t: &ast::TryExpr) -> Ty {
        let Some(inner) = t.expr() else {
            return Ty::Unknown;
        };
        let it = self.infer(&inner, None);
        let value = match self.shallow(&it) {
            Ty::Option(x) => *x,
            Ty::Unknown => return Ty::Unknown,
            Ty::Var(_) if self.var_kind(&it) == Some(VarKind::General) => {
                let x = self.fresh(VarKind::General);
                self.unify(&it, &Ty::Option(Box::new(x.clone())));
                x
            }
            other => {
                let msg = format!(
                    "the `?` operator can only be applied to `Option` (found `{}`); `Result` arrives with enums",
                    self.describe(&other)
                );
                self.error(t.syntax(), msg);
                return Ty::Unknown;
            }
        };
        let ret = self.ret_ty.clone();
        if !matches!(self.shallow(&ret), Ty::Option(_) | Ty::Unknown) {
            let msg = format!(
                "the `?` operator can only be used in a function that returns `Option` (this function returns `{}`)",
                self.describe(&ret)
            );
            self.error(t.syntax(), msg);
        }
        value
    }

    fn some_call(&mut self, c: &ast::CallExpr, args: &[ast::Expr]) -> Ty {
        self.arg_count(c.syntax(), "enum variant", 1, args.len());
        let t = args
            .first()
            .map(|a| self.infer(a, None))
            .unwrap_or(Ty::Unknown);
        for a in args.iter().skip(1) {
            self.infer(a, None);
        }
        Ty::Option(Box::new(t))
    }

    fn method(&mut self, m: &ast::MethodCallExpr, expected: Option<&Ty>) -> Ty {
        let (Some(recv), Some(name)) = (m.receiver(), m.name_ref()) else {
            return Ty::Unknown;
        };
        let method = name.text().to_string();
        let args: Vec<ast::Expr> = m.arg_list().map(|a| a.args().collect()).unwrap_or_default();
        let rt = self.infer(&recv, None);
        let resolved = self.shallow(&rt);

        // Infer remaining arguments without constraints, for paths that don't type them.
        let loose = |c: &mut Self| {
            for a in &args {
                c.infer(a, None);
            }
        };
        let no_method = |c: &mut Self, ty: &Ty| {
            let msg = format!(
                "no method named `{method}` found for `{}` (the std surface is still small; see docs/FEATURES.md)",
                c.describe(ty)
            );
            c.error(m.syntax(), msg);
            Ty::Unknown
        };

        if matches!(method.as_str(), "cmp" | "partial_cmp")
            && matches!(resolved, Ty::Var(_))
            && let Some(t) = self.method_ext(m, &recv, &resolved, &method, &args, expected)
        {
            return t;
        }
        if let Some(kind @ (VarKind::Int | VarKind::Float)) = self.var_kind(&rt) {
            let lit = if kind == VarKind::Int {
                "{integer}"
            } else {
                "{float}"
            };
            self.error(
                m.syntax(),
                format!("can't call method `{method}` on ambiguous numeric type `{lit}` (add a suffix like `2i32`)"),
            );
            loose(self);
            return Ty::Unknown;
        }
        if method == "clone" || method == "to_owned" {
            self.arg_count(m.syntax(), "method", 0, args.len());
            return rt;
        }

        // `sig(params, ret)` checks the arguments against `params` and returns `ret`.
        let sig = |c: &mut Self, params: &[Ty], ret: Ty| {
            c.arg_count(m.syntax(), "method", params.len(), args.len());
            for (a, p) in args.iter().zip(params) {
                c.check(a, p);
            }
            for a in args.iter().skip(params.len()) {
                c.infer(a, None);
            }
            ret
        };

        let receiver_ty = match resolved {
            Ty::Unknown | Ty::Var(_) if is_float_method(&method) => Ty::Float { f32: false },
            Ty::Extern(name) => return self.extern_method(m, &name, &method, &args),
            ref other if !matches!(other, Ty::Unknown | Ty::Var(_)) => {
                if let Some(t) = self.method_ext(m, &recv, other, &method, &args, expected) {
                    return t;
                }
                other.clone()
            }
            Ty::Unknown | Ty::Var(_) => {
                loose(self);
                return Ty::Unknown;
            }
            other => other,
        };
        let t = receiver_ty.clone();
        match (&receiver_ty, method.as_str()) {
            (Ty::Float { .. }, m2) if is_float_method(m2) => match m2 {
                "powi" => sig(self, &[Ty::Int(IntKind::I32)], t),
                "powf" | "min" | "max" | "atan2" => sig(self, &[t.clone()], t),
                "clamp" => sig(self, &[t.clone(), t.clone()], t),
                "is_nan" | "is_finite" => sig(self, &[], Ty::Bool),
                "to_string" => sig(self, &[], Ty::Str),
                _ => sig(self, &[], t),
            },
            (Ty::Int(k), m2) => match m2 {
                "abs" | "signum" if k.is_signed() => sig(self, &[], t),
                "pow" => sig(self, &[Ty::Int(IntKind::U32)], t),
                "min" | "max" | "rem_euclid" => sig(self, &[t.clone()], t),
                "clamp" => sig(self, &[t.clone(), t.clone()], t),
                "to_string" => sig(self, &[], Ty::Str),
                _ => no_method(self, &t),
            },
            (Ty::Str, m2) => match m2 {
                "len" => sig(self, &[], USIZE),
                "is_empty" => sig(self, &[], Ty::Bool),
                "to_string" | "as_str" | "to_uppercase" | "to_lowercase" | "trim"
                | "trim_start" | "trim_end" => sig(self, &[], Ty::Str),
                "contains" | "starts_with" | "ends_with" => {
                    self.arg_count(m.syntax(), "method", 1, args.len());
                    if let Some(a) = args.first() {
                        let at = self.infer(a, None);
                        if !matches!(self.shallow(&at), Ty::Str | Ty::Char | Ty::Unknown) {
                            let msg = format!(
                                "expected a `&str` or `char` pattern, found `{}`",
                                self.describe(&at)
                            );
                            self.error(a.syntax(), msg);
                        }
                    }
                    Ty::Bool
                }
                "replace" => sig(self, &[Ty::Str, Ty::Str], Ty::Str),
                "repeat" => sig(self, &[USIZE], Ty::Str),
                "push_str" => {
                    self.check_mutable(&recv, Mutation::Borrow);
                    sig(self, &[Ty::Str], Ty::Unit)
                }
                "push" => {
                    self.check_mutable(&recv, Mutation::Borrow);
                    sig(self, &[Ty::Char], Ty::Unit)
                }
                _ => no_method(self, &t),
            },
            (Ty::Bool | Ty::Char, "to_string") => sig(self, &[], Ty::Str),
            (Ty::Vec(elem), m2) => {
                let elem = (**elem).clone();
                let mutating = matches!(
                    m2,
                    "pop"
                        | "push"
                        | "insert"
                        | "remove"
                        | "swap"
                        | "reverse"
                        | "sort"
                        | "sort_unstable"
                        | "clear"
                        | "truncate"
                        | "extend_from_slice"
                        | "iter_mut"
                );
                if mutating {
                    self.check_mutable(&recv, Mutation::Borrow);
                }
                match m2 {
                    "len" => sig(self, &[], USIZE),
                    "is_empty" => sig(self, &[], Ty::Bool),
                    "push" => sig(self, &[elem], Ty::Unit),
                    "insert" => sig(self, &[USIZE, elem], Ty::Unit),
                    "remove" => sig(self, &[USIZE], elem),
                    "swap" => sig(self, &[USIZE, USIZE], Ty::Unit),
                    "truncate" => sig(self, &[USIZE], Ty::Unit),
                    "reverse" | "clear" => sig(self, &[], Ty::Unit),
                    "sort" | "sort_unstable" => {
                        if self.zonk(&elem).contains_float()
                            || self.var_kind(&elem) == Some(VarKind::Float)
                        {
                            self.error(
                                m.syntax(),
                                format!("the trait bound `{}: Ord` is not satisfied (floats can't be sorted with `sort`; `sort_by` is coming with closures)", self.describe(&elem)),
                            );
                        }
                        sig(self, &[], Ty::Unit)
                    }
                    "contains" => sig(self, &[elem], Ty::Bool),
                    "extend_from_slice" => sig(self, &[Ty::Vec(Box::new(elem))], Ty::Unit),
                    "join" | "concat" => {
                        if !self.unify(&elem, &Ty::Str) {
                            let msg = format!(
                                "`{m2}` needs a `Vec<String>`, found `Vec<{}>`",
                                self.describe(&elem)
                            );
                            self.error(m.syntax(), msg);
                        }
                        let params = if m2 == "join" { vec![Ty::Str] } else { vec![] };
                        sig(self, &params, Ty::Str)
                    }
                    "to_vec" => sig(self, &[], t),
                    "iter" | "into_iter" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(elem),
                            mutable: false,
                        },
                    ),
                    "iter_mut" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(elem),
                            mutable: true,
                        },
                    ),
                    "pop" | "first" | "last" => sig(self, &[], Ty::Option(Box::new(elem))),
                    "get" => sig(self, &[USIZE], Ty::Option(Box::new(elem))),
                    "get_mut" | "first_mut" | "last_mut" => {
                        self.error(
                            m.syntax(),
                            format!(
                                "`Vec::{m2}` is not supported yet (it hands out a reference into the Vec); index with `v[i]` instead"
                            ),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Option(inner), m2) => {
                let inner = (**inner).clone();
                match m2 {
                    "is_some" | "is_none" => sig(self, &[], Ty::Bool),
                    "unwrap" => sig(self, &[], inner),
                    "expect" => sig(self, &[Ty::Str], inner),
                    "unwrap_or" => sig(self, &[inner.clone()], inner),
                    "unwrap_or_default" => {
                        let resolved = self.zonk(&inner);
                        if !resolved.has_default() {
                            let msg = format!(
                                "the trait bound `{}: Default` is not satisfied",
                                self.describe(&inner)
                            );
                            self.error(m.syntax(), msg);
                        }
                        sig(self, &[], inner)
                    }
                    "take" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        sig(self, &[], t)
                    }
                    "replace" => {
                        self.check_mutable(&recv, Mutation::Borrow);
                        sig(self, &[inner], t)
                    }
                    "as_ref" | "as_mut" | "as_deref" | "cloned" | "copied" => sig(self, &[], t),
                    "map" | "and_then" | "filter" | "unwrap_or_else" | "map_or" | "map_or_else"
                    | "is_some_and" | "ok_or_else" | "or_else" => {
                        self.error(
                            m.syntax(),
                            format!(
                                "`Option::{m2}` takes a closure, and closures are not supported yet (next on the roadmap); use `match` or `if let`"
                            ),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Map(k, v), m2) => {
                let (k, v) = ((**k).clone(), (**v).clone());
                if matches!(
                    m2,
                    "insert" | "remove" | "clear" | "entry" | "get_mut" | "values_mut" | "iter_mut"
                ) {
                    self.check_mutable(&recv, Mutation::Borrow);
                }
                let opt = |t: Ty| Ty::Option(Box::new(t));
                let iter = |elem: Ty, mutable: bool| Ty::Iter {
                    elem: Box::new(elem),
                    mutable,
                };
                match m2 {
                    "insert" => sig(self, &[k, v.clone()], opt(v)),
                    "get" | "remove" => sig(self, &[k], opt(v)),
                    "contains_key" => sig(self, &[k], Ty::Bool),
                    "get_mut" => {
                        let resolved = self.zonk(&v);
                        if resolved.is_js_primitive() {
                            let msg = format!(
                                "`HashMap::get_mut` on a map of `{}` values isn't supported (the value would be copied, so changes wouldn't stick); use `*map.entry(k).or_insert(..) += ..` or `map.insert(k, v)`",
                                self.describe(&v)
                            );
                            self.error(m.syntax(), msg);
                        }
                        sig(self, &[k], opt(v))
                    }
                    "len" => sig(self, &[], USIZE),
                    "is_empty" => sig(self, &[], Ty::Bool),
                    "clear" => sig(self, &[], Ty::Unit),
                    "entry" => sig(self, &[k.clone()], Ty::Entry(Box::new(k), Box::new(v))),
                    "keys" | "into_keys" => sig(self, &[], iter(k, false)),
                    "values" | "into_values" => sig(self, &[], iter(v, false)),
                    "values_mut" => sig(self, &[], iter(v, true)),
                    "iter" | "into_iter" => sig(self, &[], iter(Ty::Tuple(vec![k, v]), false)),
                    "iter_mut" => sig(self, &[], iter(Ty::Tuple(vec![k, v]), true)),
                    "retain" | "extend" | "drain" | "and_modify" => {
                        self.error(
                            m.syntax(),
                            format!("`HashMap::{m2}` needs iterators or closures, which are not supported yet; loop and `insert` instead"),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Entry(_, v), m2) => {
                let v = (**v).clone();
                match m2 {
                    "or_insert" => sig(self, &[v.clone()], v),
                    "or_default" => {
                        let resolved = self.zonk(&v);
                        if !resolved.has_default() {
                            let msg = format!(
                                "the trait bound `{}: Default` is not satisfied",
                                self.describe(&v)
                            );
                            self.error(m.syntax(), msg);
                        }
                        sig(self, &[], v)
                    }
                    "or_insert_with" => {
                        // Closures don't exist yet; constructor paths like `Vec::new` work.
                        let made = match args.first() {
                            Some(ast::Expr::PathExpr(p)) => {
                                let text = p.syntax().text().to_string();
                                match std_tail(&text.split("::").collect::<Vec<_>>()) {
                                    [.., "Vec", "new"] => {
                                        Some(Ty::Vec(Box::new(self.fresh(VarKind::General))))
                                    }
                                    [.., "String", "new"] => Some(Ty::Str),
                                    [.., "HashSet", "new"] => {
                                        Some(Ty::Set(Box::new(self.fresh(VarKind::General))))
                                    }
                                    [.., "HashMap", "new"] => Some(Ty::Map(
                                        Box::new(self.fresh(VarKind::General)),
                                        Box::new(self.fresh(VarKind::General)),
                                    )),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };
                        match (made, args.first()) {
                            (Some(made), Some(a)) => {
                                self.record(a.syntax(), made.clone());
                                if !self.unify(&made, &v) {
                                    let msg = format!(
                                        "mismatched types: expected `{}`, found `{}`",
                                        self.describe(&v),
                                        self.describe(&made)
                                    );
                                    self.error(a.syntax(), msg);
                                }
                            }
                            _ => {
                                // A function value: `fn() -> V`.
                                if let Some(a) = args.first() {
                                    let want = Ty::Closure {
                                        params: vec![],
                                        ret: Box::new(v.clone()),
                                    };
                                    self.check(a, &want);
                                }
                            }
                        }
                        v
                    }
                    "and_modify" | "or_insert_with_key" => {
                        self.error(
                            m.syntax(),
                            format!(
                                "`Entry::{m2}` takes a closure, and closures are not supported yet"
                            ),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Set(elem), m2) => {
                let elem = (**elem).clone();
                if matches!(m2, "insert" | "remove" | "clear") {
                    self.check_mutable(&recv, Mutation::Borrow);
                }
                match m2 {
                    "insert" | "contains" | "remove" => sig(self, &[elem], Ty::Bool),
                    "len" => sig(self, &[], USIZE),
                    "is_empty" => sig(self, &[], Ty::Bool),
                    "clear" => sig(self, &[], Ty::Unit),
                    "is_subset" | "is_superset" | "is_disjoint" => {
                        sig(self, &[t.clone()], Ty::Bool)
                    }
                    "iter" | "into_iter" => sig(
                        self,
                        &[],
                        Ty::Iter {
                            elem: Box::new(elem),
                            mutable: false,
                        },
                    ),
                    "union"
                    | "intersection"
                    | "difference"
                    | "symmetric_difference"
                    | "retain"
                    | "extend" => {
                        self.error(
                            m.syntax(),
                            format!("`HashSet::{m2}` needs iterators or closures, which are not supported yet; loop and `insert` instead"),
                        );
                        loose(self);
                        Ty::Unknown
                    }
                    _ => no_method(self, &t),
                }
            }
            (Ty::Iter { elem, mutable }, "enumerate") => {
                let item = Ty::Tuple(vec![USIZE, (**elem).clone()]);
                sig(
                    self,
                    &[],
                    Ty::Iter {
                        elem: Box::new(item),
                        mutable: *mutable,
                    },
                )
            }
            (Ty::Iter { .. }, _) => {
                self.error(
                    m.syntax(),
                    format!("iterator adapters like `.{method}()` are not supported yet (planned for phase 1); use a `for` loop"),
                );
                loose(self);
                Ty::Unknown
            }
            _ => no_method(self, &t),
        }
    }

    fn index(&mut self, ix: &ast::IndexExpr) -> Ty {
        let (Some(base), Some(index)) = (ix.base(), ix.index()) else {
            return Ty::Unknown;
        };
        let bt = self.infer(&base, None);
        if let Ty::Map(k, v) = self.shallow(&bt) {
            self.check(&index, &k);
            return *v;
        }
        if let ast::Expr::RangeExpr(_) = index {
            self.infer(&index, None);
            return Ty::Unknown; // slicing: codegen reports
        }
        let it = self.infer(&index, Some(&USIZE));
        let resolved = self.shallow(&bt);
        if !self.unify(&it, &USIZE) {
            let container = match &resolved {
                Ty::Vec(e) => format!("[{}]", self.describe(e)),
                other => self.describe(other),
            };
            let msg = format!(
                "the type `{container}` cannot be indexed by `{}` (indices must be `usize`)",
                self.describe(&it)
            );
            self.error(index.syntax(), msg);
        }
        match resolved {
            Ty::Vec(e) => *e,
            Ty::Unknown | Ty::Var(_) => Ty::Unknown,
            Ty::Str => {
                self.error(
                    ix.syntax(),
                    "strings cannot be indexed by integers (Rust strings are UTF-8)",
                );
                Ty::Unknown
            }
            other => {
                let msg = format!(
                    "cannot index into a value of type `{}`",
                    self.describe(&other)
                );
                self.error(ix.syntax(), msg);
                Ty::Unknown
            }
        }
    }

    fn if_expr(&mut self, i: &ast::IfExpr, expected: Option<&Ty>) -> Ty {
        // An `if let`'s bindings are visible in the `then` branch only.
        self.scopes.push(HashMap::new());
        match i.condition() {
            Some(ast::Expr::LetExpr(le)) => self.let_condition(&le),
            Some(c) => {
                self.check(&c, &Ty::Bool);
            }
            None => {}
        }
        let then_ty = match i.then_branch() {
            Some(b) => self.block(&b, expected),
            None => Ty::Unit,
        };
        self.scopes.pop();
        let (else_ty, else_node) = match i.else_branch() {
            Some(ElseBranch::Block(b)) => (self.block(&b, Some(&then_ty)), b.syntax().clone()),
            Some(ElseBranch::IfExpr(ei)) => {
                let t = self.infer(&ast::Expr::IfExpr(ei.clone()), Some(&then_ty));
                (t, ei.syntax().clone())
            }
            None => return Ty::Unit,
        };
        if !self.unify(&then_ty, &else_ty) {
            let msg = format!(
                "`if` and `else` have incompatible types: expected `{}`, found `{}`",
                self.describe(&then_ty),
                self.describe(&else_ty)
            );
            self.error(&else_node, msg);
        }
        if self.shallow(&then_ty) == Ty::Never {
            else_ty
        } else {
            then_ty
        }
    }

    fn for_expr(&mut self, f: &ast::ForExpr) {
        let Some(iterable) = f.iterable() else { return };
        // (element type, reference kind of the loop variable)
        let (elem, reference) = match &iterable {
            ast::Expr::RangeExpr(r) => (self.range_elem(r), None),
            ast::Expr::MethodCallExpr(m)
                if m.name_ref().is_some_and(|n| n.text() == "rev")
                    && matches!(
                        m.receiver().map(|r| strip_parens(&r)),
                        Some(ast::Expr::RangeExpr(_))
                    ) =>
            {
                let Some(ast::Expr::RangeExpr(r)) = m.receiver().map(|r| strip_parens(&r)) else {
                    unreachable!()
                };
                let t = self.range_elem(&r);
                self.record(m.syntax(), Ty::Unknown);
                (t, None)
            }
            other => {
                let t = self.infer(other, None);
                let by_ref = match other {
                    ast::Expr::RefExpr(r) => Some(r.mut_token().is_some()),
                    _ => None,
                };
                match self.shallow(&t) {
                    Ty::Vec(e) | Ty::Set(e) => (*e, by_ref),
                    Ty::Map(k, v) => (Ty::Tuple(vec![*k, *v]), by_ref),
                    Ty::Iter { elem, mutable } => (*elem, Some(mutable)),
                    Ty::Unknown | Ty::Var(_) => (Ty::Unknown, None),
                    other_ty => {
                        let msg = format!("`{}` is not an iterator", self.describe(&other_ty));
                        self.error(iterable.syntax(), msg);
                        (Ty::Unknown, None)
                    }
                }
            }
        };
        self.scopes.push(HashMap::new());
        if let Some(pat) = f.pat() {
            if !irrefutable(&pat) {
                self.error(pat.syntax(), "refutable pattern in `for` loop binding");
            }
            self.pat(&pat, &elem);
            // Loop variables over `&v` / `&mut v` are references.
            if let Some(scope) = self.scopes.last_mut() {
                for local in scope.values_mut() {
                    local.reference = reference;
                }
            }
        }
        if let Some(body) = f.loop_body() {
            self.block(&body, None);
        }
        self.scopes.pop();
    }

    fn range_elem(&mut self, r: &ast::RangeExpr) -> Ty {
        let lo = r.start().map(|e| self.infer(&e, None));
        let hi = match (r.end(), &lo) {
            (Some(e), Some(l)) => {
                let l = l.clone();
                Some(self.check(&e, &l))
            }
            (Some(e), None) => Some(self.infer(&e, None)),
            (None, _) => None,
        };
        self.record(r.syntax(), Ty::Unknown);
        let t = lo.or(hi).unwrap_or(Ty::Unknown);
        if !self.is_integer(&t) && self.shallow(&t) != Ty::Char {
            let msg = format!(
                "`{}` ranges can't be iterated (only integer ranges)",
                self.describe(&t)
            );
            self.error(r.syntax(), msg);
        }
        t
    }

    // ------------------------------------------------------------ macros

    fn macro_arg(&mut self, arg: &MacroArg, expected: Option<&Ty>) -> Ty {
        let (expr, errors) = macros::parse_arg(arg);
        for (start, end, message) in errors {
            self.diags.push(RawDiag {
                start,
                end,
                message,
                severity: "error",
            });
        }
        let Some(expr) = expr else { return Ty::Unknown };
        let saved = std::mem::replace(&mut self.base, arg.start);
        let t = match expected {
            Some(e) => self.check(&expr, e),
            None => self.infer(&expr, None),
        };
        self.base = saved;
        t
    }

    fn macro_call(&mut self, mc: &ast::MacroCall, expected: Option<&Ty>) -> Ty {
        let Some(name) = macros::macro_name(mc) else {
            return Ty::Unknown;
        };
        if name == "vec" {
            let elem = match expected.map(|t| self.shallow(t)) {
                Some(Ty::Vec(e)) => *e,
                _ => self.fresh(VarKind::General),
            };
            if let Some((value, count)) = macros::split_repeat(self.src, self.base, mc) {
                self.macro_arg(&value, Some(&elem));
                self.macro_arg(&count, Some(&USIZE));
            } else {
                for a in macros::split_args(self.src, self.base, mc) {
                    self.macro_arg(&a, Some(&elem));
                }
            }
            return Ty::Vec(Box::new(elem));
        }
        let args = macros::split_args(self.src, self.base, mc);
        match name.as_str() {
            "println" | "print" | "eprintln" | "eprint" | "format" | "panic" | "unreachable"
            | "todo" | "unimplemented" => {
                if !args.is_empty() {
                    self.format_args(&args);
                }
                match name.as_str() {
                    "format" => Ty::Str,
                    "println" | "print" | "eprintln" | "eprint" => Ty::Unit,
                    _ => Ty::Never,
                }
            }
            "assert" => {
                if let Some(cond) = args.first() {
                    self.macro_arg(cond, Some(&Ty::Bool));
                }
                if args.len() > 1 {
                    self.format_args(&args[1..]);
                }
                Ty::Unit
            }
            "assert_eq" | "assert_ne" => {
                if args.len() >= 2 {
                    let l = self.macro_arg(&args[0], None);
                    let r = self.macro_arg(&args[1], Some(&l));
                    let _ = r;
                }
                if args.len() > 2 {
                    self.format_args(&args[2..]);
                }
                Ty::Unit
            }
            _ => Ty::Unknown,
        }
    }

    /// Type the arguments of a format macro and check each placeholder can format its value.
    fn format_args(&mut self, args: &[MacroArg]) {
        let tys: Vec<Ty> = args[1..].iter().map(|a| self.macro_arg(a, None)).collect();
        let Some(fmt_str) = macros::format_string(&args[0]) else {
            return;
        };
        let Ok(pieces) = fmt::parse(&fmt_str) else {
            return;
        };
        let mut next = 0;
        for p in pieces {
            let Piece::Arg { arg, spec } = p else {
                continue;
            };
            let ty = match arg {
                ArgRef::Next => {
                    next += 1;
                    tys.get(next - 1).cloned()
                }
                ArgRef::Index(i) => tys.get(i).cloned(),
                ArgRef::Name(n) => match args[1..]
                    .iter()
                    .position(|a| a.name.as_deref() == Some(n.as_str()))
                {
                    Some(i) => tys.get(i).cloned(),
                    // `{name}` with no `name = ...` argument captures a local.
                    None => match self.lookup(&n) {
                        Some(l) => Some(l.ty.clone()),
                        None => {
                            let (start, end) = (args[0].start, args[0].end());
                            let message = format!("cannot find value `{n}` in this scope");
                            self.diags.push(RawDiag {
                                start,
                                end,
                                message,
                                severity: "error",
                            });
                            None
                        }
                    },
                },
            };
            let Some(ty) = ty else { continue };
            let resolved = self.shallow(&ty);
            let bad = match &resolved {
                Ty::Vec(_) | Ty::Option(_) | Ty::Unit | Ty::Tuple(_) | Ty::Set(_) | Ty::Map(..) => {
                    !spec.debug
                }
                Ty::Entry(..) | Ty::Closure { .. } => true,
                Ty::Ordering => !spec.debug,
                Ty::Iter { .. } | Ty::Future(_) | Ty::Fn => true,
                Ty::Extern(_) => !spec.debug,
                _ => false,
            };
            if bad {
                let trait_name = if spec.debug { "Debug" } else { "Display" };
                let hint = if matches!(resolved, Ty::Vec(_)) {
                    " (use `{:?}`)"
                } else {
                    ""
                };
                let message = format!(
                    "`{}` doesn't implement `std::fmt::{trait_name}`{hint}",
                    self.describe(&ty)
                );
                let (start, end) = (args[0].start, args[0].end());
                self.diags.push(RawDiag {
                    start,
                    end,
                    message,
                    severity: "error",
                });
            }
        }
    }
}

/// An unsuffixed integer literal, possibly negated: `100`, `-5`.
fn is_int_literal(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::Literal(l) => {
            matches!(l.kind(), LiteralKind::IntNumber(n) if n.suffix().is_none())
        }
        ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Neg) => {
            p.expr().is_some_and(|i| is_int_literal(&i))
        }
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| is_int_literal(&i)),
        _ => false,
    }
}

/// `Ordering::Less` → -1, `Equal` → 0, `Greater` → 1.
pub fn ordering_variant(path: Option<ast::Path>) -> Option<i8> {
    let text = path?.syntax().text().to_string();
    let last2: Vec<&str> = text.rsplit("::").take(2).collect();
    match last2.as_slice() {
        ["Less", "Ordering"] => Some(-1),
        ["Equal", "Ordering"] => Some(0),
        ["Greater", "Ordering"] => Some(1),
        _ => None,
    }
}

/// A pattern reduced to the constructors exhaustiveness checking understands.
#[derive(Clone, Debug)]
enum Ap {
    Wild,
    Bool(bool),
    Ord(i8),
    None,
    Some(Box<Ap>),
    Tuple(Vec<Ap>),
    /// A refutable pattern over an effectively infinite domain (an integer, char or string).
    Other,
}

enum Family {
    Bool,
    Ordering,
    Option(Ty),
    Tuple(Vec<Ty>),
    Infinite,
}

/// `None` in a pattern parses as an identifier binding; it's really the variant.
fn is_none_pat(ip: &ast::IdentPat) -> bool {
    ip.pat().is_none()
        && ip.mut_token().is_none()
        && ip.ref_token().is_none()
        && ip.name().is_some_and(|n| n.text() == "None")
}

/// Whether a path is `name` or `Option::name`.
pub fn path_is(path: Option<ast::Path>, name: &str) -> bool {
    let Some(path) = path else { return false };
    let segs: Vec<String> = path
        .segments()
        .filter_map(|s| s.name_ref())
        .map(|n| n.text().to_string())
        .collect();
    match segs.as_slice() {
        [n] => n == name,
        [.., o, n] => o == "Option" && n == name,
        _ => false,
    }
}

pub fn strip_pat(p: &ast::Pat) -> ast::Pat {
    match p {
        ast::Pat::ParenPat(pp) => pp.pat().map(|i| strip_pat(&i)).unwrap_or_else(|| p.clone()),
        ast::Pat::RefPat(rp) => rp.pat().map(|i| strip_pat(&i)).unwrap_or_else(|| p.clone()),
        _ => p.clone(),
    }
}

/// Patterns that match every value: `_`, `x`, `x @ <irrefutable>`.
pub fn irrefutable(p: &ast::Pat) -> bool {
    match strip_pat(p) {
        ast::Pat::WildcardPat(_) | ast::Pat::RestPat(_) => true,
        ast::Pat::IdentPat(ip) => !is_none_pat(&ip) && ip.pat().is_none_or(|sub| irrefutable(&sub)),
        ast::Pat::TuplePat(tp) => tp.fields().all(|f| irrefutable(&f)),
        _ => false,
    }
}

pub fn is_none_binding(ip: &ast::IdentPat) -> bool {
    is_none_pat(ip)
}

fn strip_parens(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::ParenExpr(p) => p
            .expr()
            .map(|i| strip_parens(&i))
            .unwrap_or_else(|| e.clone()),
        _ => e.clone(),
    }
}
