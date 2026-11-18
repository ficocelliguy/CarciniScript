//! Compile-time evaluation of constant integer and `bool` expressions: const generic arguments
//! (`{ 2 * D_CODES + 1 }`, `{ CpuFeatures::NONE }`, `{ size_of::<u32>() }`, `{ elts(BITS) }`)
//! and enum discriminants. Literals, `const` items (the file's own, and other files' through
//! `Known`), associated consts of the file's impls, `u8::MAX` / `u32::BITS`, arithmetic,
//! comparisons, `as` casts, `if`, and calls of the file's `const fn`s with simple bodies.
//! `None` for anything else.
use ra_ap_syntax::ast::{
    self, ArithOp, BinaryOp, CmpOp, HasArgList, HasGenericParams, HasName, LogicOp, UnaryOp,
};
use ra_ap_syntax::{AstNode, SyntaxKind};
use std::collections::HashMap;

/// A constant's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CVal {
    Int(i128),
    /// A `u128` above `i128::MAX` (`u128::MAX`): passed on and read by a few methods, not
    /// computed with.
    U128(u128),
    Bool(bool),
}

impl CVal {
    pub fn int(self) -> Option<i128> {
        match self {
            CVal::Int(n) => Some(n),
            CVal::Bool(b) => Some(i128::from(b)),
            CVal::U128(_) => None,
        }
    }

    /// As a const argument is written (`3`, `true`).
    pub fn text(self) -> String {
        match self {
            CVal::Int(n) => n.to_string(),
            CVal::U128(n) => n.to_string(),
            CVal::Bool(b) => b.to_string(),
        }
    }
}

/// An integer, as an `Int` when it fits an `i128`.
fn from_u128(n: u128) -> CVal {
    i128::try_from(n).map_or(CVal::U128(n), CVal::Int)
}

/// Values known from elsewhere: other files' `const` items (also as `module.NAME`) and
/// associated consts (by type, then name).
#[derive(Default)]
pub struct Known<'a> {
    pub consts: Option<&'a HashMap<String, CVal>>,
    pub assoc: Option<&'a HashMap<String, HashMap<String, CVal>>>,
}

/// The value of `e`, a constant expression in `file`.
pub fn eval(file: &ast::SourceFile, known: &Known, e: &ast::Expr) -> Option<CVal> {
    let mut ev = Eval {
        file,
        known,
        fuel: 10_000,
        depth: 0,
    };
    ev.expr(e, &Env::default())
}

/// The value of `ty`'s associated const `name` (`Grid::W`), from `file`'s impls.
pub fn eval_assoc(file: &ast::SourceFile, known: &Known, ty: &str, name: &str) -> Option<CVal> {
    let mut ev = Eval {
        file,
        known,
        fuel: 10_000,
        depth: 0,
    };
    ev.assoc(ty, name)
}

/// The value of the constant a path names (`LIMIT`, `Grid::W`, `Self::W` in `self_ty`'s impl,
/// `u8::MAX`), by its segments.
pub fn eval_segs(
    file: &ast::SourceFile,
    known: &Known,
    segs: &[String],
    self_ty: Option<&str>,
) -> Option<CVal> {
    let mut ev = Eval {
        file,
        known,
        fuel: 10_000,
        depth: 0,
    };
    let env = Env {
        self_ty: self_ty.map(str::to_string),
        ..Env::default()
    };
    ev.segs(segs, &env)
}

#[derive(Default, Clone)]
struct Env {
    vals: HashMap<String, CVal>,
    /// A `const fn`'s type parameters, by the types its call gave (`elts::<u8>(..)`).
    tys: HashMap<String, String>,
    /// The impl's type, for `Self::X`.
    self_ty: Option<String>,
}

enum Flow {
    Val(CVal),
    Ret(CVal),
}

struct Eval<'a, 'k> {
    file: &'a ast::SourceFile,
    known: &'a Known<'k>,
    fuel: u32,
    depth: u32,
}

/// An integer type's width and signedness (`isize` / `usize` are 64 bits here, as the
/// target's `usize`).
pub fn int_kind(name: &str) -> Option<(u32, bool)> {
    Some(match name {
        "i8" => (8, true),
        "i16" => (16, true),
        "i32" => (32, true),
        "i64" | "isize" => (64, true),
        "i128" => (128, true),
        "u8" => (8, false),
        "u16" => (16, false),
        "u32" => (32, false),
        "u64" | "usize" => (64, false),
        "u128" => (128, false),
        _ => return None,
    })
}

/// `v` wrapped to an integer type (`as u8`); None when it doesn't fit an `i128` (`u128` above
/// `i128::MAX`).
fn wrap(v: i128, bits: u32, signed: bool) -> Option<i128> {
    if bits == 128 {
        return (signed || v >= 0).then_some(v);
    }
    let m = v.rem_euclid(1i128 << bits);
    Some(if signed && m >= 1i128 << (bits - 1) {
        m - (1i128 << bits)
    } else {
        m
    })
}

/// The file's items, also those in inline `mod`s and function bodies (not in impls).
fn items(file: &ast::SourceFile) -> Vec<ast::Item> {
    file.syntax()
        .descendants()
        .filter(|n| {
            n.parent().is_some_and(|p| match p.kind() {
                SyntaxKind::SOURCE_FILE | SyntaxKind::STMT_LIST => true,
                SyntaxKind::ITEM_LIST => p.parent().is_some_and(|m| m.kind() == SyntaxKind::MODULE),
                _ => false,
            })
        })
        .filter_map(ast::Item::cast)
        .collect()
}

/// A type's name when it's a plain path (`u8`, `Grid`, `crate::m::Grid` → `Grid`).
fn type_name(t: &ast::Type) -> Option<String> {
    match t {
        ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
        ast::Type::ParenType(p) => type_name(&p.ty()?),
        _ => None,
    }
}

impl Eval<'_, '_> {
    fn expr(&mut self, e: &ast::Expr, env: &Env) -> Option<CVal> {
        match self.flow(e, env)? {
            Flow::Val(v) | Flow::Ret(v) => Some(v),
        }
    }

    fn int(&mut self, e: &ast::Expr, env: &Env) -> Option<i128> {
        match self.flow(e, env)? {
            Flow::Val(CVal::Int(n)) => Some(n),
            _ => None,
        }
    }

    fn flow(&mut self, e: &ast::Expr, env: &Env) -> Option<Flow> {
        self.fuel = self.fuel.checked_sub(1)?;
        Some(Flow::Val(match e {
            ast::Expr::Literal(l) => match l.kind() {
                ast::LiteralKind::IntNumber(n) => from_u128(n.value().ok()?),
                ast::LiteralKind::Byte(b) => CVal::Int(i128::from(b.value().ok()?)),
                ast::LiteralKind::Char(c) => CVal::Int(i128::from(u32::from(c.value().ok()?))),
                ast::LiteralKind::Bool(b) => CVal::Bool(b),
                _ => return None,
            },
            ast::Expr::ParenExpr(p) => return self.flow(&p.expr()?, env),
            ast::Expr::BlockExpr(b) => return self.block(b, env),
            ast::Expr::ReturnExpr(r) => return Some(Flow::Ret(self.expr(&r.expr()?, env)?)),
            ast::Expr::PrefixExpr(p) => match (p.op_kind()?, self.expr(&p.expr()?, env)?) {
                (UnaryOp::Neg, CVal::Int(v)) => CVal::Int(v.checked_neg()?),
                (UnaryOp::Not, CVal::Int(v)) => CVal::Int(!v),
                (UnaryOp::Not, CVal::Bool(b)) => CVal::Bool(!b),
                _ => return None,
            },
            ast::Expr::BinExpr(b) => self.bin(b, env)?,
            ast::Expr::CastExpr(c) => {
                let v = self.expr(&c.expr()?, env)?;
                let name = type_name(&c.ty()?)?;
                let (bits, signed) = int_kind(&name)?;
                match v {
                    CVal::U128(n) if bits == 128 && !signed => CVal::U128(n),
                    CVal::U128(n) => {
                        CVal::Int(wrap((n & (u128::MAX >> 1)).cast_signed(), bits, signed)?)
                    }
                    v => CVal::Int(wrap(v.int()?, bits, signed)?),
                }
            }
            ast::Expr::IfExpr(i) => {
                let CVal::Bool(c) = self.expr(&i.condition()?, env)? else {
                    return None;
                };
                return match (c, i.then_branch(), i.else_branch()) {
                    (true, Some(t), _) => self.block(&t, env),
                    (false, _, Some(ast::ElseBranch::Block(b))) => self.block(&b, env),
                    (false, _, Some(ast::ElseBranch::IfExpr(e))) => {
                        self.flow(&ast::Expr::IfExpr(e), env)
                    }
                    // (No `else`: a unit value nothing reads.)
                    (false, _, None) => Some(Flow::Val(CVal::Bool(false))),
                    _ => None,
                };
            }
            ast::Expr::PathExpr(p) => self.path(&p.path()?, env)?,
            ast::Expr::CallExpr(c) => self.call(c, env)?,
            ast::Expr::MethodCallExpr(m) => self.method(m, env)?,
            _ => return None,
        }))
    }

    fn block(&mut self, b: &ast::BlockExpr, env: &Env) -> Option<Flow> {
        if b.async_token().is_some() || b.unsafe_token().is_some() || b.label().is_some() {
            return None;
        }
        let list = b.stmt_list()?;
        let mut env = env.clone();
        for s in list.statements() {
            match s {
                ast::Stmt::LetStmt(l) => {
                    let ast::Pat::IdentPat(ip) = l.pat()? else {
                        return None;
                    };
                    let name = ip.name()?.text().to_string();
                    match self.flow(&l.initializer()?, &env)? {
                        Flow::Val(v) => {
                            env.vals.insert(name, v);
                        }
                        r @ Flow::Ret(_) => return Some(r),
                    }
                }
                ast::Stmt::ExprStmt(es) => match self.flow(&es.expr()?, &env)? {
                    Flow::Ret(v) => return Some(Flow::Ret(v)),
                    // (`if c { return x; }` without an `else`: nothing when `c` is false.)
                    Flow::Val(_) => {}
                },
                ast::Stmt::Item(_) => return None,
            }
        }
        match list.tail_expr() {
            Some(t) => self.flow(&t, &env),
            // `if width == 0 { return 0; }`: a unit value nothing reads.
            None => Some(Flow::Val(CVal::Bool(false))),
        }
    }

    fn bin(&mut self, b: &ast::BinExpr, env: &Env) -> Option<CVal> {
        let op = b.op_kind()?;
        if let BinaryOp::LogicOp(lop) = op {
            let CVal::Bool(l) = self.expr(&b.lhs()?, env)? else {
                return None;
            };
            if matches!((lop, l), (LogicOp::And, false) | (LogicOp::Or, true)) {
                return Some(CVal::Bool(l));
            }
            let CVal::Bool(r) = self.expr(&b.rhs()?, env)? else {
                return None;
            };
            return Some(CVal::Bool(r));
        }
        let (l, r) = (self.expr(&b.lhs()?, env)?, self.expr(&b.rhs()?, env)?);
        if let BinaryOp::CmpOp(c) = op {
            let (l, r) = (l.int()?, r.int()?);
            return Some(CVal::Bool(match c {
                CmpOp::Eq { negated } => (l == r) != negated,
                CmpOp::Ord { ordering, strict } => match (ordering, strict) {
                    (ast::Ordering::Less, true) => l < r,
                    (ast::Ordering::Less, false) => l <= r,
                    (ast::Ordering::Greater, true) => l > r,
                    (ast::Ordering::Greater, false) => l >= r,
                },
            }));
        }
        if let (CVal::Bool(l), CVal::Bool(r)) = (l, r) {
            return Some(CVal::Bool(match op {
                BinaryOp::ArithOp(ArithOp::BitAnd) => l & r,
                BinaryOp::ArithOp(ArithOp::BitOr) => l | r,
                BinaryOp::ArithOp(ArithOp::BitXor) => l ^ r,
                _ => return None,
            }));
        }
        let (CVal::Int(l), CVal::Int(r)) = (l, r) else {
            return None;
        };
        Some(CVal::Int(match op {
            BinaryOp::ArithOp(ArithOp::Add) => l.checked_add(r)?,
            BinaryOp::ArithOp(ArithOp::Sub) => l.checked_sub(r)?,
            BinaryOp::ArithOp(ArithOp::Mul) => l.checked_mul(r)?,
            BinaryOp::ArithOp(ArithOp::Div) => l.checked_div(r)?,
            BinaryOp::ArithOp(ArithOp::Rem) => l.checked_rem(r)?,
            BinaryOp::ArithOp(ArithOp::Shl) => l.checked_shl(u32::try_from(r).ok()?)?,
            BinaryOp::ArithOp(ArithOp::Shr) => l.checked_shr(u32::try_from(r).ok()?)?,
            BinaryOp::ArithOp(ArithOp::BitAnd) => l & r,
            BinaryOp::ArithOp(ArithOp::BitOr) => l | r,
            BinaryOp::ArithOp(ArithOp::BitXor) => l ^ r,
            _ => return None,
        }))
    }

    /// The path's segments without `crate::` / `self::` / `super::` and module names
    /// (`m::Grid::W` → `Grid::W`); a module's item is `module.NAME` (`crate::m::ID` → `m.ID`).
    fn segments(path: &ast::Path) -> Option<Vec<String>> {
        let segs: Vec<String> = path
            .segments()
            .map(|s| {
                s.name_ref()
                    .map(|n| n.text().to_string())
                    .or_else(|| s.kind().map(|_| s.syntax().text().to_string()))
            })
            .collect::<Option<_>>()?;
        let skip = segs
            .iter()
            .take(segs.len().saturating_sub(1))
            .take_while(|s| {
                matches!(s.as_str(), "crate" | "self" | "super")
                    || s.starts_with(|c: char| c.is_ascii_lowercase())
                        && int_kind(s).is_none()
                        && !matches!(s.as_str(), "char" | "bool" | "f32" | "f64" | "str")
            })
            .count();
        let module = segs[..skip]
            .iter()
            .rev()
            .find(|s| !matches!(s.as_str(), "crate" | "self" | "super"))
            .cloned();
        let rest = segs[skip..].to_vec();
        Some(match (module, rest.as_slice()) {
            (Some(m), [one]) => vec![format!("{m}.{one}")],
            _ => rest,
        })
    }

    fn path(&mut self, path: &ast::Path, env: &Env) -> Option<CVal> {
        let segs = Self::segments(path)?;
        self.segs(&segs, env)
    }

    fn segs(&mut self, segs: &[String], env: &Env) -> Option<CVal> {
        match segs {
            [name] => {
                if let Some(v) = env.vals.get(name) {
                    return Some(*v);
                }
                // (`util::SIZE`: that file's; a bare name: this file's own first.)
                let bare = name.rsplit('.').next().unwrap_or(name);
                if name.contains('.')
                    && let Some(v) = self.known.consts.and_then(|k| k.get(name))
                {
                    return Some(*v);
                }
                let body = items(self.file).into_iter().find_map(|i| match i {
                    ast::Item::Const(k) if k.name().is_some_and(|n| n.text() == bare) => k.body(),
                    ast::Item::Static(s)
                        if s.name().is_some_and(|n| n.text() == bare)
                            && s.mut_token().is_none() =>
                    {
                        s.body()
                    }
                    _ => None,
                });
                match body {
                    Some(b) => self.nested(&b, &Env::default()),
                    None => self.known.consts.and_then(|k| k.get(bare)).copied(),
                }
            }
            [ty, name] => {
                let ty = match ty.as_str() {
                    "Self" => env.self_ty.clone()?,
                    t => env.tys.get(t).cloned().unwrap_or_else(|| t.to_string()),
                };
                if let Some((bits, signed)) = int_kind(&ty) {
                    let v = match (name.as_str(), signed) {
                        ("MAX", true) => (1i128 << (bits - 1)) - 1,
                        ("MIN", true) => -(1i128 << (bits - 1)),
                        ("MAX", false) if bits == 128 => return Some(CVal::U128(u128::MAX)),
                        ("MAX", false) => (1i128 << bits) - 1,
                        ("MIN", false) => 0,
                        ("BITS", _) => i128::from(bits),
                        _ => return self.assoc(&ty, name),
                    };
                    return Some(CVal::Int(v));
                }
                match (ty.as_str(), name.as_str()) {
                    ("char", "MAX") => Some(CVal::Int(0x10FFFF)),
                    ("bool", _) => None,
                    _ => self
                        .assoc(&ty, name)
                        .or_else(|| self.discriminant(&ty, name)),
                }
            }
            _ => None,
        }
    }

    /// An associated const of `ty` in the file's impls (inherent or of a trait), else a known
    /// one.
    fn assoc(&mut self, ty: &str, name: &str) -> Option<CVal> {
        let found = items(self.file)
            .into_iter()
            .filter_map(|i| match i {
                ast::Item::Impl(imp) => Some(imp),
                _ => None,
            })
            .filter(|imp| imp.self_ty().and_then(|t| type_name(&t)).as_deref() == Some(ty))
            .find_map(|imp| {
                imp.assoc_item_list()?.assoc_items().find_map(|a| match a {
                    ast::AssocItem::Const(k) if k.name().is_some_and(|n| n.text() == name) => {
                        k.body()
                    }
                    _ => None,
                })
            });
        match found {
            Some(b) => {
                let env = Env {
                    self_ty: Some(ty.to_string()),
                    ..Env::default()
                };
                self.nested(&b, &env)
            }
            None => self
                .known
                .assoc
                .and_then(|a| a.get(ty))
                .and_then(|m| m.get(name))
                .copied(),
        }
    }

    /// `Enum::Variant` of a field-less enum: its discriminant (for `as` casts).
    fn discriminant(&mut self, en: &str, variant: &str) -> Option<CVal> {
        let e = items(self.file).into_iter().find_map(|i| match i {
            ast::Item::Enum(e) if e.name().is_some_and(|n| n.text() == en) => Some(e),
            _ => None,
        })?;
        let mut next = 0i128;
        for v in e.variant_list()?.variants() {
            if v.field_list().is_some() {
                return None;
            }
            if let Some(d) = v.const_arg().and_then(|c| c.expr()) {
                next = self.nested(&d, &Env::default())?.int()?;
            }
            if v.name().is_some_and(|n| n.text() == variant) {
                return Some(CVal::Int(next));
            }
            next = next.checked_add(1)?;
        }
        None
    }

    /// Evaluate another item's body (a `const`, a `const fn`'s), one level deeper.
    fn nested(&mut self, e: &ast::Expr, env: &Env) -> Option<CVal> {
        if self.depth >= 32 {
            return None;
        }
        self.depth += 1;
        let v = self.expr(e, env);
        self.depth -= 1;
        v
    }

    /// The types a path's last segment gives (`f::<u8>` → `["u8"]`).
    fn type_args(seg: &ast::PathSegment, env: &Env) -> Vec<String> {
        use ra_ap_syntax::ast::HasGenericArgs;
        seg.generic_arg_list()
            .into_iter()
            .flat_map(|l| l.generic_args())
            .filter_map(|g| match g {
                ast::GenericArg::TypeArg(t) => type_name(&t.ty()?),
                _ => None,
            })
            .map(|t| env.tys.get(&t).cloned().unwrap_or(t))
            .collect()
    }

    fn call(&mut self, c: &ast::CallExpr, env: &Env) -> Option<CVal> {
        let ast::Expr::PathExpr(callee) = c.expr()? else {
            return None;
        };
        let path = callee.path()?;
        let seg = path.segment()?;
        let name = seg.name_ref()?.text().to_string();
        let targs = Self::type_args(&seg, env);
        let args: Vec<ast::Expr> = c.arg_list()?.args().collect();
        // `size_of::<u32>()` / `mem::size_of` / `align_of`.
        if matches!(name.as_str(), "size_of" | "align_of") && args.is_empty() {
            let (bits, _) = int_kind(targs.first()?).or_else(|| match targs.first()?.as_str() {
                "bool" => Some((8, false)),
                "char" | "f32" => Some((32, false)),
                "f64" => Some((64, false)),
                _ => None,
            })?;
            return Some(CVal::Int(i128::from(bits / 8)));
        }
        if path.qualifier().is_some() {
            return None;
        }
        let f = items(self.file).into_iter().find_map(|i| match i {
            ast::Item::Fn(f)
                if f.const_token().is_some() && f.name().is_some_and(|n| n.text() == name) =>
            {
                Some(f)
            }
            _ => None,
        })?;
        let mut fenv = Env::default();
        let tparams: Vec<String> = f
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
            .filter_map(|p| match p {
                ast::GenericParam::TypeParam(t) => Some(t.name()?.text().to_string()),
                _ => None,
            })
            .collect();
        for (p, t) in tparams.iter().zip(targs) {
            fenv.tys.insert(p.clone(), t);
        }
        let params: Vec<ast::Param> = f.param_list()?.params().collect();
        if params.len() != args.len() || f.param_list()?.self_param().is_some() {
            return None;
        }
        for (p, a) in params.iter().zip(&args) {
            let ast::Pat::IdentPat(ip) = p.pat()? else {
                return None;
            };
            let v = self.expr(a, env)?;
            fenv.vals.insert(ip.name()?.text().to_string(), v);
        }
        let body = f.body()?;
        self.nested(&ast::Expr::BlockExpr(body), &fenv)
    }

    /// Integer methods: `pow`, `min`, `max`, `div_ceil`, `ilog2`, `ilog10`, `count_ones`,
    /// `is_power_of_two`, `next_power_of_two`, `abs`.
    fn method(&mut self, m: &ast::MethodCallExpr, env: &Env) -> Option<CVal> {
        let name = m.name_ref()?.text().to_string();
        let recv = match self.expr(&m.receiver()?, env)? {
            // (`u128::MAX.ilog10()`)
            CVal::U128(n) => {
                if m.arg_list()?.args().next().is_some() {
                    return None;
                }
                return Some(CVal::Int(i128::from(match name.as_str() {
                    "ilog2" => n.ilog2(),
                    "ilog10" => n.ilog10(),
                    "count_ones" => n.count_ones(),
                    _ => return None,
                })));
            }
            CVal::Int(n) => n,
            CVal::Bool(_) => return None,
        };
        let args: Vec<i128> = m
            .arg_list()?
            .args()
            .map(|a| self.int(&a, env))
            .collect::<Option<_>>()?;
        Some(match (name.as_str(), args.as_slice()) {
            ("pow", [e]) => CVal::Int(recv.checked_pow(u32::try_from(*e).ok()?)?),
            ("min", [o]) => CVal::Int(recv.min(*o)),
            ("max", [o]) => CVal::Int(recv.max(*o)),
            ("div_ceil", [d]) if *d > 0 && recv >= 0 => CVal::Int((recv + d - 1) / d),
            ("ilog2", []) if recv > 0 => CVal::Int(i128::from(recv.ilog2())),
            ("ilog10", []) if recv > 0 => CVal::Int(i128::from(recv.ilog10())),
            ("count_ones", []) if recv >= 0 => CVal::Int(i128::from(recv.count_ones())),
            ("is_power_of_two", []) => CVal::Bool(recv > 0 && recv & (recv - 1) == 0),
            ("next_power_of_two", []) if recv >= 0 => CVal::Int(
                u128::try_from(recv)
                    .ok()?
                    .checked_next_power_of_two()?
                    .try_into()
                    .ok()?,
            ),
            ("abs", []) => CVal::Int(recv.checked_abs()?),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_in(items: &str, e: &str) -> Option<CVal> {
        let src = format!("{items}\nfn __probe() {{ let _ = {e}; }}");
        let file = ast::SourceFile::parse(&src, ra_ap_syntax::Edition::Edition2024).tree();
        let expr = file
            .syntax()
            .descendants()
            .filter_map(ast::LetStmt::cast)
            .last()?
            .initializer()?;
        eval(&file, &Known::default(), &expr)
    }

    #[test]
    fn values() {
        let items = "const D: usize = 30; struct C; impl C { const NONE: usize = 0; const AVX: usize = 1 << 3; }
            const fn elts(bits: usize) -> usize { (bits + 63) / 64 }
            const fn bits_of<T>() -> usize { size_of::<T>() * 8 }
            const fn e2<T>(bits: usize) -> usize { let w = bits_of::<T>(); if w == 0 { return 0; } bits / w + (bits % w != 0) as usize }
            mod m { pub const ID: usize = 5; }
            trait L { const MAX_STR_LEN: usize; }
            impl L for u64 { const MAX_STR_LEN: usize = u64::MAX.ilog10() as usize + 1; }";
        let int = |e: &str| eval_in(items, e).and_then(CVal::int);
        assert_eq!(int("{ 2 * D + 1 }"), Some(61));
        assert_eq!(int("C::NONE"), Some(0));
        assert_eq!(int("{ C::AVX }"), Some(8));
        assert_eq!(int("elts(200)"), Some(4));
        assert_eq!(int("e2::<u16>(33)"), Some(3));
        assert_eq!(int("usize::BITS as usize / 16"), Some(4));
        assert_eq!(int("size_of::<u32>()"), Some(4));
        assert_eq!(int("std::mem::size_of::<u64>()"), Some(8));
        assert_eq!(int("crate::m::ID"), Some(5));
        assert_eq!(int("m::ID - 3"), Some(2));
        assert_eq!(int("u64::MAX / 4"), Some(4611686018427387903));
        assert_eq!(int("u64::MAX_STR_LEN - 16"), Some(4));
        assert_eq!(int("i32::MAX"), Some(i32::MAX.into()));
        assert_eq!(eval_in(items, "D > 3 && true"), Some(CVal::Bool(true)));
        assert_eq!(eval_in(items, "u128::MAX"), Some(CVal::U128(u128::MAX)));
        assert_eq!(int("u128::MAX.ilog10() as usize + 1"), Some(39));
        assert_eq!(int("u128::MAX as u8"), Some(255));
        assert_eq!(int("u128::MAX + 1"), None);
        assert_eq!(int("undefined_name + 1"), None);
    }
}
