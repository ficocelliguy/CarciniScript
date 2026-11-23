//! Spike code generator: walks the ra_ap_syntax AST directly and emits JS with source mappings.
//! Phase 1 inserts a HIR + type checker between parsing and this stage.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, AstToken, SyntaxKind, SyntaxNode,
    ast::{
        self, ArithOp, BinaryOp, CmpOp, ElseBranch, HasArgList, HasGenericParams, HasLoopBody,
        HasName, LiteralKind, LogicOp, RangeItem, RangeOp, UnaryOp,
    },
};
use serde::Serialize;

use crate::bindings::{self, bare};

#[path = "codegen_adt.rs"]
mod adt_gen;
#[path = "codegen_arrays.rs"]
mod arrays_gen;
#[path = "codegen_coll.rs"]
mod coll_gen;
#[path = "codegen_consts.rs"]
mod consts_gen;
#[path = "codegen_ext.rs"]
mod ext;
#[path = "codegen_gaps.rs"]
mod gaps_gen;
#[path = "codegen_iface.rs"]
mod iface_gen;
#[path = "codegen_json.rs"]
mod json_gen;
#[path = "codegen_jsval.rs"]
mod jsval_gen;
#[path = "codegen_quote.rs"]
mod quote_gen;
#[path = "codegen_res.rs"]
mod res_gen;
#[path = "codegen_std.rs"]
mod std_gen;
#[path = "codegen_views.rs"]
mod views_gen;
use crate::fmt::{self, ArgRef, Piece, Spec};
use crate::lineindex::{LineIndex, utf16_len};
use crate::macros::{self, MacroArg};
use crate::sourcemap::{self, Mapping};
use crate::ty::{IntKind, RangeKind, Ty, is_float_method, std_const, std_tail};
use crate::typeck::{self, Types, is_none_binding, path_is};

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub message: String,
    pub severity: &'static str,
    /// The rule behind the message in JavaScript or Python terms, and what to try, for a player
    /// who doesn't know Rust (`diag_help.rs`). Not every message has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// Byte offsets into the source.
    pub start: usize,
    pub end: usize,
    /// 1-based line, 1-based UTF-16 column (Monaco's convention).
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompileOutput {
    pub code: Option<String>,
    pub source_map: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    /// Files the output depends on besides its `mod` files (which it imports): the proc-macro
    /// crates it used. A host that caches compiled scripts recompiles when one changes.
    pub dependencies: Vec<String>,
}

/// A piece of JS expression text. `Some(offset)` parts record a source mapping from the current
/// output position back to that Rust byte offset.
#[derive(Default, Clone, Debug)]
struct Frag {
    parts: Vec<(Option<usize>, String)>,
}

impl Frag {
    fn text(s: impl Into<String>) -> Frag {
        Frag {
            parts: vec![(None, s.into())],
        }
    }
    fn mapped(src: usize, s: impl Into<String>) -> Frag {
        Frag {
            parts: vec![(Some(src), s.into())],
        }
    }
    fn push(&mut self, s: impl Into<String>) -> &mut Self {
        self.parts.push((None, s.into()));
        self
    }
    fn append(&mut self, other: Frag) -> &mut Self {
        self.parts.extend(other.parts);
        self
    }
    fn wrap(mut self, pre: &str, post: &str) -> Frag {
        self.parts.insert(0, (None, pre.to_string()));
        self.parts.push((None, post.to_string()));
        self
    }
    fn at(mut self, src: usize) -> Frag {
        self.parts.insert(0, (Some(src), String::new()));
        self
    }
    fn plain(&self) -> String {
        self.parts.iter().map(|(_, s)| s.as_str()).collect()
    }
    fn join(frags: Vec<Frag>, sep: &str) -> Frag {
        let mut out = Frag::default();
        for (i, f) in frags.into_iter().enumerate() {
            if i > 0 {
                out.push(sep);
            }
            out.append(f);
        }
        out
    }
}

struct Out {
    code: String,
    line: u32,
    col: u32,
    indent: usize,
    mappings: Vec<Mapping>,
    /// (byte position in `code`, source offset) for each mapping, so captured output can be
    /// turned back into a `Frag`.
    marks: Vec<(usize, usize)>,
}

impl Out {
    fn write(&mut self, frag: &Frag, idx: &LineIndex) {
        for (src, text) in &frag.parts {
            if let Some(src) = src {
                self.marks.push((self.code.len(), *src));
                let (src_line, src_col) = idx.line_col(*src);
                self.mappings.push(Mapping {
                    gen_line: self.line,
                    gen_col: self.col,
                    src_line,
                    src_col,
                });
            }
            for (i, seg) in text.split('\n').enumerate() {
                if i > 0 {
                    self.code.push('\n');
                    self.line += 1;
                    self.col = 0;
                }
                self.code.push_str(seg);
                self.col += utf16_len(seg);
            }
        }
    }

    fn line(&mut self, frag: &Frag, idx: &LineIndex) {
        let mut f = Frag::text("  ".repeat(self.indent));
        f.append(frag.clone());
        f.push("\n");
        self.write(&f, idx);
    }
}

#[derive(Clone)]
struct Local {
    js: String,
    ty: Ty,
}

/// A `const` / `static` item: Rust name, name node, initializer, item node.
type ConstItem = (String, ast::Name, Option<ast::Expr>, SyntaxNode);

struct FnSig {
    js: String,
    params: Vec<Ty>,
    ret: Ty,
    is_async: bool,
    /// No `&mut` argument can outlive the call: a local, non-async fn without lifetime
    /// parameters whose return type holds no reference, `impl`, `dyn` or type parameter. Its
    /// `&mut place` arguments are passed as write-back temps (`write_back_call`).
    writeback: bool,
}

/// Which part of each `[key, value]` slot a `for` loop over a map binds.
#[derive(Clone, Copy)]
enum MapPart {
    Entries,
    Keys,
    Values,
}

/// A pattern binding: the name, `mut`, and the JS for its value.
type Bind = (ast::Name, bool, String);

/// A lowered pattern: conditions that must all hold, and `(name, mutable, js)` bindings.
#[derive(Default)]
struct PatCode {
    conds: Vec<Frag>,
    binds: Vec<Bind>,
}

impl PatCode {
    /// The conditions joined with `&&`, or None when the pattern always matches.
    fn condition(&self) -> Option<Frag> {
        if self.conds.is_empty() {
            return None;
        }
        Some(Frag::join(self.conds.clone(), " && "))
    }
}

#[derive(Clone)]
enum Dest {
    Discard,
    Assign(String),
    Return,
}

/// Names the generated code relies on, plus JS reserved words. Rust identifiers that collide get
/// a `$` suffix (`$` can't appear in Rust identifiers, so this never collides itself).
const RESERVED: &[&str] = &[
    "arguments",
    "await",
    "case",
    "catch",
    "class",
    "const",
    "debugger",
    "default",
    "delete",
    "do",
    "enum",
    "eval",
    "export",
    "extends",
    "finally",
    "function",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "with",
    "yield",
    "undefined",
    "NaN",
    "Infinity",
    "globalThis",
    "console",
    "Math",
    "String",
    "Number",
    "Boolean",
    "Object",
    "Array",
    "BigInt",
    "Symbol",
    "κσ",
];

fn escape_ident(name: &str) -> String {
    let name = name.strip_prefix("r#").unwrap_or(name);
    if RESERVED.contains(&name) || bindings::is_ram_name(name) {
        format!("{name}$")
    } else {
        name.to_string()
    }
}

/// `.name`, or `["name"]` when the JS property name isn't a valid identifier.
fn member(js: &str) -> String {
    let ident = js
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && js
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if ident {
        format!(".{js}")
    } else {
        format!("[{}]", js_string(js))
    }
}

/// The JS name of a field or method of a bound JS interface. Without bindings for the type,
/// snake_case maps to camelCase.
fn extern_js_name(type_name: &str, rust_name: &str, method: bool) -> String {
    let b = bindings::current();
    let key = bare(rust_name);
    let found = b.types.get(type_name).and_then(|t| {
        if method {
            t.methods.get(&key).map(|m| m.js.clone())
        } else {
            t.fields.get(&key).map(|f| f.js.clone())
        }
    });
    found.unwrap_or_else(|| snake_to_camel(&key))
}

fn snake_to_camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for (i, c) in name.chars().enumerate() {
        if c == '_' && i > 0 {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn template_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
}

/// True if lowering `node` may need to write statements before the expression that uses it.
fn needs_hoist(node: &SyntaxNode) -> bool {
    use ra_ap_syntax::WalkEvent;
    let mut walk = node.preorder();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(n) = event else { continue };
        // A closure's body becomes its own JS function.
        if n.kind() == SyntaxKind::CLOSURE_EXPR && &n != node {
            walk.skip_subtree();
            continue;
        }
        if hoists(&n) {
            return true;
        }
    }
    false
}

/// Expressions that lower to statements.
fn hoists(n: &SyntaxNode) -> bool {
    {
        matches!(
            n.kind(),
            SyntaxKind::BLOCK_EXPR
                | SyntaxKind::IF_EXPR
                | SyntaxKind::LOOP_EXPR
                | SyntaxKind::WHILE_EXPR
                | SyntaxKind::FOR_EXPR
                | SyntaxKind::MATCH_EXPR
                | SyntaxKind::RETURN_EXPR
                | SyntaxKind::BREAK_EXPR
                | SyntaxKind::CONTINUE_EXPR
                | SyntaxKind::TRY_EXPR
        )
    }
}

/// A block that is just a tail expression with nothing to hoist, so it can become a JS expression.
fn simple_tail(block: &ast::BlockExpr) -> Option<ast::Expr> {
    let list = block.stmt_list()?;
    if list.statements().next().is_some()
        || block.label().is_some()
        || !matches!(block.modifier(), None | Some(ast::BlockModifier::Const(_)))
    {
        return None;
    }
    let tail = list.tail_expr()?;
    if needs_hoist(tail.syntax()) {
        None
    } else {
        Some(tail)
    }
}

fn if_is_ternary(e: &ast::IfExpr) -> bool {
    let cond_ok = e.condition().is_some_and(|c| {
        !needs_hoist(c.syntax())
            && !matches!(c, ast::Expr::LetExpr(_))
            && !crate::typeck::is_let_chain(&c)
    });
    let then_ok = e.then_branch().is_some_and(|b| simple_tail(&b).is_some());
    let else_ok = match e.else_branch() {
        Some(ElseBranch::Block(b)) => simple_tail(&b).is_some(),
        Some(ElseBranch::IfExpr(i)) => if_is_ternary(&i),
        None => false,
    };
    cond_ok && then_ok && else_ok
}

/// Operands that must be parenthesized when nested inside a JS binary expression. Rust's bitwise
/// operators bind tighter than comparisons; JS's don't, so every nested binary gets parens.
fn needs_parens(e: &ast::Expr) -> bool {
    matches!(
        e,
        ast::Expr::BinExpr(_)
            | ast::Expr::CastExpr(_)
            | ast::Expr::IfExpr(_)
            | ast::Expr::RangeExpr(_)
    )
}

/// Receivers of `.method()` / `.field` additionally need parens around prefix ops, `await` and
/// numeric literals (`5.toString()` is a JS syntax error).
fn receiver_needs_parens(e: &ast::Expr) -> bool {
    needs_parens(e)
        || matches!(
            e,
            ast::Expr::PrefixExpr(_) | ast::Expr::AwaitExpr(_) | ast::Expr::RefExpr(_)
        )
        || matches!(e, ast::Expr::Literal(l) if matches!(l.kind(), LiteralKind::IntNumber(_) | LiteralKind::FloatNumber(_)))
}

/// `e` wrapped to a BigInt kind's width: `BigInt.asUintN(64, e)` / `BigInt.asIntN(..)` inline
/// (about 9x faster than a call of `κσ.$w128`).
fn wrap_big(f: Frag, k: IntKind) -> Frag {
    let name = if k.is_signed() { "asIntN" } else { "asUintN" };
    f.wrap(&format!("BigInt.{name}({}, ", k.bits()), ")")
}

/// A non-negative integer literal below 2^31: `x & lit` on a 64-bit number kind is JS's `&`
/// (which takes the low 32 bits of `x`, and only the literal's bits are kept).
fn small_mask_literal(e: &ast::Expr) -> bool {
    match strip_parens_expr(e) {
        ast::Expr::Literal(l) => match l.kind() {
            LiteralKind::IntNumber(n) => n.value().is_ok_and(|v| v < (1 << 31)),
            _ => false,
        },
        _ => false,
    }
}

/// An integer literal other than zero (`x / 7`, `i % 2`): dividing by it can't panic.
fn nonzero_literal(e: Option<ast::Expr>) -> bool {
    match e.map(|e| strip_parens_expr(&e)) {
        Some(ast::Expr::Literal(l)) => match l.kind() {
            LiteralKind::IntNumber(n) => n.value().is_ok_and(|v| v != 0),
            _ => false,
        },
        _ => false,
    }
}

/// An index cheap enough to evaluate three times for an inline bounds check: a name, a literal,
/// a field, or arithmetic on those (`i`, `j - 1`, `self.pos + 2`).
fn cheap_index(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::PathExpr(_) | ast::Expr::Literal(_) => true,
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| cheap_index(&i)),
        ast::Expr::FieldExpr(f) => f.expr().is_some_and(|i| cheap_index(&i)),
        ast::Expr::BinExpr(b) => {
            matches!(
                b.op_kind(),
                Some(BinaryOp::ArithOp(ArithOp::Add | ArithOp::Sub))
            ) && b.lhs().is_some_and(|l| cheap_index(&l))
                && b.rhs().is_some_and(|r| cheap_index(&r))
        }
        _ => false,
    }
}

/// Expressions that are side-effect free and cheap, so lowering may evaluate them twice
/// (`v[κσ.$ix(v, i)]` reads `v` for the bounds check and again for the access).
fn is_pure(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::PathExpr(_) | ast::Expr::Literal(_) => true,
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| is_pure(&i)),
        ast::Expr::RefExpr(r) => r.expr().is_some_and(|i| is_pure(&i)),
        ast::Expr::PrefixExpr(p) => p.expr().is_some_and(|i| is_pure(&i)),
        ast::Expr::FieldExpr(f) => f.expr().is_some_and(|i| is_pure(&i)),
        ast::Expr::CastExpr(c) => c.expr().is_some_and(|i| is_pure(&i)),
        ast::Expr::IndexExpr(ix) => {
            ix.base().is_some_and(|b| is_pure(&b)) && ix.index().is_some_and(|i| is_pure(&i))
        }
        ast::Expr::BinExpr(b) => {
            !matches!(b.op_kind(), Some(BinaryOp::Assignment { .. }))
                && b.lhs().is_some_and(|l| is_pure(&l))
                && b.rhs().is_some_and(|r| is_pure(&r))
        }
        _ => false,
    }
}

/// `&v[a..b]` (or `v[a..]`, `v[..b]`) of a place that's safe to evaluate twice.
fn is_range_of_pure(e: &ast::Expr) -> bool {
    match strip_refs(e) {
        ast::Expr::IndexExpr(ix) => {
            matches!(ix.index(), Some(ast::Expr::RangeExpr(_)))
                && ix.base().is_some_and(|b| is_pure(&b))
        }
        _ => false,
    }
}

/// Whether no `&mut` argument of `f` can outlive a call: not `async`, no lifetime parameters
/// (on `f` or on its parameters, which an `impl<'a>` block's method could store into `self`),
/// and a return type without references, lifetimes, `impl` / `dyn`, type parameters or types
/// with lifetime parameters (`borrowing`), any of which could hand the borrow back to the caller.
fn writeback_safe(f: &ast::Fn, borrowing: &std::collections::HashSet<String>) -> bool {
    if f.async_token().is_some() {
        return false;
    }
    let mut type_params = Vec::new();
    if let Some(g) = f.generic_param_list() {
        if g.lifetime_params().next().is_some() {
            return false;
        }
        for t in g.generic_params() {
            if let ast::GenericParam::TypeParam(t) = t
                && let Some(n) = t.name()
            {
                type_params.push(n.text().to_string());
            }
        }
    }
    if let Some(pl) = f.param_list()
        && pl
            .syntax()
            .descendants_with_tokens()
            .any(|e| e.kind() == SyntaxKind::LIFETIME_IDENT)
    {
        return false;
    }
    let Some(ret) = f.ret_type().and_then(|r| r.ty()) else {
        return true;
    };
    ret.syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .all(|t| match t.kind() {
            SyntaxKind::AMP
            | SyntaxKind::LIFETIME_IDENT
            | SyntaxKind::IMPL_KW
            | SyntaxKind::DYN_KW
            | SyntaxKind::STAR => false,
            SyntaxKind::IDENT => {
                !type_params.iter().any(|p| p == t.text()) && !borrowing.contains(t.text())
            }
            _ => true,
        })
}

/// Whether `v[i]` may be written through: the target of an assignment, borrowed (`&mut v[i]`),
/// a method's receiver (`v[i].push_str(..)` is `v[i] += ..`) or under a prefix operator
/// (`*v[i] = ..`). Anything else only reads the element.
fn index_is_place(ix: &ast::IndexExpr) -> bool {
    let mut node = ix.syntax().clone();
    while let Some(p) = node.parent()
        && ast::ParenExpr::can_cast(p.kind())
    {
        node = p;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if let Some(b) = ast::BinExpr::cast(parent.clone()) {
        return matches!(b.op_kind(), Some(BinaryOp::Assignment { .. }))
            && b.lhs().is_some_and(|l| l.syntax() == &node);
    }
    if let Some(m) = ast::MethodCallExpr::cast(parent.clone()) {
        return m.receiver().is_some_and(|r| r.syntax() == &node);
    }
    ast::RefExpr::can_cast(parent.kind())
        || ast::PrefixExpr::can_cast(parent.kind())
        || ast::LetExpr::can_cast(parent.kind())
        || ast::MatchExpr::can_cast(parent.kind())
        || ast::ForExpr::can_cast(parent.kind())
}

/// The last segment of a path type (`Stack<T>` → `Stack`, `a::B` → `B`).
fn type_last_name(t: &ast::Type) -> Option<String> {
    match t {
        ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
        ast::Type::ParenType(p) => type_last_name(&p.ty()?),
        _ => None,
    }
}

fn strip_parens_expr(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::ParenExpr(p) => p
            .expr()
            .map(|i| strip_parens_expr(&i))
            .unwrap_or_else(|| e.clone()),
        _ => e.clone(),
    }
}

fn strip_refs(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::ParenExpr(p) => p
            .expr()
            .map(|i| strip_refs(&i))
            .unwrap_or_else(|| e.clone()),
        ast::Expr::RefExpr(r) => r
            .expr()
            .map(|i| strip_refs(&i))
            .unwrap_or_else(|| e.clone()),
        _ => e.clone(),
    }
}

pub fn compile(src: &str, filename: &str) -> CompileOutput {
    compile_with(src, filename, &|_| None)
}

static OVERFLOW_CHECKS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Debug-build overflow checks: integer arithmetic that overflows panics instead of wrapping.
pub fn set_overflow_checks(on: bool) {
    OVERFLOW_CHECKS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn overflow_checks() -> bool {
    OVERFLOW_CHECKS.load(std::sync::atomic::Ordering::Relaxed)
}

/// An integer kind's range as BigInt literals (JS compares them with numbers exactly).
pub(crate) fn int_bounds(k: IntKind) -> (String, String) {
    let (min, max): (i128, u128) = match k {
        IntKind::I8 => (i8::MIN.into(), i8::MAX as u128),
        IntKind::I16 => (i16::MIN.into(), i16::MAX as u128),
        IntKind::I32 => (i32::MIN.into(), i32::MAX as u128),
        IntKind::I64 | IntKind::Isize => (i64::MIN.into(), i64::MAX as u128),
        IntKind::I128 => (i128::MIN, i128::MAX as u128),
        IntKind::U8 => (0, u8::MAX.into()),
        IntKind::U16 => (0, u16::MAX.into()),
        IntKind::U32 => (0, u32::MAX.into()),
        IntKind::U64 | IntKind::Usize => (0, u64::MAX.into()),
        IntKind::U128 => (0, u128::MAX),
    };
    (format!("{min}n"), format!("{max}n"))
}

/// Compile `src`; `resolver` reads the files of its `mod` declarations (paths relative to the
/// project root, like `lib/util.rs`).
pub fn compile_with(src: &str, filename: &str, resolver: typeck::Resolver) -> CompileOutput {
    thread_local! {
        static COMPILES: crate::memo::Cache<CompileOutput> = Default::default();
    }
    crate::memo::cached(&COMPILES, src, filename, resolver, |resolver| {
        compile_uncached(src, filename, resolver)
    })
}

fn compile_uncached(src: &str, filename: &str, resolver: typeck::Resolver) -> CompileOutput {
    let src = &crate::lineindex::source_fixups(src);
    let resolve =
        |p: &str| resolver(p).map(|t| crate::lineindex::source_fixups(&t).into_owned());
    let resolver: typeck::Resolver = &resolve;
    // (A proc-macro crate compiled for this compile keeps its own list.)
    let outer = crate::procmacro::take_dependencies();
    let mut out = compile_files(src, filename, resolver);
    out.dependencies = crate::procmacro::take_dependencies();
    crate::procmacro::restore_dependencies(outer);
    out
}

fn compile_files(src: &str, filename: &str, resolver: typeck::Resolver) -> CompileOutput {
    // A proc-macro crate: `use proc_macro::..` set aside and `TokenStream` a `String`.
    if crate::procmacro::is_proc_crate(src) {
        return compile_files(&crate::procmacro::prepared(src), filename, resolver);
    }
    // `macro_rules!` macros are expanded and inline modules flattened first; positions are
    // reported in `src`.
    if src.contains("macro_rules")
        || src.contains("include!")
        || src.contains("mod ")
        || src.contains("self::")
        || src.contains("super::")
        || src.contains("type ")
        || src.contains("use ")
        || crate::ufcs::has_prim_paths(src)
        || src.contains("include_str!")
        || src.contains("include_bytes!")
        || src.contains("cfg(")
        || crate::stdgaps::has_rewrites(src)
        // (`mac::show!(..)`, `#[derive(mac::Show)]`: a proc-macro crate, maybe)
        || src.contains("::")
        // (A module file may use macros of the file that declares it.)
        || (src.contains('!') && !matches!(filename.rsplit('/').next(), Some("main.rs" | "lib.rs")))
        // (Nested `fn`s that share a name, renamed by `nested.rs`.)
        || crate::nested::has_duplicate_fns(src)
    {
        let macros = crate::mbe::imported_macros(src, filename, resolver);
        match crate::mbe::preprocess_with(src, &macros, Some((filename, resolver))) {
            Ok(Some(pre)) => {
                let map = std::rc::Rc::new(pre.map);
                return compile_text(&pre.text, src, Some(map), pre.names, filename, resolver);
            }
            Ok(None) => {}
            Err(errs) => {
                let idx = LineIndex::new(src);
                let diagnostics = errs
                    .into_iter()
                    .map(|(start, end, message)| {
                        let (l0, c0) = idx.line_col(start);
                        let (l1, c1) = idx.line_col(end);
                        Diagnostic {
                            message,
                            severity: "error",
                            help: None,
                            start,
                            end,
                            line: l0 + 1,
                            column: c0 + 1,
                            end_line: l1 + 1,
                            end_column: c1 + 1,
                        }
                    })
                    .collect();
                return CompileOutput {
                    code: None,
                    source_map: None,
                    diagnostics,
                    dependencies: Vec::new(),
                };
            }
        }
    }
    compile_text(src, src, None, HashMap::new(), filename, resolver)
}

/// Compile `src`, which is `original` with its macros expanded when `map` is set.
fn compile_text(
    src: &str,
    original: &str,
    map: Option<std::rc::Rc<crate::mbe::OffsetMap>>,
    display_names: HashMap<String, String>,
    filename: &str,
    resolver: typeck::Resolver,
) -> CompileOutput {
    // `std::time`'s types, compiled in after the program when it uses them.
    let with_time = crate::stdtime::with_prelude(src, filename, resolver, false);
    let prelude_at = if with_time.is_some() {
        src.len()
    } else {
        usize::MAX
    };
    let src = with_time.as_deref().unwrap_or(src);
    let parse = crate::macros::parse_file(src);
    let mut cg = Codegen {
        serde: None,
        drop_flags: HashMap::new(),
        stmt_drops: Vec::new(),
        param_drops: Vec::new(),
        async_body: false,
        src,
        filename,
        idx: match &map {
            Some(m) => LineIndex::mapped(original, m.clone()),
            None => LineIndex::new(src),
        },
        out: Out {
            code: String::new(),
            line: 0,
            col: 0,
            indent: 0,
            mappings: Vec::new(),
            marks: Vec::new(),
        },
        diags: Vec::new(),
        type_errors: false,
        fns: HashMap::new(),
        scopes: Vec::new(),
        used_names: HashMap::new(),
        name_log: Vec::new(),
        tmp: 0,
        recv_override: None,
        wb_methods: std::collections::HashSet::new(),
        base: 0,
        ret_ty: Ty::Unit,
        in_async: false,
        types: Types::default(),
        self_adt: None,
        consts: HashMap::new(),
        local_adts: Default::default(),
        file_traits: Vec::new(),
        file_root: None,
        display_names,
        slice_view: false,
        view_params: HashMap::new(),
        view_args: Vec::new(),
        dict_scope: Vec::new(),
        blanket_this: None,
        trait_const_args: HashMap::new(),
        param_consts: Default::default(),
        extra_consts: Vec::new(),
        ext_class: None,
        in_trait_default: false,
        loop_dests: Vec::new(),
        prelude_at,
        crate_imports: Default::default(),
        const_bodies: HashMap::new(),
    };
    for e in parse.errors() {
        let r = e.range();
        cg.diag_range(
            usize::from(r.start()),
            usize::from(r.end()),
            crate::diag_help::parser_message(&e.to_string()),
            "error",
        );
    }
    if cg.diags.is_empty() {
        let file = parse.tree();
        let origin = typeck::Origin {
            map: map.clone(),
            len: original.len(),
        };
        let (types, type_diags) = typeck::check(&file, src, filename, resolver, &origin);
        cg.types = types;
        cg.type_errors = type_diags.iter().any(|d| d.severity == "error");
        cg.emit_file(&file);
        // The type checker owns name and type errors. Codegen may report the same problem at the
        // same node (it re-checks some things while lowering); the checker's report wins.
        let typed: Vec<(usize, usize)> = type_diags
            .iter()
            .filter(|d| d.severity == "error")
            .map(|d| (d.start, d.end))
            .collect();
        cg.diags.retain(|d| !typed.contains(&(d.start, d.end)));
        for d in type_diags {
            if !cg
                .diags
                .iter()
                .any(|e| e.start == d.start && e.end == d.end && e.message == d.message)
            {
                cg.diag_range_help(d.start, d.end, d.message, d.severity, d.help);
            }
        }
        cg.diags.sort_by_key(|d| (d.start, d.end));
    }
    let has_errors = cg.diags.iter().any(|d| d.severity == "error");
    let (code, source_map) = if has_errors {
        (None, None)
    } else {
        let mut mappings = std::mem::take(&mut cg.out.mappings);
        (
            Some(std::mem::take(&mut cg.out.code)),
            Some(sourcemap::to_json(filename, original, &mut mappings)),
        )
    };
    let mut diagnostics = cg.diags;
    if let Some(m) = &map {
        // Offsets in `original`; a range inside an expansion covers its whole invocation.
        let idx = LineIndex::new(original);
        for d in &mut diagnostics {
            d.start = m.orig(d.start, false);
            d.end = m.orig(d.end, true).max(d.start);
            let (l1, c1) = idx.line_col(d.end);
            (d.end_line, d.end_column) = (l1 + 1, c1 + 1);
        }
    }
    CompileOutput {
        code,
        source_map,
        diagnostics,
        dependencies: Vec::new(),
    }
}

struct Codegen<'a> {
    src: &'a str,
    /// Where the `std::time` prelude starts in `src` (`usize::MAX` without one): its code has no
    /// source mappings, and its panics are located in `<std>`.
    prelude_at: usize,
    /// `#[serde(..)]` attributes of the file's types, read when `JSON` first needs them.
    serde: Option<std::rc::Rc<HashMap<String, crate::json::SerdeInfo>>>,
    /// Locals whose value has a `Drop` impl somewhere in it: JS name -> the flag that says it
    /// still owns the value (cleared where it's moved).
    drop_flags: HashMap<String, String>,
    /// Closures given to std methods in the statement being emitted: (flag, JS name), dropped
    /// after it.
    stmt_drops: Vec<(String, String)>,
    /// A function's by-value parameters to drop when its body ends: (JS access, type).
    param_drops: Vec<(String, Ty)>,
    /// The next block body is an `async` block's (emitted as a function body).
    async_body: bool,
    filename: &'a str,
    idx: LineIndex<'a>,
    out: Out,
    diags: Vec<Diagnostic>,
    /// The type checker reported errors: types it couldn't work out (`Ty::Unknown`) follow from
    /// one of them, so codegen needn't report what it can't do with them.
    type_errors: bool,
    fns: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, Local>>,
    used_names: HashMap<String, u32>,
    /// `used_names`' changes since the file's globals were all in (what `fresh` found for
    /// each name): an item's names are undone from it, see `undo_names`.
    name_log: Vec<(String, Option<u32>)>,
    tmp: u32,
    /// While emitting `v[a..b].sort()`: the receiver's range and the temporary holding the copy.
    recv_override: Option<((usize, usize), String, Ty)>,
    /// Methods (by type and name) that can't keep a `&mut` argument past the call
    /// (`writeback_safe`): their `&mut place` arguments are write-back temps.
    wb_methods: std::collections::HashSet<(String, String)>,
    /// Absolute offset of the text the current AST was parsed from. Non-zero while lowering
    /// macro arguments, which are re-parsed from substrings of the source.
    base: usize,
    ret_ty: Ty,
    in_async: bool,
    /// Resolved types from the type checker, keyed by absolute source range.
    types: Types,
    /// The struct or enum whose `impl` is being emitted (`Self`).
    self_adt: Option<String>,
    /// `const` and `static` items: Rust name → JS name.
    consts: HashMap<String, String>,
    /// Structs and enums declared in this file (not imported).
    local_adts: std::collections::HashSet<String>,
    /// The file's trait declarations (their default methods are compiled again for built-in
    /// types that implement them).
    file_traits: Vec<ast::Trait>,
    /// The file being compiled (a macro argument is parsed on its own: its nodes don't reach
    /// it).
    file_root: Option<SyntaxNode>,
    /// Types renamed when inline modules were flattened (`util__Point`), by their written name.
    display_names: HashMap<String, String>,
    /// The range index being emitted is `&mut v[a..b]`'s: a view, not a copy.
    slice_view: bool,
    /// The file's functions whose `&[T]` parameters can take a view of `&v[a..b]` (see
    /// `views_gen`), and the flags for the call whose arguments are being emitted.
    view_params: HashMap<String, Vec<bool>>,
    view_args: Vec<bool>,
    /// The type parameters the function being emitted takes a class for (`$T`).
    dict_scope: Vec<String>,
    /// Emitting a blanket impl's associated function (`impl<T: Default> Make for T`): its `T`,
    /// whose class is `this`.
    pub(super) blanket_this: Option<String>,
    /// For each impl of a trait with const parameters (`impl Buf<4> for A`), by (type key,
    /// trait): the JS accessors of its const arguments (`() => 4`, `(s) => s.$M`), which its
    /// copy of the trait's default methods takes (`Buf$defaults(..)`).
    pub(super) trait_const_args: HashMap<(String, String), Vec<String>>,
    /// Associated consts whose value reads the impl's const parameters (`impl<const P: u128>
    /// U<P> { const PER: u128 = P * 1000; }`), by (type, name): getters read through a class
    /// view (`κσ.$cv(U, [7]).PER`).
    pub(super) param_consts: std::collections::HashSet<(String, String)>,
    /// Const values every method emitted next has in scope besides its own (a trait's const
    /// parameters in its default methods: `N` is `$N(this)`), as (name, JS, type).
    pub(super) extra_consts: Vec<(String, String, Ty)>,
    /// Emitting the class of your trait impls for a built-in type (`$ext$i32`): its key.
    pub(super) ext_class: Option<String>,
    /// Emitting a trait's default methods: `Self::f()` is `this` (static) or `this.constructor`.
    in_trait_default: bool,
    /// Enclosing loops, innermost last: (label, where a `loop`'s `break value` goes; None for
    /// `while`/`for`).
    loop_dests: Vec<(Option<String>, Option<Dest>, bool)>,
    /// The file's `use` imports from the bundled crates (`crate_imports`), read once.
    crate_imports: std::cell::OnceCell<HashMap<String, (String, String)>>,
    /// The file's `const` / `static` values by name: their range, for `{NAME}` in format strings.
    const_bodies: HashMap<String, (usize, usize)>,
}

impl<'a> Codegen<'a> {
    /// `*map.get_mut(k).unwrap()` of a `HashMap` / `BTreeMap`: the map and the key, for a cell
    /// over that entry (`index_mut`'s `&mut *m.get_mut(k).unwrap()`).
    fn map_entry_place(&self, e: &ast::Expr) -> Option<(ast::Expr, ast::Expr)> {
        let ast::Expr::PrefixExpr(pe) = strip_parens_expr(e) else {
            return None;
        };
        if pe.op_kind() != Some(UnaryOp::Deref) {
            return None;
        }
        let ast::Expr::MethodCallExpr(unwrap) = strip_parens_expr(&pe.expr()?) else {
            return None;
        };
        if unwrap.name_ref()?.text() != "unwrap" {
            return None;
        }
        let ast::Expr::MethodCallExpr(get) = strip_parens_expr(&unwrap.receiver()?) else {
            return None;
        };
        if get.name_ref()?.text() != "get_mut" {
            return None;
        }
        let map = get.receiver()?;
        let mut args = get.arg_list()?.args();
        let (key, None) = (args.next()?, args.next()) else {
            return None;
        };
        match self.raw_ty_of(map.syntax())? {
            Ty::Map(..) | Ty::Coll(crate::ty::Coll::BTreeMap, _) => Some((map, key)),
            _ => None,
        }
    }

    // ---------------------------------------------------------------- diagnostics & positions

    fn off(&self, n: &SyntaxNode) -> usize {
        self.base + usize::from(n.text_range().start())
    }

    fn diag_range(&mut self, start: usize, end: usize, message: String, severity: &'static str) {
        self.diag_range_help(start, end, message, severity, None);
    }

    /// A diagnostic with its help line; without one, the message's shape picks it (`diag_help`).
    fn diag_range_help(
        &mut self,
        start: usize,
        end: usize,
        message: String,
        severity: &'static str,
        help: Option<String>,
    ) {
        let (l0, c0) = self.idx.line_col(start);
        let (l1, c1) = self.idx.line_col(end);
        let help = help.or_else(|| crate::diag_help::for_message(&message, severity));
        self.diags.push(Diagnostic {
            message,
            severity,
            help,
            start,
            end,
            line: l0 + 1,
            column: c0 + 1,
            end_line: l1 + 1,
            end_column: c1 + 1,
        });
    }

    fn error(&mut self, n: &SyntaxNode, message: impl Into<String>) {
        let r = n.text_range();
        let (s, e) = (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        );
        self.diag_range(s, e, message.into(), "error");
    }

    fn warn(&mut self, n: &SyntaxNode, message: impl Into<String>) {
        let r = n.text_range();
        let (s, e) = (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        );
        self.diag_range(s, e, message.into(), "warning");
    }

    fn unsupported(&mut self, n: &SyntaxNode, what: &str) {
        self.error(n, format!("{what} is not supported yet"));
    }

    /// `file:line:col` for panic messages, 1-based like rustc.
    fn location(&self, n: &SyntaxNode) -> String {
        self.location_at(self.off(n))
    }

    fn location_at(&self, offset: usize) -> String {
        if offset >= self.prelude_at {
            return "<std>".into();
        }
        let (l, c) = self.idx.line_col(offset);
        format!("{}:{}:{}", self.filename, l + 1, c + 1)
    }

    fn line(&mut self, frag: Frag) {
        if self.prelude_at != usize::MAX {
            let mut frag = frag;
            for (src, _) in &mut frag.parts {
                if src.is_some_and(|s| s >= self.prelude_at) {
                    *src = None;
                }
            }
            self.out.line(&frag, &self.idx);
            return;
        }
        self.out.line(&frag, &self.idx);
    }

    // ---------------------------------------------------------------- scopes & names

    fn fresh(&mut self, rust_name: &str) -> String {
        let base = escape_ident(rust_name);
        self.name_log
            .push((base.clone(), self.used_names.get(&base).copied()));
        let n = self.used_names.entry(base.clone()).or_insert(0);
        let name = if *n == 0 {
            base.clone()
        } else {
            format!("{base}${n}")
        };
        *n += 1;
        name
    }

    /// Put `used_names` back as it was when `name_log` had `mark` entries.
    fn undo_names(&mut self, mark: usize) {
        while self.name_log.len() > mark {
            let (name, before) = self.name_log.pop().unwrap();
            match before {
                Some(n) => self.used_names.insert(name, n),
                None => self.used_names.remove(&name),
            };
        }
    }

    /// A node's absolute source range (the type checker's table key).
    fn range_of(&self, n: &SyntaxNode) -> (usize, usize) {
        let r = n.text_range();
        (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        )
    }

    /// Whether the binding named at `name` lives in a cell.
    fn is_boxed(&self, name: &SyntaxNode) -> bool {
        self.types.boxed.contains(&self.range_of(name))
    }

    fn temp(&mut self) -> String {
        self.tmp += 1;
        format!("$t{}", self.tmp)
    }

    /// Bind a local. `name_node` is the binding's name in the source, whose resolved type the
    /// type checker recorded.
    fn declare(&mut self, rust_name: &str, name_node: Option<&SyntaxNode>, mut local: Local) {
        if let Some(ty) = name_node.and_then(|n| self.ty_of(n)) {
            local.ty = ty;
        }
        // A binding of a `get_mut` slot reads and writes the slot's value.
        if let Some(n) = name_node
            && self.types.slot_bindings.contains(&self.range_of(n))
        {
            local.js = format!("{}[1]", local.js);
        }
        self.scopes
            .last_mut()
            .unwrap()
            .insert(rust_name.to_string(), local);
    }

    fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// The type checker's resolved type for a node (an expression or a binding name), with std
    /// collections as their views (see `Ty::view`).
    fn ty_of(&self, n: &SyntaxNode) -> Option<Ty> {
        self.raw_ty_of(n).map(|t| t.view())
    }

    /// `ty_of` keeping `VecDeque`, `BTreeMap`, .. as themselves.
    fn raw_ty_of(&self, n: &SyntaxNode) -> Option<Ty> {
        let r = n.text_range();
        self.types
            .get(
                self.base + usize::from(r.start()),
                self.base + usize::from(r.end()),
            )
            .cloned()
    }

    /// The conversion the type checker chose for a `Y::from(x)`, `x.into()` or `?`.
    fn conv_of(&self, n: &SyntaxNode) -> Option<crate::adt::Conv> {
        let r = n.text_range();
        let key = (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        );
        self.types.conversions.get(&key).cloned()
    }

    /// Whether `p` is an argument of a method that isn't one of the file's own types' (a std
    /// method: `it.map(f)`).
    fn std_method_arg(&self, p: &ast::PathExpr) -> bool {
        let Some(m) = p
            .syntax()
            .parent()
            .filter(|x| ast::ArgList::can_cast(x.kind()))
            .and_then(|l| l.parent())
            .and_then(ast::MethodCallExpr::cast)
        else {
            return false;
        };
        let Some(r) = m.receiver() else { return false };
        !matches!(self.ty_of(r.syntax()), Some(Ty::Adt(n, _)) if self.types.adts.defs.contains_key(&n))
    }

    /// `s.inner` moved out of its struct, with something to drop: `κσ.$moveOut(s, "inner")`, so
    /// dropping `s` later skips it (its new owner drops it).
    fn field_move_out(&mut self, fe: &ast::FieldExpr, (f, t): (Frag, Ty)) -> (Frag, Ty) {
        if !self.types.move_sites.contains(&self.range_of(fe.syntax())) || !self.needs_drop(&t) {
            return (f, t);
        }
        let Some(recv) = fe.expr() else { return (f, t) };
        if !matches!(self.ty_of(recv.syntax()), Some(Ty::Adt(..))) {
            return (f, t);
        }
        let text = f.plain();
        // (`<receiver>.<prop>`: split at the last `.`, when the property is a plain name.)
        let Some(dot) = text.rfind('.') else {
            return (f, t);
        };
        let (base, prop) = (&text[..dot], &text[dot + 1..]);
        if base.is_empty()
            || !prop
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            return (f, t);
        }
        let out = Frag::mapped(
            self.off(fe.syntax()),
            format!("κσ.$moveOut({base}, {})", js_string(prop)),
        );
        (out, t)
    }

    /// A call giving an `Option<()>`: `Some(())` as a non-null marker (`()` is `undefined`, which
    /// would read as `None`).
    fn some_unit(&self, node: &SyntaxNode, (f, t): (Frag, Ty)) -> (Frag, Ty) {
        let opt_unit = |t: &Ty| matches!(t, Ty::Option(inner) if **inner == Ty::Unit);
        if opt_unit(&t) || self.ty_of(node).is_some_and(|x| opt_unit(&x)) {
            (f.wrap("κσ.$someUnit(", ")"), t)
        } else {
            (f, t)
        }
    }

    /// `value` converted: `Y.from$0(value)`, or unchanged.
    fn convert(&self, conv: crate::adt::Conv, value: Frag) -> Frag {
        match conv {
            crate::adt::Conv::Identity => value,
            crate::adt::Conv::Method { key } => value.wrap("(", &format!(").{key}()")),
            crate::adt::Conv::Ext { .. } | crate::adt::Conv::ExtDict { .. } => value,
            crate::adt::Conv::From { adt, key } if self.types.adts.ext_types.contains(&adt) => {
                value.wrap(&format!("$ext${adt}.{key}("), ")")
            }
            crate::adt::Conv::From { adt, key } => {
                value.wrap(&format!("{}.{}(", self.class(&adt), key), ")")
            }
            crate::adt::Conv::TryInt { bits, signed, big } => {
                value.wrap("κσ.$tryInt(", &format!(", {bits}, {signed}, {big})"))
            }
            crate::adt::Conv::TryFrom { adt, key } => {
                value.wrap(&format!("{}.{key}(", self.class(&adt)), ")")
            }
            crate::adt::Conv::TryVec { len: Some(n), owned } => {
                value.wrap("κσ.$arrayTryFrom(", &format!(", {n}, {})", !owned))
            }
            crate::adt::Conv::TryVec { len: None, .. } => value.wrap("κσ.Ok(κσ.$clone(", "))"),
            crate::adt::Conv::TryChar => value.wrap("κσ.$tryChar(", ")"),
            crate::adt::Conv::Wrap { js } => value.wrap(&format!("{js}("), ")"),
        }
    }

    // ---------------------------------------------------------------- types

    /// Lower a type annotation. The type checker reports unsupported types; this only needs the
    /// shape (for casts and function signatures).
    fn lower_type(&mut self, t: &ast::Type) -> Ty {
        match t {
            ast::Type::RefType(r) => r.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::PtrType(r) => r.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::ParenType(p) => p.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            ast::Type::TupleType(tt) if tt.fields().next().is_none() => Ty::Unit,
            ast::Type::TupleType(tt) => {
                Ty::Tuple(tt.fields().map(|t| self.lower_type(&t)).collect())
            }
            ast::Type::NeverType(_) => Ty::Never,
            ast::Type::SliceType(s) => Ty::Vec(Box::new(
                s.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            )),
            ast::Type::ArrayType(a) => Ty::Vec(Box::new(
                a.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
            )),
            ast::Type::PathType(p) => {
                let seg = p.path().and_then(|p| p.segment());
                match seg.as_ref().and_then(|s| s.name_ref()) {
                    Some(n) if n.text() == "Vec" => Ty::Vec(Box::new(Ty::Unknown)),
                    Some(n) if n.text() == "HashSet" => Ty::Set(Box::new(Ty::Unknown)),
                    Some(n)
                        if n.text() == "Ordering"
                            && !self.types.adts.defs.contains_key("Ordering") =>
                    {
                        Ty::Ordering
                    }
                    Some(n) if n.text() == "HashMap" => {
                        Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))
                    }
                    Some(n)
                        if self
                            .types
                            .adts
                            .defs
                            .contains_key(n.text().to_string().as_str()) =>
                    {
                        Ty::Adt(n.text().to_string(), vec![])
                    }
                    Some(n) if n.text() == "Self" && self.self_adt.is_some() => {
                        Ty::Adt(self.self_adt.clone().unwrap(), vec![])
                    }
                    Some(n) => Ty::from_name(n.text()),
                    None => Ty::Unknown,
                }
            }
            _ => Ty::Unknown,
        }
    }

    // ---------------------------------------------------------------- items

    fn emit_file(&mut self, file: &ast::SourceFile) {
        self.file_root = Some(file.syntax().clone());
        // Pass 1: signatures, so calls can be lowered before the callee is emitted.
        // (Types with lifetime parameters, which a return type can name with the lifetime
        // elided: `-> Holder`.)
        let borrowing: std::collections::HashSet<String> = crate::adt::all_items(file)
            .iter()
            .filter_map(|item| {
                let (name, generics) = match item {
                    ast::Item::Struct(s) => (s.name()?, s.generic_param_list()?),
                    ast::Item::Enum(e) => (e.name()?, e.generic_param_list()?),
                    ast::Item::Union(u) => (u.name()?, u.generic_param_list()?),
                    ast::Item::TypeAlias(t) => (t.name()?, t.generic_param_list()?),
                    _ => return None,
                };
                generics
                    .lifetime_params()
                    .next()
                    .map(|_| name.text().to_string())
            })
            .collect();
        for item in crate::adt::all_items(file) {
            if let ast::Item::Fn(f) = &item
                && let Some(name) = f.name()
            {
                let params = f
                    .param_list()
                    .map(|pl| {
                        pl.params()
                            .map(|p| p.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown))
                            .collect()
                    })
                    .unwrap_or_default();
                let ret = f
                    .ret_type()
                    .and_then(|r| r.ty())
                    .map(|t| self.lower_type(&t))
                    .unwrap_or(Ty::Unit);
                let js = escape_ident(name.text());
                self.used_names.insert(js.clone(), 1);
                let is_async = f.async_token().is_some();
                let sig = FnSig {
                    js,
                    params,
                    ret,
                    is_async,
                    writeback: writeback_safe(f, &borrowing),
                };
                self.fns.insert(name.text().to_string(), sig);
            }
        }
        // Methods whose `&mut place` arguments can be write-back temps (`write_back_call`): by
        // (type, name), when every method of that name on the type qualifies.
        let mut wb_ok = std::collections::HashSet::new();
        let mut wb_bad = std::collections::HashSet::new();
        for item in crate::adt::all_items(file) {
            let ast::Item::Impl(imp) = &item else {
                continue;
            };
            let Some(ty_name) = imp.self_ty().and_then(|t| type_last_name(&t)) else {
                continue;
            };
            let Some(list) = imp.assoc_item_list() else {
                continue;
            };
            for ai in list.assoc_items() {
                let ast::AssocItem::Fn(f) = ai else { continue };
                let Some(n) = f.name() else { continue };
                let key = (ty_name.clone(), n.text().to_string());
                if writeback_safe(&f, &borrowing) {
                    wb_ok.insert(key);
                } else {
                    wb_bad.insert(key);
                }
            }
        }
        self.wb_methods = wb_ok.difference(&wb_bad).cloned().collect();
        self.view_params = views_gen::view_safe_params(file);
        // Functions and constants from `mod` files live on their module's namespace.
        for module in self.types.modules.clone() {
            self.used_names.insert(escape_ident(&module.name), 1);
        }
        let local_fns: std::collections::HashSet<String> = self.fns.keys().cloned().collect();
        for imp in self.types.imported.clone() {
            // (A local function of the same name keeps it; the module's is `util::name`.)
            if imp.sig.is_some() && local_fns.contains(&imp.name) {
                continue;
            }
            let js = format!("{}.{}", escape_ident(&imp.module), escape_ident(&imp.js));
            match imp.sig {
                Some(sig) => {
                    let sig = FnSig {
                        js,
                        params: sig.params,
                        ret: sig.ret,
                        is_async: sig.is_async,
                        writeback: false,
                    };
                    self.fns.insert(imp.name, sig);
                }
                None => {
                    self.consts.insert(imp.name, js);
                }
            }
        }
        // `const` / `static` items are module-level `const`s, emitted last (see `emit_consts`).
        for item in crate::adt::all_items(file) {
            let name = match &item {
                ast::Item::Const(k) => k.name(),
                ast::Item::Static(s) => s.name(),
                _ => None,
            };
            if let Some(n) = name {
                let js = escape_ident(n.text().to_string().as_str());
                self.used_names.insert(js.clone(), 1);
                self.consts.insert(n.text().to_string(), js);
                let body = match &item {
                    ast::Item::Const(k) => k.body(),
                    ast::Item::Static(s) => s.body(),
                    _ => None,
                };
                if let Some(b) = body {
                    let r = b.syntax().text_range();
                    self.const_bodies
                        .entry(n.text().to_string())
                        .or_insert((usize::from(r.start()), usize::from(r.end())));
                }
            }
        }
        for item in crate::adt::all_items(file) {
            let name = match &item {
                ast::Item::Struct(s) => s.name(),
                ast::Item::Enum(e) => e.name(),
                _ => None,
            };
            if let Some(n) = name {
                self.local_adts.insert(n.text().to_string());
            }
            if let ast::Item::Trait(t) = &item {
                self.file_traits.push(t.clone());
            }
        }
        // `impl` blocks are emitted inside their type's class.
        let mut impls: HashMap<String, Vec<ast::Impl>> = HashMap::new();
        for item in crate::adt::all_items(file) {
            if let ast::Item::Impl(imp) = &item
                && let Some(t) = crate::adt::impl_parts(imp)
                    .1
                    .or_else(|| crate::adt::impl_self_name(imp))
            {
                // `impl<T: Display> Shout for T` goes into the `$ext$Shout$all` class.
                let t = match crate::adt::blanket_impl(imp) {
                    Some((tr, _, _)) if self.types.adts.blankets.contains_key(&tr) => {
                        format!("{tr}$all")
                    }
                    _ => t,
                };
                // `impl Money for f64` goes into the `$ext$f64` class.
                let t = match crate::adt::ext_key_any(&t) {
                    Some(k)
                        if self.types.adts.ext_types.contains(&k)
                            && !self.local_adts.contains(&t) =>
                    {
                        k
                    }
                    _ => t,
                };
                impls.entry(t).or_default().push(imp.clone());
            }
        }
        self.note_const_impls(file);
        let mut ext_done: std::collections::HashSet<String> = Default::default();
        // Pass 2: code. Impls for built-in types register in this script's own registry. A file
        // with `pub` traits exports it: a file that loads this one with `mod` registers its
        // impls of them there too, so this file's generic functions find them.
        if self.has_pub_traits() {
            self.line(Frag::text("export const $extRegs = κσ.$newRegs();"));
        } else if !self.types.adts.ext_types.is_empty() {
            self.line(Frag::text("const $extRegs = κσ.$newRegs();"));
        }
        // (Each item starts from the globals: its names are undone after it.)
        self.name_log.clear();
        for item in crate::adt::all_items(file) {
            match &item {
                ast::Item::Fn(f) => {
                    self.undo_names(0);
                    self.tmp = 0;
                    self.emit_fn(f);
                }
                ast::Item::Struct(s) => {
                    if let Some(n) = s.name() {
                        self.undo_names(0);
                        let imps = impls
                            .get(n.text().to_string().as_str())
                            .cloned()
                            .unwrap_or_default();
                        self.emit_adt(s.syntax(), &n, &imps);
                    }
                }
                ast::Item::Enum(e) => {
                    if let Some(n) = e.name() {
                        self.undo_names(0);
                        let imps = impls
                            .get(n.text().to_string().as_str())
                            .cloned()
                            .unwrap_or_default();
                        self.emit_adt(e.syntax(), &n, &imps);
                    }
                }
                // Impls for built-in types: one `$ext$key` class each, at the first impl.
                ast::Item::Impl(imp) => {
                    let blanket = crate::adt::blanket_impl(imp)
                        .map(|(tr, _, _)| format!("{tr}$all"))
                        .filter(|k| self.types.adts.ext_types.contains(k));
                    let key = blanket.or_else(|| {
                        crate::adt::impl_parts(imp)
                            .1
                            .or_else(|| crate::adt::impl_self_name(imp))
                            .and_then(|t| crate::adt::ext_key_any(&t))
                            .filter(|k| self.types.adts.ext_types.contains(k))
                    });
                    if let Some(k) = key
                        && ext_done.insert(k.clone())
                    {
                        self.undo_names(0);
                        let imps = impls.get(&k).cloned().unwrap_or_default();
                        self.emit_ext_class(&k, &imps);
                    }
                }
                ast::Item::Trait(t) => {
                    self.undo_names(0);
                    self.emit_trait_defaults(t);
                }
                ast::Item::Const(_) | ast::Item::Static(_) => {}
                // `use` only brings names into scope; the type checker resolves them. `pub use`
                // re-exports: `export { Circle } from "./circle.rs";`.
                ast::Item::Use(u) if ra_ap_syntax::ast::HasVisibility::visibility(u).is_some() => {
                    for (path, rename, star) in crate::typeck::use_leaves(u) {
                        let rel: Vec<&str> = path
                            .iter()
                            .map(String::as_str)
                            .skip_while(|s| matches!(*s, "crate" | "self"))
                            .collect();
                        let Some(module) = rel
                            .first()
                            .and_then(|m| self.types.modules.iter().find(|x| x.name == *m))
                            .cloned()
                        else {
                            continue;
                        };
                        let spec = js_string(&module.specifier);
                        let line = match (rel.as_slice(), star) {
                            ([_], true) => format!("export * from {spec};"),
                            // A trait exists in JS only as its default methods.
                            ([_, .., item], false)
                                if self.types.adts.traits.contains_key(*item) =>
                            {
                                let has_defaults = self.types.adts.traits[*item]
                                    .methods
                                    .iter()
                                    .any(|m| m.has_default);
                                if !has_defaults {
                                    continue;
                                }
                                let d = format!("{}$defaults", escape_ident(item));
                                format!("export {{ {d} }} from {spec};")
                            }
                            ([_, .., item], false) => {
                                let js = escape_ident(item);
                                match rename {
                                    Some(r) if r != *item => {
                                        format!(
                                            "export {{ {js} as {} }} from {spec};",
                                            escape_ident(&r)
                                        )
                                    }
                                    _ => format!("export {{ {js} }} from {spec};"),
                                }
                            }
                            _ => continue,
                        };
                        self.line(Frag::mapped(self.item_start(u.syntax()), line));
                    }
                }
                ast::Item::Use(_) | ast::Item::ExternCrate(_) => {}
                // `mod util;` → `import * as util from "./util.rs";`
                ast::Item::Module(m) => {
                    if let Some(n) = m.name()
                        && let Some(module) = self
                            .types
                            .modules
                            .iter()
                            .find(|x| x.name == n.text())
                            .cloned()
                    {
                        let mut line = Frag::mapped(self.item_start(m.syntax()), "import * as ");
                        line.append(Frag::mapped(
                            self.off(n.syntax()),
                            escape_ident(&module.name),
                        ))
                        .push(format!(" from {};", js_string(&module.specifier)));
                        self.line(line);
                        // `pub mod util;`: `parent::util::item` reaches it from other files.
                        if module.public {
                            self.line(Frag::text(format!(
                                "export {{ {} }};",
                                escape_ident(&module.name)
                            )));
                        }
                        self.line(Frag::default());
                    }
                }
                // `compile_error!("..");` as an item: an error unless it's under a `#[cfg]` that's
                // off.
                ast::Item::MacroCall(mc)
                    if crate::macros::macro_name(mc).as_deref() == Some("compile_error") =>
                {
                    self.item_compile_error(mc);
                }
                other => {
                    let kind = match other {
                        ast::Item::MacroRules(_) => "`macro_rules!`",
                        _ => "this item",
                    };
                    self.unsupported(other.syntax(), kind);
                }
            }
        }
        self.emit_mixins();
        self.emit_consts(file);
    }

    /// `export const NAME = value;` for each `const` / `static`, after everything else (JS `const`s
    /// can't be read before their declaration runs, and Rust allows any order), ordered so each
    /// comes after the constants it reads.
    fn emit_consts(&mut self, file: &ast::SourceFile) {
        let mut items: Vec<ConstItem> = Vec::new();
        for item in crate::adt::all_items(file) {
            let (name, body) = match &item {
                ast::Item::Const(k) => (k.name(), k.body()),
                ast::Item::Static(s) => (s.name(), s.body()),
                _ => continue,
            };
            if let Some(n) = name {
                items.push((n.text().to_string(), n, body, item.syntax().clone()));
            }
        }
        // Depth-first topological order over the constants each initializer reads.
        let index: HashMap<String, usize> = items
            .iter()
            .enumerate()
            .map(|(i, it)| (it.0.clone(), i))
            .collect();
        let deps: Vec<Vec<usize>> = items
            .iter()
            .map(|(_, _, body, _)| {
                body.iter()
                    .flat_map(|b| b.syntax().descendants().filter_map(ast::PathExpr::cast))
                    .filter_map(|p| p.path()?.as_single_name_ref().map(|n| n.text().to_string()))
                    .filter_map(|n| index.get(&n).copied())
                    .collect()
            })
            .collect();
        let mut order = Vec::with_capacity(items.len());
        let mut state = vec![0u8; items.len()]; // 0 unvisited, 1 in progress, 2 done
        for start in 0..items.len() {
            let mut stack = vec![(start, 0usize)];
            while let Some((i, next)) = stack.pop() {
                if next == 0 {
                    if state[i] != 0 {
                        continue;
                    }
                    state[i] = 1;
                }
                if let Some(&d) = deps[i].get(next) {
                    stack.push((i, next + 1));
                    if state[d] == 0 {
                        stack.push((d, 0));
                    }
                } else {
                    state[i] = 2;
                    order.push(i);
                }
            }
        }
        let mut slots: Vec<Option<ConstItem>> = items.into_iter().map(Some).collect();
        for i in order {
            let Some((rust, name, body, node)) = slots[i].take() else {
                continue;
            };
            self.scopes.push(HashMap::new());
            self.ret_ty = Ty::Unit;
            self.in_async = false;
            let js = self.consts[&rust].clone();
            let value = match &body {
                Some(e) => {
                    let (f, ty) = self.emit_expr(e, None);
                    self.by_value(e, f, &ty)
                }
                None => Frag::text("undefined"),
            };
            // `static mut`: assigned later, so a `let`.
            let keyword = match ast::Static::cast(node.clone()) {
                Some(st) if st.mut_token().is_some() => "export let ",
                _ => "export const ",
            };
            let mut line = Frag::mapped(self.item_start(&node), keyword);
            line.append(Frag::mapped(self.off(name.syntax()), js))
                .push(" = ")
                .append(value)
                .push(";");
            self.line(line);
            self.scopes.pop();
        }
    }

    fn emit_fn(&mut self, f: &ast::Fn) {
        let Some(name) = f.name() else { return };
        // (Type parameters are erased; const parameters are `$N` arguments.)
        let sig = &self.fns[name.text()];
        let (js_name, ret, is_async, param_tys) = (
            sig.js.clone(),
            sig.ret.clone(),
            sig.is_async,
            sig.params.clone(),
        );
        self.ret_ty = ret;
        self.in_async = is_async;
        self.scopes.push(HashMap::new());

        let mut params = Vec::new();
        let mut destructured: Vec<(ast::Pat, String)> = Vec::new();
        if let Some(pl) = f.param_list() {
            if let Some(sp) = pl.self_param() {
                self.unsupported(sp.syntax(), "methods (`self` parameters)");
            }
            for (p, ty) in pl.params().zip(param_tys) {
                match p.pat() {
                    Some(ast::Pat::IdentPat(ip)) if ip.name().is_some() => {
                        let n = ip.name().unwrap();
                        let js = self.fresh(n.text());
                        // A `&mut i32` parameter receives a cell.
                        let access = if self.is_boxed(n.syntax()) {
                            format!("{js}[0]")
                        } else {
                            js.clone()
                        };
                        if self.owned_param(&p, &ty) {
                            self.param_drops.push((access.clone(), ty.clone()));
                        }
                        self.declare(n.text(), Some(n.syntax()), Local { js: access, ty });
                        params.push(Frag::mapped(self.off(n.syntax()), js));
                    }
                    Some(ast::Pat::WildcardPat(_)) => params.push(Frag::text(self.temp())),
                    Some(other) => {
                        let t = self.temp();
                        params.push(Frag::mapped(self.off(other.syntax()), t.clone()));
                        destructured.push((other, t));
                    }
                    None => {}
                }
            }
        }

        // `T::make()` in the body (or in a function it hands `T` to): the class for each such `T`.
        let dicts = self
            .types
            .dict_params
            .get(name.text())
            .cloned()
            .unwrap_or_default();
        for d in &dicts {
            params.push(Frag::text(format!("${d}")));
        }
        // `{:?}` of a `T` in the body: the caller's formatter for it.
        let dbgs = self
            .types
            .dbg_params
            .get(name.text())
            .cloned()
            .unwrap_or_default();
        for d in &dbgs {
            params.push(Frag::text(format!("$dbg${}", d.replace("::", "$"))));
        }
        let saved_dbgs = set_dbg_scope(dbgs);
        let saved_dicts = std::mem::replace(&mut self.dict_scope, dicts);
        let saved_consts = set_const_scope(None);
        let const_lines = self.enter_consts(f, Vec::new());
        let mut header = Frag::mapped(self.item_start(f.syntax()), "export ");
        if is_async {
            header.push("async ");
        }
        header.push("function ");
        header.append(Frag::mapped(self.off(name.syntax()), js_name));
        header
            .push("(")
            .append(Frag::join(params, ", "))
            .push(") {");
        self.line(header);
        self.out.indent += 1;
        for l in const_lines {
            self.line(Frag::text(l));
        }
        // A Netscript `main(ns)` whose program reads `std::env::args()` (clap's `parse()` does):
        // the script's name and arguments.
        if name.text() == "main"
            && self.src.contains("env::args")
            && let Some(p) = f.param_list().and_then(|l| l.params().next())
            && p.ty()
                .is_some_and(|t| t.syntax().text().to_string().contains("NS"))
            && let Some(ast::Pat::IdentPat(ip)) = p.pat()
            && let Some(n) = ip.name()
            && let Some(local) = self.lookup(n.text().to_string().as_str())
        {
            let ns = local.js.clone();
            self.line(Frag::text(format!(
                "κσ.$setArgs([{ns}.getScriptName(), ...{ns}.args.map(String)]);"
            )));
        }
        // A Netscript `main(ns)` whose program can exit (`process::exit`, clap): the exit ends
        // the script quietly instead of as an error.
        let ns_exit = if name.text() == "main"
            && (self.src.contains("env::args") || self.src.contains("process::exit"))
            && let Some(p) = f.param_list().and_then(|l| l.params().next())
            && p.ty()
                .is_some_and(|t| t.syntax().text().to_string().contains("NS"))
            && let Some(ast::Pat::IdentPat(ip)) = p.pat()
            && let Some(n) = ip.name()
            && let Some(local) = self.lookup(n.text().to_string().as_str())
        {
            let ns = local.js.clone();
            self.line(Frag::text("try {"));
            self.out.indent += 1;
            Some(ns)
        } else {
            None
        };
        // Destructured parameters (`(r, c): (usize, usize)`): bound from their temps first.
        for (pat, t) in destructured {
            let mut pc = PatCode::default();
            self.lower_pat(&pat, &t, &mut pc);
            self.emit_binds(pc.binds);
        }
        // `fn main() -> Result<(), E>`: an `Err` ends the script like rustc's `Termination`
        // (`Error: {e:?}`, a failure).
        let returns_result = f.ret_type().and_then(|r| r.ty()).is_some_and(|t| {
            t.syntax()
                .text()
                .to_string()
                .trim_start()
                .starts_with("Result")
        });
        let main_result = match (name.text() == "main" && returns_result, &self.ret_ty) {
            (false, _) => None,
            (true, Ty::Result(_, e)) => Some(debug_fn(e).unwrap_or_else(|| "κσ.$dbg".into())),
            (true, _) => Some("κσ.$dbg".into()),
        };
        if main_result.is_some() {
            let (a, w) = if is_async {
                ("async ", "await ")
            } else {
                ("", "")
            };
            self.line(Frag::text(format!("const $main = {w}({a}() => {{")));
            self.out.indent += 1;
        }
        if let Some(body) = f.body() {
            self.emit_block_body(&body, &Dest::Return, None);
        }
        if let Some(dbg) = main_result {
            self.out.indent -= 1;
            self.line(Frag::text("})();"));
            self.line(Frag::text(format!("κσ.$mainResult($main, {dbg});")));
        }
        if let Some(ns) = ns_exit {
            self.out.indent -= 1;
            self.line(Frag::text(format!(
                "}} catch ($e) {{ if (κσ.$nsExit({ns}, $e)) return; throw $e; }}"
            )));
        }
        self.out.indent -= 1;
        self.close_line("}", f.body().as_ref());
        self.line(Frag::default());
        self.scopes.pop();
        self.dict_scope = saved_dicts;
        set_const_scope(Some(saved_consts));
        set_dbg_scope(saved_dbgs);
    }

    // ---------------------------------------------------------------- statements

    fn emit_block_body(
        &mut self,
        block: &ast::BlockExpr,
        dest: &Dest,
        expected: Option<&Ty>,
    ) -> Ty {
        // (`unsafe { .. }` and `const { .. }` are ordinary blocks in JS: a const block's value is
        // computed when it's reached rather than at compile time, which only moves the work; an
        // `async` one's body is emitted here by `emit_async_block`.)
        let async_body = std::mem::take(&mut self.async_body);
        if let Some(m) = block.modifier()
            && !matches!(
                m,
                ast::BlockModifier::Label(_)
                    | ast::BlockModifier::Unsafe(_)
                    | ast::BlockModifier::Const(_)
            )
            && !(async_body && matches!(m, ast::BlockModifier::Async(_)))
        {
            let what = match m {
                ast::BlockModifier::Async(_) => "`async` blocks",
                ast::BlockModifier::Unsafe(_) => "`unsafe` blocks",
                _ => "this block kind",
            };
            self.unsupported(block.syntax(), what);
            return Ty::Unknown;
        }
        let Some(list) = block.stmt_list() else {
            return Ty::Unit;
        };
        self.scopes.push(HashMap::new());
        // Values with a `Drop` impl declared here (and the function's by-value parameters) are
        // dropped when the block ends, however it ends, in reverse order: each `let` adds a
        // closure to a list that `finally` runs backwards.
        let params = std::mem::take(&mut self.param_drops);
        let drops_here = !params.is_empty()
            || list.statements().any(|s| match s {
                ast::Stmt::LetStmt(l) => self.let_drops(&l).is_some(),
                _ => false,
            });
        let list_js = drops_here.then(|| {
            let dk = self.temp();
            self.line(Frag::text(format!("const {dk} = [];")));
            self.line(Frag::text("try {"));
            self.out.indent += 1;
            for (js, _) in &params {
                self.track_drop(&dk, js);
            }
            dk
        });
        let mut diverged = false;
        for stmt in list.statements() {
            let saved_stmt_drops = std::mem::take(&mut self.stmt_drops);
            diverged |= self.emit_stmt(&stmt);
            // (Closures handed to std methods in it: dropped now, as Rust drops the adapters.)
            for (flag, js) in std::mem::replace(&mut self.stmt_drops, saved_stmt_drops) {
                self.line(Frag::text(format!(
                    "if ({flag}) {{ {flag} = false; κσ.$drop({js}); }}"
                )));
            }
            if let (Some(dk), ast::Stmt::LetStmt(l)) = (&list_js, &stmt)
                && let Some(name) = self.let_drops(l)
                && let Some(local) = self.lookup(name.text()).cloned()
            {
                self.track_drop(dk, &local.js);
            }
        }
        let ty = match list.tail_expr() {
            Some(tail) => self.emit_into(&tail, dest, expected),
            None if diverged => Ty::Never,
            None => Ty::Unit,
        };
        if let Some(dk) = list_js {
            self.out.indent -= 1;
            self.line(Frag::text("} finally {"));
            self.line(Frag::text(format!("  κσ.$dropAll({dk});")));
            self.line(Frag::text("}"));
        }
        self.scopes.pop();
        ty
    }

    /// A by-value parameter whose value needs dropping when the function ends.
    fn owned_param(&self, p: &ast::Param, ty: &Ty) -> bool {
        // (A closure parameter, `f: impl Fn()` / `f: F`, by the checker's type.)
        let closure = match p.pat() {
            Some(ast::Pat::IdentPat(ip)) => ip
                .name()
                .and_then(|n| self.ty_of(n.syntax()))
                .filter(|t| matches!(t, Ty::Closure { .. })),
            _ => None,
        };
        p.ty().is_some_and(|t| {
            !t.syntax().text().contains_char('&')
                && (!matches!(t, ast::Type::ImplTraitType(_)) || closure.is_some())
        }) && (self.needs_drop(ty) || closure.is_some_and(|c| self.needs_drop(&c)))
    }

    /// A closure, owning the values with a `Drop` impl it captures by value (all it names, for
    /// `move`; those its body moves, otherwise): their old scope no longer drops them, and
    /// dropping the closure does (`κσ.$own` keeps them on the function).
    fn emit_owning_closure(&mut self, c: &ast::ClosureExpr) -> Frag {
        let owned = self.owned_captures(c);
        let f = self.emit_closure(c);
        if owned.is_empty() {
            return f;
        }
        let mut out = Frag::text("(");
        for (_, flag) in &owned {
            out.push(format!("{flag} = false, "));
        }
        let caps: Vec<&str> = owned.iter().map(|(js, _)| js.as_str()).collect();
        out.push("κσ.$own(")
            .append(f)
            .push(format!(", [{}]))", caps.join(", ")));
        out
    }

    /// The drop-tracked locals (JS name, drop flag) that closure `c` takes ownership of.
    /// Whether the field `fe` reads is `Copy`: a number, `bool`, `char`, or declared as a
    /// reference (`&'static str`, which is a `String` once references are erased).
    fn copy_field(&self, fe: &ast::FieldExpr) -> bool {
        use ra_ap_syntax::ast::HasName;
        match self.ty_of(fe.syntax()) {
            Some(Ty::Int(_) | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Unit) => return true,
            Some(Ty::Str) => {}
            _ => return false,
        }
        let (Some(base), Some(name)) = (fe.expr(), fe.name_ref()) else {
            return false;
        };
        let Some(Ty::Adt(adt, _)) = self.ty_of(base.syntax()) else {
            return false;
        };
        let field = name.text().to_string();
        let Some(root) = fe.syntax().ancestors().last() else {
            return false;
        };
        root.descendants()
            .filter_map(ast::Struct::cast)
            .filter(|st| st.name().is_some_and(|n| n.text() == adt.as_str()))
            .any(|st| match st.field_list() {
                Some(ast::FieldList::RecordFieldList(l)) => l.fields().any(|f| {
                    f.name().is_some_and(|n| n.text() == field.as_str())
                        && matches!(f.ty(), Some(ast::Type::RefType(_)))
                }),
                Some(ast::FieldList::TupleFieldList(l)) => field
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| l.fields().nth(i))
                    .is_some_and(|f| matches!(f.ty(), Some(ast::Type::RefType(_)))),
                None => false,
            })
    }

    fn owned_captures(&self, c: &ast::ClosureExpr) -> Vec<(String, String)> {
        let Some(body) = c.body() else {
            return Vec::new();
        };
        self.owned_captures_in(c.syntax(), body.syntax(), c.move_token().is_some())
    }

    /// `owned_captures` of a closure or `async` block `whole` (its body `body`).
    fn owned_captures_in(
        &self,
        whole: &SyntaxNode,
        body: &SyntaxNode,
        by_move: bool,
    ) -> Vec<(String, String)> {
        // Names bound inside the closure shadow the outer ones (conservatively, anywhere in it).
        let inner: std::collections::HashSet<String> = whole
            .descendants()
            .filter_map(ast::IdentPat::cast)
            .filter_map(|ip| ip.name())
            .map(|n| n.text().to_string())
            .collect();
        let mut names: Vec<(String, bool)> = Vec::new();
        for p in body.descendants().filter_map(ast::PathExpr::cast) {
            if let Some(n) = p.path().and_then(|p| p.as_single_name_ref()) {
                let moved = self.types.move_sites.contains(&self.range_of(p.syntax()));
                // (Rust 2021 captures `tag.0` itself: a field with nothing to drop is copied
                // or borrowed, and `tag` stays with its scope.)
                // (A type with its own `Drop` can't lose a field, so then all of it is captured.)
                let own_drop = matches!(self.ty_of(p.syntax()), Some(Ty::Adt(n, _))
                    if self.types.adts.defs.get(&n).is_some_and(|d| d.implements("Drop")));
                // (A `Copy` field is copied out even then.)
                let field_only = p
                    .syntax()
                    .parent()
                    .and_then(ast::FieldExpr::cast)
                    .is_some_and(|fe| {
                        !self.ty_of(fe.syntax()).is_some_and(|t| self.needs_drop(&t))
                            && (!own_drop || self.copy_field(&fe))
                    });
                names.push((n.text().to_string(), (by_move && !field_only) || moved));
            }
        }
        // (Macro arguments are tokens: `println!("{}", g.name)`, `"{g:?}"`.)
        if by_move {
            names.extend(
                macro_arg_names(body)
                    .into_iter()
                    .map(|n| (n, true)),
            );
        }
        let mut out: Vec<(String, String)> = Vec::new();
        for (name, owns) in names {
            if !owns || inner.contains(&name) {
                continue;
            }
            let Some(local) = self.lookup(&name) else {
                continue;
            };
            let Some(flag) = self.drop_flags.get(&local.js) else {
                continue;
            };
            if !out.iter().any(|(js, _)| *js == local.js) {
                out.push((local.js.clone(), flag.clone()));
            }
        }
        out
    }

    /// `async { .. }`: an async arrow function called at once, so the promise it returns is the
    /// future. (It starts running there, where a Rust future waits for its first poll.) Like a
    /// closure, it owns the values with a `Drop` impl it captures by value, and drops them when
    /// it finishes.
    fn emit_async_block(&mut self, b: &ast::BlockExpr) -> (Frag, Ty) {
        let start = self.off(b.syntax());
        let owned = self.owned_captures_in(b.syntax(), b.syntax(), b.move_token().is_some());
        let ty = self.ty_of(b.syntax()).unwrap_or(Ty::Unknown);
        let ret = match &ty {
            Ty::Future(t) => (**t).clone(),
            _ => Ty::Unknown,
        };
        self.scopes.push(HashMap::new());
        let saved_ret = std::mem::replace(&mut self.ret_ty, ret.clone());
        let saved_async = std::mem::replace(&mut self.in_async, true);
        let saved_loops = std::mem::take(&mut self.loop_dests);
        let inner = self.capture(|cg| {
            cg.async_body = true;
            cg.emit_block_body(b, &Dest::Return, Some(&ret));
        });
        self.ret_ty = saved_ret;
        self.in_async = saved_async;
        self.loop_dests = saved_loops;
        self.scopes.pop();
        let mut f = Frag::mapped(start, "(async () => {\n");
        f.append(inner)
            .push(format!("{}}})()", "  ".repeat(self.out.indent)));
        if owned.is_empty() {
            return (f, ty);
        }
        let mut out = Frag::text("(");
        for (_, flag) in &owned {
            out.push(format!("{flag} = false, "));
        }
        let caps: Vec<&str> = owned.iter().map(|(js, _)| js.as_str()).collect();
        out.push("κσ.$ownAsync(")
            .append(f)
            .push(format!(", [{}]))", caps.join(", ")));
        (out, ty)
    }

    /// Whether a type holds a value with a `Drop` impl (in a field, element, ..).
    fn needs_drop(&self, t: &Ty) -> bool {
        fn walk(cg: &Codegen, t: &Ty, seen: &mut Vec<String>) -> bool {
            match t {
                Ty::Adt(n, args) => {
                    if seen.contains(n) {
                        return false;
                    }
                    let Some(def) = cg.types.adts.defs.get(n) else {
                        return false;
                    };
                    if def.implements("Drop") {
                        return true;
                    }
                    seen.push(n.clone());
                    let inner = args.iter().any(|a| walk(cg, a, seen))
                        || def
                            .variants
                            .iter()
                            .flat_map(|v| v.fields.iter())
                            .any(|f| walk(cg, &f.ty, seen));
                    seen.pop();
                    inner
                }
                // A closure may own captured values (`move ||`): dropping it drops them.
                Ty::Closure { .. } => true,
                other => other.inner_types().iter().any(|i| walk(cg, i, seen)),
            }
        }
        self.types.adts.defs.values().any(|d| d.implements("Drop"))
            && walk(self, &t.view(), &mut Vec::new())
    }

    /// The name a `let` binds when its value needs dropping at the end of the block.
    fn let_drops(&self, l: &ast::LetStmt) -> Option<ast::Name> {
        let Some(ast::Pat::IdentPat(ip)) = l.pat() else {
            return None;
        };
        let name = ip.name()?;
        if ip.pat().is_some() || l.let_else().is_some() || ip.ref_token().is_some() {
            return None;
        }
        // (A `let r = &x;` borrows: `x`'s owner drops it.)
        if l.initializer()
            .is_some_and(|i| matches!(strip_parens_expr(&i), ast::Expr::RefExpr(_)))
            || l.ty().is_some_and(|t| t.syntax().text().contains_char('&'))
        {
            return None;
        }
        let t = self.ty_of(name.syntax())?;
        self.needs_drop(&t).then_some(name)
    }

    /// A local (by its JS name) that now owns a value to drop: its flag, and the closure.
    fn track_drop(&mut self, list: &str, js: &str) {
        let flag = self.temp();
        self.line(Frag::text(format!("let {flag} = true;")));
        self.line(Frag::text(format!(
            "{list}.push(() => {flag} && κσ.$drop({js}));"
        )));
        self.drop_flags.insert(js.to_string(), flag);
    }

    /// Whether `e` makes a new value with a `Drop` impl that nothing keeps (a call's result, a
    /// struct literal), so a statement or `let _ =` drops it at once.
    fn drops_now(&self, e: &ast::Expr) -> bool {
        matches!(
            strip_parens_expr(e),
            ast::Expr::CallExpr(_) | ast::Expr::MethodCallExpr(_) | ast::Expr::RecordExpr(_)
        ) && self.ty_of(e.syntax()).is_some_and(|t| self.needs_drop(&t))
    }

    /// Returns true if the statement always diverges (return/break/continue/panic).
    fn emit_stmt(&mut self, stmt: &ast::Stmt) -> bool {
        match stmt {
            ast::Stmt::LetStmt(l) => {
                self.emit_let(l);
                false
            }
            ast::Stmt::ExprStmt(es) => match es.expr() {
                // `g = Guard::new(..)`: the old value is dropped first.
                Some(ast::Expr::BinExpr(b))
                    if matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { op: None }))
                        && let Some(ast::Expr::PathExpr(p)) = b.lhs()
                        && let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
                        && let Some(local) = self.lookup(n.text()).cloned()
                        && let Some(flag) = self.drop_flags.get(&local.js).cloned() =>
                {
                    self.line(Frag::text(format!("if ({flag}) κσ.$drop({});", local.js)));
                    let t = self.emit_into(&ast::Expr::BinExpr(b), &Dest::Discard, None);
                    self.line(Frag::text(format!("{flag} = true;")));
                    t == Ty::Never
                }
                // `make_guard();`: a value nothing keeps is dropped at once.
                Some(e) if self.drops_now(&e) => {
                    let (f, _) = self.emit_expr(&e, None);
                    self.line(f.wrap("κσ.$drop(", ");"));
                    false
                }
                Some(e) => self.emit_into(&e, &Dest::Discard, None) == Ty::Never,
                None => false,
            },
            // Nested items are emitted at the top level (see `adt::all_items`).
            ast::Stmt::Item(_) => false,
        }
    }

    fn emit_let(&mut self, l: &ast::LetStmt) {
        let simple = match l.pat() {
            Some(ast::Pat::IdentPat(ip)) => ip.pat().is_none() && !is_none_binding(&ip),
            Some(ast::Pat::WildcardPat(_)) | None => true,
            Some(_) => false,
        };
        if l.let_else().is_some() || !simple {
            return self.emit_pattern_let(l);
        }
        let declared = l.ty().map(|t| self.lower_type(&t));
        let (name, mutable) = match l.pat() {
            Some(ast::Pat::IdentPat(ip)) if ip.pat().is_none() && ip.name().is_some() => {
                (Some(ip.name().unwrap()), ip.mut_token().is_some())
            }
            Some(ast::Pat::WildcardPat(_)) => (None, false),
            Some(other) => {
                self.unsupported(other.syntax(), "destructuring `let` patterns");
                return;
            }
            None => return,
        };
        let start = self.off(l.syntax());

        let Some(init) = l.initializer() else {
            let Some(name) = name else { return };
            let js = self.fresh(name.text());
            self.line(Frag::mapped(start, format!("let {js};")));
            let ty = declared.unwrap_or(Ty::IntLit);
            self.declare(name.text(), Some(name.syntax()), Local { js, ty });
            return;
        };

        let Some(name) = name else {
            // `let _ = expr;` evaluates and drops (a value with a `Drop` impl, at once).
            if self.drops_now(&init) {
                let (f, _) = self.emit_expr(&init, declared.as_ref());
                self.line(f.wrap("κσ.$drop(", ");"));
                return;
            }
            self.emit_into(&init, &Dest::Discard, declared.as_ref());
            return;
        };
        // `let p = &mut x as *mut i32;`: `p` holds x's cell (or `null`).
        if self
            .types
            .ptrs
            .cells
            .contains(&self.range_of(name.syntax()))
        {
            let (f, ty) = self.emit_expr(&init, declared.as_ref());
            let js = self.fresh(name.text());
            let kw = if mutable { "let" } else { "const" };
            let mut line = Frag::mapped(start, format!("{kw} "));
            line.append(Frag::mapped(self.off(name.syntax()), js.clone()))
                .push(" = ")
                .append(f)
                .push(";");
            self.line(line);
            let ty = declared.unwrap_or(ty);
            self.declare(
                name.text(),
                Some(name.syntax()),
                Local {
                    js: format!("{js}[0]"),
                    ty,
                },
            );
            return;
        }
        // `let r = opt.get_or_insert(0);`: `r` aliases the option, so `*r += 1` sticks.
        if matches!(&init, ast::Expr::MethodCallExpr(_))
            && let Some((fill, place)) = self.option_place(&init)
        {
            self.line(Frag::mapped(start, "").append(fill).push(";").clone());
            let ty = self.ty_of(init.syntax()).unwrap_or(Ty::Unknown);
            let local = Local {
                js: place.plain(),
                ty,
            };
            self.declare(name.text(), Some(name.syntax()), local);
            return;
        }
        // `let v = map.entry(k).or_insert(0);`: `v` aliases the slot's value, so `*v += 1` sticks.
        if let ast::Expr::MethodCallExpr(m) = &init
            && let Some(slot) = self.entry_slot(m)
        {
            let s = self.temp();
            self.line(
                Frag::mapped(start, format!("const {s} = "))
                    .append(slot)
                    .push(";")
                    .clone(),
            );
            let local = Local {
                js: format!("{s}[1]"),
                ty: Ty::Unknown,
            };
            self.declare(name.text(), Some(name.syntax()), local);
            return;
        }
        // `let b = cell.borrow_mut();`: `b` aliases the cell's value.
        if let ast::Expr::MethodCallExpr(m) = strip_parens_expr(&init)
            && m.name_ref().is_some_and(|n| n.text() == "borrow_mut")
            && let Some(recv) = m.receiver()
            && matches!(self.ty_of(recv.syntax()), Some(Ty::RefCell(_)))
        {
            let (rf, _) = self.emit_expr(&recv, None);
            let js = self.fresh(name.text());
            let mut f = Frag::mapped(start, "const ");
            f.append(Frag::mapped(self.off(name.syntax()), js.clone()))
                .push(" = ")
                .append(rf)
                .push(";");
            self.line(f);
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            self.declare(
                name.text(),
                Some(name.syntax()),
                Local {
                    js: format!("{js}.v"),
                    ty,
                },
            );
            return;
        }
        // `let mut g = m.lock().unwrap();`: `g` aliases the lock's value.
        if let Some(lock) = self.lock_place(&init) {
            let (mut rf, _) = self.emit_expr(&lock, None);
            rf.push(".__c");
            let js = self.fresh(name.text());
            let mut f = Frag::mapped(start, "const ");
            f.append(Frag::mapped(self.off(name.syntax()), js.clone()))
                .push(" = ")
                .append(rf)
                .push(";");
            self.line(f);
            let ty = self.ty_of(init.syntax()).unwrap_or(Ty::Unknown);
            self.declare(
                name.text(),
                Some(name.syntax()),
                Local {
                    js: format!("{js}.v"),
                    ty,
                },
            );
            return;
        }
        // `let r = &mut n;` with `n` in a cell: `r` holds the same cell.
        if let ast::Expr::RefExpr(r) = strip_parens_expr(&init)
            && r.mut_token().is_some()
            && let Some(ast::Expr::PathExpr(p)) = r.expr()
            && let Some(local) = p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()).cloned())
            && let Some(cell) = local.js.strip_suffix("[0]").map(str::to_string)
        {
            let js = self.fresh(name.text());
            // (`let mut q = &mut n;`: `q = &mut other;` later re-points it.)
            let kw = if mutable { "let" } else { "const" };
            let mut f = Frag::mapped(start, format!("{kw} "));
            f.append(Frag::mapped(self.off(name.syntax()), js.clone()))
                .push(format!(" = {cell};"));
            self.line(f);
            let local = Local {
                js: format!("{js}[0]"),
                ty: local.ty,
            };
            self.declare(name.text(), Some(name.syntax()), local);
            return;
        }
        let block_like = matches!(init, ast::Expr::BlockExpr(_) | ast::Expr::LoopExpr(_))
            || matches!(&init, ast::Expr::IfExpr(i) if !if_is_ternary(i));
        let (js, ty) = if block_like {
            let js = self.fresh(name.text());
            self.line(Frag::mapped(start, format!("let {js};")));
            let ty = self.emit_into(&init, &Dest::Assign(js.clone()), declared.as_ref());
            (js, ty)
        } else {
            let (frag, ty) = self.emit_expr(&init, declared.as_ref());
            let frag = self.by_value(&init, frag, &ty);
            // Declare after lowering the initializer: `let x = x + 1` refers to the outer `x`.
            let js = self.fresh(name.text());
            let kw = if mutable || self.is_boxed(name.syntax()) {
                "let"
            } else {
                "const"
            };
            let mut f = Frag::mapped(start, format!("{kw} "));
            f.append(Frag::mapped(self.off(name.syntax()), js.clone()));
            f.push(" = ").append(frag).push(";");
            self.line(f);
            (js, ty)
        };
        let ty = declared.unwrap_or(ty);
        // A `&mut` to this binding is taken somewhere: it lives in a cell, `[value]`.
        let js = if self.is_boxed(name.syntax()) {
            self.line(Frag::text(format!("{js} = [{js}];")));
            format!("{js}[0]")
        } else {
            js
        };
        self.declare(name.text(), Some(name.syntax()), Local { js, ty });
    }

    /// Lower an expression whose value goes to `dest`. Block-like expressions become statements.
    fn emit_into(&mut self, e: &ast::Expr, dest: &Dest, expected: Option<&Ty>) -> Ty {
        let start = self.off(e.syntax());
        match e {
            ast::Expr::BlockExpr(b)
                if matches!(b.modifier(), Some(ast::BlockModifier::Label(_))) =>
            {
                // `'a: { .. }` → `$a: { .. }`; `break 'a v` assigns `dest` and leaves the block.
                let rust = b
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                let label = self.label_prefix(b.label());
                self.line(Frag::mapped(start, format!("{label}{{")));
                self.out.indent += 1;
                self.loop_dests.push((rust, Some(dest.clone()), true));
                let ty = self.emit_block_body(b, dest, expected);
                self.loop_dests.pop();
                self.out.indent -= 1;
                self.close_line("}", Some(b));
                self.ty_of(b.syntax()).unwrap_or(ty)
            }
            ast::Expr::BlockExpr(b)
                if !matches!(b.modifier(), Some(ast::BlockModifier::Async(_))) =>
            {
                self.line(Frag::mapped(start, "{"));
                self.out.indent += 1;
                let ty = self.emit_block_body(b, dest, expected);
                self.out.indent -= 1;
                self.close_line("}", Some(b));
                ty
            }
            ast::Expr::IfExpr(i) => self.emit_if_stmt(i, dest, expected, false),
            ast::Expr::MatchExpr(m) => self.emit_match(m, dest, expected),
            ast::Expr::WhileExpr(w) => {
                let label = w
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.loop_dests.push((label, None, false));
                self.emit_while(w);
                self.loop_dests.pop();
                Ty::Unit
            }
            ast::Expr::LoopExpr(l) => {
                let label = l
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.loop_dests.push((label, Some(dest.clone()), false));
                self.emit_loop(l);
                self.loop_dests.pop();
                self.ty_of(l.syntax()).unwrap_or(Ty::Unit)
            }
            ast::Expr::ForExpr(f) => {
                let label = f
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.loop_dests.push((label, None, false));
                self.emit_for(f);
                self.loop_dests.pop();
                Ty::Unit
            }
            ast::Expr::ReturnExpr(_) | ast::Expr::BreakExpr(_) | ast::Expr::ContinueExpr(_) => {
                self.emit_jump(e);
                Ty::Never
            }
            _ => {
                let (frag, ty) = self.emit_expr(e, expected);
                // A block's value is moved out by value: copy tuples read from places.
                let frag = self.by_value(e, frag, &ty);
                let returns_value = !matches!(self.ret_ty, Ty::Unit | Ty::Never);
                let stmt = match dest {
                    Dest::Assign(t) if ty != Ty::Unit && ty != Ty::Never => {
                        let mut f = Frag::mapped(start, format!("{t} = "));
                        f.append(frag).push(";");
                        f
                    }
                    Dest::Return if returns_value && ty != Ty::Never => {
                        let mut f = Frag::mapped(start, "return ");
                        f.append(frag).push(";");
                        f
                    }
                    _ => {
                        if frag.plain() == "undefined" {
                            return ty;
                        }
                        let mut f = frag.at(start);
                        f.push(";");
                        f
                    }
                };
                self.line(stmt);
                ty
            }
        }
    }

    fn emit_jump(&mut self, e: &ast::Expr) {
        let start = self.off(e.syntax());
        match e {
            ast::Expr::ReturnExpr(r) => match r.expr() {
                Some(v) => {
                    let ret = self.ret_ty.clone();
                    let (frag, ty) = self.emit_expr(&v, Some(&ret));
                    let frag = self.by_value(&v, frag, &ty);
                    let mut f = Frag::mapped(start, if ty == Ty::Unit { "" } else { "return " });
                    f.append(frag).push(";");
                    self.line(f);
                    if ty == Ty::Unit {
                        self.line(Frag::mapped(start, "return;"));
                    }
                }
                None => self.line(Frag::mapped(start, "return;")),
            },
            ast::Expr::BreakExpr(b) => {
                if let Some(v) = b.expr() {
                    // The value goes where the target `loop`'s value goes.
                    let rust_label = b.lifetime().map(|l| l.text().to_string());
                    let target = match &rust_label {
                        Some(l) => self
                            .loop_dests
                            .iter()
                            .rev()
                            .find(|(x, ..)| x.as_ref() == Some(l)),
                        None => self.loop_dests.iter().rev().find(|e| !e.2),
                    };
                    let dest = target
                        .and_then(|(_, d, _)| d.clone())
                        .unwrap_or(Dest::Discard);
                    self.emit_into(&v, &dest, None);
                    if matches!(dest, Dest::Return) {
                        return;
                    }
                }
                let label = b
                    .lifetime()
                    .map(|l| format!(" ${}", &l.text()[1..]))
                    .unwrap_or_default();
                self.line(Frag::mapped(start, format!("break{label};")));
            }
            ast::Expr::ContinueExpr(c) => {
                let label = c
                    .lifetime()
                    .map(|l| format!(" ${}", &l.text()[1..]))
                    .unwrap_or_default();
                self.line(Frag::mapped(start, format!("continue{label};")));
            }
            _ => unreachable!(),
        }
    }

    fn emit_if_stmt(
        &mut self,
        i: &ast::IfExpr,
        dest: &Dest,
        expected: Option<&Ty>,
        is_else_if: bool,
    ) -> Ty {
        let start = self.off(i.syntax());
        let Some(cond) = i.condition() else {
            return Ty::Unknown;
        };
        if crate::typeck::is_let_chain(&cond) {
            return self.emit_if_chain(i, &cond, dest, expected, is_else_if);
        }
        // `if let PAT = EXPR`: test the pattern, then bind inside the `then` block.
        let (cf, binds) = match &cond {
            ast::Expr::LetExpr(le) => {
                let (Some(e), Some(p)) = (le.expr(), le.pat()) else {
                    return Ty::Unknown;
                };
                let subject = self.subject(&e);
                let mut pc = PatCode::default();
                self.lower_pat(&p, &subject, &mut pc);
                (
                    pc.condition().unwrap_or_else(|| Frag::text("true")),
                    pc.binds,
                )
            }
            _ => (self.emit_expr(&cond, Some(&Ty::Bool)).0, Vec::new()),
        };
        let mut head = Frag::mapped(start, if is_else_if { "} else if (" } else { "if (" });
        head.append(cf).push(") {");
        self.line(head);
        self.out.indent += 1;
        self.scopes.push(HashMap::new());
        self.emit_binds(binds);
        let then_branch = i.then_branch();
        let then_ty = match &then_branch {
            Some(b) => self.emit_block_body(b, dest, expected),
            None => Ty::Unit,
        };
        self.scopes.pop();
        self.out.indent -= 1;
        let else_ty = match i.else_branch() {
            Some(ElseBranch::IfExpr(ei)) => {
                // `else if let` on an expression that needs a temporary also has to nest.
                let cond_hoists = ei.condition().is_some_and(|c| {
                    needs_hoist(c.syntax())
                        || matches!(&c, ast::Expr::LetExpr(le) if !le.expr().is_some_and(|e| is_pure(&e)))
                });
                if cond_hoists {
                    // Hoisted statements must only run when this branch is reached.
                    self.close_line("} else {", then_branch.as_ref());
                    self.out.indent += 1;
                    let t = self.emit_if_stmt(&ei, dest, expected, false);
                    self.out.indent -= 1;
                    self.line(Frag::text("}"));
                    return Ty::unify(&then_ty, &t);
                }
                return Ty::unify(&then_ty, &self.emit_if_stmt(&ei, dest, expected, true));
            }
            Some(ElseBranch::Block(b)) => {
                self.close_line("} else {", then_branch.as_ref());
                self.out.indent += 1;
                let t = self.emit_block_body(&b, dest, expected);
                self.out.indent -= 1;
                self.close_line("}", Some(&b));
                return Ty::unify(&then_ty, &t);
            }
            None => Ty::Unit,
        };
        self.close_line("}", then_branch.as_ref());
        Ty::unify(&then_ty, &else_ty)
    }

    /// Open one nested `if` per part of a let-chain; returns how many were opened. Each part's
    /// statements run only once the parts before it held.
    fn open_chain(&mut self, cond: &ast::Expr) -> usize {
        let mut opened = 0;
        for part in crate::typeck::chain_parts(cond) {
            let (cf, binds) = match &part {
                ast::Expr::LetExpr(le) => {
                    let (Some(e), Some(p)) = (le.expr(), le.pat()) else {
                        continue;
                    };
                    let subject = self.subject(&e);
                    let mut pc = PatCode::default();
                    self.lower_pat(&p, &subject, &mut pc);
                    (
                        pc.condition().unwrap_or_else(|| Frag::text("true")),
                        pc.binds,
                    )
                }
                other => (self.emit_expr(other, Some(&Ty::Bool)).0, Vec::new()),
            };
            let mut head = Frag::mapped(self.off(part.syntax()), "if (");
            head.append(cf).push(") {");
            self.line(head);
            self.out.indent += 1;
            self.emit_binds(binds);
            opened += 1;
        }
        opened
    }

    fn close_chain(&mut self, opened: usize) {
        for _ in 0..opened {
            self.out.indent -= 1;
            self.line(Frag::text("}"));
        }
    }

    /// `if let A = a && b && let C = c { .. } else { .. }`: nested `if`s around the `then` block,
    /// and a flag for the `else`.
    fn emit_if_chain(
        &mut self,
        i: &ast::IfExpr,
        cond: &ast::Expr,
        dest: &Dest,
        expected: Option<&Ty>,
        is_else_if: bool,
    ) -> Ty {
        if is_else_if {
            self.line(Frag::text("} else {"));
            self.out.indent += 1;
        }
        let matched = self.temp();
        let has_else = i.else_branch().is_some();
        if has_else {
            self.line(Frag::mapped(
                self.off(i.syntax()),
                format!("let {matched} = false;"),
            ));
        }
        self.scopes.push(HashMap::new());
        let opened = self.open_chain(cond);
        if has_else {
            self.line(Frag::text(format!("{matched} = true;")));
        }
        let then_ty = match i.then_branch() {
            Some(b) => self.emit_block_body(&b, dest, expected),
            None => Ty::Unit,
        };
        self.close_chain(opened);
        self.scopes.pop();
        let else_ty = match i.else_branch() {
            Some(e) => {
                self.line(Frag::text(format!("if (!{matched}) {{")));
                self.out.indent += 1;
                let t = match e {
                    ElseBranch::Block(b) => self.emit_block_body(&b, dest, expected),
                    ElseBranch::IfExpr(ei) => self.emit_if_stmt(&ei, dest, expected, false),
                };
                self.out.indent -= 1;
                self.line(Frag::text("}"));
                t
            }
            None => Ty::Unit,
        };
        if is_else_if {
            self.out.indent -= 1;
            self.line(Frag::text("}"));
        }
        Ty::unify(&then_ty, &else_ty)
    }

    fn label_prefix(&self, l: Option<ast::Label>) -> String {
        l.and_then(|l| l.lifetime())
            .map(|lt| format!("${}: ", &lt.text()[1..]))
            .unwrap_or_default()
    }

    fn emit_body(&mut self, body: Option<ast::BlockExpr>) {
        self.out.indent += 1;
        if let Some(b) = &body {
            self.emit_block_body(b, &Dest::Discard, Some(&Ty::Unit));
        }
        self.out.indent -= 1;
        self.close_line("}", body.as_ref());
    }

    /// Write a line that closes `block`, mapped to the block's `}`.
    fn close_line(&mut self, text: &str, block: Option<&ast::BlockExpr>) {
        let close = block
            .and_then(|b| b.stmt_list())
            .and_then(|l| l.r_curly_token())
            .map(|t| self.base + usize::from(t.text_range().start()));
        self.line(match close {
            Some(off) => Frag::mapped(off, text),
            None => Frag::text(text),
        });
    }

    /// Where an item really starts: rust-analyzer attaches leading comments, doc comments and
    /// attributes to the item node, so its range would begin at the first comment.
    fn item_start(&self, n: &SyntaxNode) -> usize {
        let first = n.children_with_tokens().find(|el| {
            !el.kind().is_trivia()
                && !matches!(
                    el.kind(),
                    SyntaxKind::ATTR | SyntaxKind::COMMENT | SyntaxKind::DOC_COMMENT
                )
        });
        match first {
            Some(el) => self.base + usize::from(el.text_range().start()),
            None => self.off(n),
        }
    }

    fn emit_while(&mut self, w: &ast::WhileExpr) {
        let start = self.off(w.syntax());
        let label = self.label_prefix(w.label());
        let Some(cond) = w.condition() else { return };
        if crate::typeck::is_let_chain(&cond) {
            // `while (true) { if (a) { if (b) { $m = true; body } } if (!$m) break; }`
            self.line(Frag::mapped(start, format!("{label}while (true) {{")));
            self.out.indent += 1;
            let matched = self.temp();
            self.line(Frag::text(format!("let {matched} = false;")));
            self.scopes.push(HashMap::new());
            let opened = self.open_chain(&cond);
            self.line(Frag::text(format!("{matched} = true;")));
            let body = w.loop_body();
            if let Some(b) = &body {
                self.emit_block_body(b, &Dest::Discard, Some(&Ty::Unit));
            }
            self.close_chain(opened);
            self.scopes.pop();
            self.line(Frag::text(format!("if (!{matched}) break;")));
            self.out.indent -= 1;
            self.close_line("}", body.as_ref());
            return;
        }
        if let ast::Expr::LetExpr(le) = &cond {
            // The scrutinee is re-evaluated each iteration; `continue` re-runs it too.
            let (Some(e), Some(p)) = (le.expr(), le.pat()) else {
                return;
            };
            self.line(Frag::mapped(start, format!("{label}while (true) {{")));
            self.out.indent += 1;
            self.scopes.push(HashMap::new());
            let subject = self.subject(&e);
            let mut pc = PatCode::default();
            self.lower_pat(&p, &subject, &mut pc);
            if let Some(c) = pc.condition() {
                let mut f = Frag::mapped(self.off(le.syntax()), "if (!(");
                f.append(c).push(")) break;");
                self.line(f);
            }
            self.emit_binds(pc.binds);
            let body = w.loop_body();
            if let Some(b) = &body {
                self.emit_block_body(b, &Dest::Discard, Some(&Ty::Unit));
            }
            self.scopes.pop();
            self.out.indent -= 1;
            self.close_line("}", body.as_ref());
            return;
        }
        if needs_hoist(cond.syntax()) {
            // The condition's hoisted statements must re-run every iteration.
            self.line(Frag::mapped(start, format!("{label}while (true) {{")));
            self.out.indent += 1;
            let (cf, _) = self.emit_expr(&cond, Some(&Ty::Bool));
            let mut f = Frag::mapped(self.off(cond.syntax()), "if (!(");
            f.append(cf).push(")) break;");
            self.line(f);
            self.out.indent -= 1;
        } else {
            let (cf, _) = self.emit_expr(&cond, Some(&Ty::Bool));
            let mut f = Frag::mapped(start, format!("{label}while ("));
            f.append(cf).push(") {");
            self.line(f);
        }
        self.emit_body(w.loop_body());
    }

    fn emit_loop(&mut self, l: &ast::LoopExpr) {
        let label = self.label_prefix(l.label());
        self.line(Frag::mapped(
            self.off(l.syntax()),
            format!("{label}while (true) {{"),
        ));
        self.emit_body(l.loop_body());
    }

    fn emit_for(&mut self, f: &ast::ForExpr) {
        let start = self.off(f.syntax());
        let label = self.label_prefix(f.label());
        let Some(iterable) = f.iterable() else { return };

        // Numeric `a..b`, `a..=b` and `(a..b).rev()` become counting loops; everything else
        // (char ranges, `a..`, iterator chains) iterates with `for...of`.
        let counting = |r: &ast::RangeExpr| r.start().is_some() && r.end().is_some();
        let char_range = matches!(
            self.ty_of(iterable.syntax()),
            Some(Ty::Iter { elem, .. }) if *elem == Ty::Char
        ) || {
            // The checker types a loop's range through its bounds.
            let r = match &iterable {
                ast::Expr::MethodCallExpr(m) => m.receiver().map(|r| strip_refs(&r)),
                other => Some(other.clone()),
            };
            match r {
                Some(ast::Expr::RangeExpr(r)) => r
                    .start()
                    .or(r.end())
                    .is_some_and(|e| self.ty_of(e.syntax()) == Some(Ty::Char)),
                _ => false,
            }
        };
        let (range, rev) = match &iterable {
            _ if char_range => return self.emit_for_vec(f, &iterable, &label),
            ast::Expr::RangeExpr(r) if !counting(r) => {
                return self.emit_for_vec(f, &iterable, &label);
            }
            ast::Expr::RangeExpr(r) => (r.clone(), false),
            ast::Expr::MethodCallExpr(m)
                if m.name_ref().is_some_and(|n| n.text() == "rev")
                    && m.arg_list().is_some_and(|a| a.args().next().is_none()) =>
            {
                match m.receiver().map(|r| strip_refs(&r)) {
                    Some(ast::Expr::RangeExpr(r)) if counting(&r) => (r, true),
                    _ => return self.emit_for_vec(f, &iterable, &label),
                }
            }
            _ => return self.emit_for_vec(f, &iterable, &label),
        };
        let (Some(lo), Some(hi)) = (range.start(), range.end()) else {
            self.unsupported(range.syntax(), "open-ended ranges");
            return;
        };
        let inclusive = range.op_kind() == Some(RangeOp::Inclusive);
        let (mut lo_f, lo_ty) = self.emit_expr(&lo, None);
        let (mut hi_f, hi_ty) = self.emit_expr(&hi, Some(&lo_ty));
        let ty = Ty::unify(&lo_ty, &hi_ty).default_lit();

        self.scopes.push(HashMap::new());
        let (var_name, var_node) = match f.pat() {
            Some(ast::Pat::IdentPat(ip)) if ip.name().is_some() => {
                let n = ip.name().unwrap();
                (n.text().to_string(), Some(n.syntax().clone()))
            }
            Some(ast::Pat::WildcardPat(_)) => ("_".to_string(), None),
            Some(other) => {
                self.unsupported(other.syntax(), "destructuring `for` patterns");
                self.scopes.pop();
                return;
            }
            None => {
                self.scopes.pop();
                return;
            }
        };
        let var = if var_node.is_some() {
            self.fresh(&var_name)
        } else {
            self.temp()
        };
        let is_simple = |e: &ast::Expr| matches!(e, ast::Expr::Literal(_) | ast::Expr::PathExpr(_));
        let mut init = Frag::mapped(
            var_node.as_ref().map(|n| self.off(n)).unwrap_or(start),
            format!("let {var} = "),
        );
        // The bound the loop tests against is evaluated once, like Rust's range.
        let (bound_expr, bound_frag) = if rev {
            (&lo, &mut lo_f)
        } else {
            (&hi, &mut hi_f)
        };
        if !is_simple(bound_expr) {
            let end = self.temp();
            let bf = std::mem::replace(bound_frag, Frag::text(end.clone()));
            let mut decl = Frag::text(format!(", {end} = "));
            decl.append(bf);
            if rev {
                init.append(hi_f.clone());
                if !inclusive {
                    init.push(" - 1");
                }
                init.append(decl);
            } else {
                init.append(lo_f.clone()).append(decl);
            }
        } else if rev {
            init.append(hi_f.clone());
            if !inclusive {
                init.push(" - 1");
            }
        } else {
            init.append(lo_f.clone());
        }
        let mut head = Frag::mapped(start, format!("{label}for ("));
        head.append(init);
        if rev {
            head.push(format!("; {var} >= "))
                .append(lo_f)
                .push(format!("; {var}--) {{"));
        } else {
            head.push(format!("; {var} {} ", if inclusive { "<=" } else { "<" }))
                .append(hi_f)
                .push(format!("; {var}++) {{"));
        }
        self.line(head);
        if var_node.is_some() {
            self.declare(&var_name, var_node.as_ref(), Local { js: var, ty });
        }
        self.emit_body(f.loop_body());
        self.scopes.pop();
    }

    /// Element `i` of `source` (emitted as `place`) in a loop by index: `v[i]`, or a deque's
    /// `q.b[q.$p(i)]` (its backing array at the ring index).
    pub(super) fn index_place(&self, source: &ast::Expr, place: &str, i: &str) -> String {
        if matches!(
            self.raw_ty_of(strip_refs(source).syntax()),
            Some(Ty::Coll(crate::ty::Coll::VecDeque, _))
        ) {
            format!("{place}.b[{place}.$p({i})]")
        } else {
            format!("{place}[{i}]")
        }
    }

    /// `for (x, y) in v.iter_mut().zip(ys)`: an index loop over `v` with `x` aliasing `v[i]` (so
    /// `*x = ..` writes through), and `y` from an iterator over `ys`, stopping at the shorter.
    fn emit_for_zip_mut(&mut self, f: &ast::ForExpr, iterable: &ast::Expr, label: &str) -> bool {
        let ast::Expr::MethodCallExpr(zip) = iterable else {
            return false;
        };
        if !zip.name_ref().is_some_and(|n| n.text() == "zip") {
            return false;
        }
        let Some(ast::Expr::MethodCallExpr(im)) = zip.receiver() else {
            return false;
        };
        if !im.name_ref().is_some_and(|n| n.text() == "iter_mut") {
            return false;
        }
        let (Some(source), Some(other)) =
            (im.receiver(), zip.arg_list().and_then(|l| l.args().next()))
        else {
            return false;
        };
        if !matches!(self.ty_of(source.syntax()), Some(Ty::Vec(_))) {
            return false;
        }
        let Some(ast::Pat::TuplePat(tp)) = f.pat() else {
            return false;
        };
        let fields: Vec<ast::Pat> = tp.fields().collect();
        if fields.len() != 2 {
            return false;
        }
        let start = self.off(f.syntax());
        let (sf, _) = self.emit_expr(&source, None);
        let place = if is_pure(&source) {
            sf.plain()
        } else {
            let t = self.temp();
            let mut decl = Frag::mapped(self.off(source.syntax()), format!("const {t} = "));
            decl.append(sf).push(";");
            self.line(decl);
            t
        };
        let it = self.temp();
        let ys = self.iterable_arg(Some(other));
        let mut decl = Frag::text(format!("const {it} = ("));
        decl.append(ys).push(")[Symbol.iterator]();");
        self.line(decl);
        let i = self.temp();
        let n = self.temp();
        self.scopes.push(HashMap::new());
        self.line(Frag::mapped(
            start,
            format!("{label}for (let {i} = 0; {i} < {place}.length; {i}++) {{"),
        ));
        self.out.indent += 1;
        self.line(Frag::text(format!("const {n} = {it}.next();")));
        self.line(Frag::text(format!("if ({n}.done) break;")));
        let mut pc = PatCode::default();
        let element = self.index_place(&source, &place, &i);
        self.lower_pat(&fields[0], &element, &mut pc);
        self.alias_binds(pc.binds);
        let mut pc = PatCode::default();
        self.lower_pat(&fields[1], &format!("{n}.value"), &mut pc);
        self.emit_binds(pc.binds);
        self.out.indent -= 1;
        self.emit_body(f.loop_body());
        self.scopes.pop();
        true
    }

    /// `for x in v` / `v.iter()` / `v.into_iter()` (Vec or HashSet) become `for...of`.
    /// `for x in &mut v` / `v.iter_mut()` iterate by index with `x` aliasing `v[i]`, so `*x = ...`
    /// writes through. `.enumerate()` iterates by index and binds `(i, element)`. Any irrefutable
    /// pattern can be the loop variable.
    fn emit_for_vec(&mut self, f: &ast::ForExpr, iterable: &ast::Expr, label: &str) {
        let start = self.off(f.syntax());
        let is_call = |e: &ast::Expr, names: &[&str]| match e {
            ast::Expr::MethodCallExpr(m) => {
                m.name_ref().is_some_and(|n| names.contains(&n.text()))
                    && m.arg_list().is_some_and(|a| a.args().next().is_none())
            }
            _ => false,
        };
        let receiver = |e: &ast::Expr| match e {
            ast::Expr::MethodCallExpr(m) => m.receiver(),
            _ => None,
        };
        // Peel `.enumerate()`, then `&`, `&mut` and the iterator methods down to the collection.
        if let Some((map, part, mutable)) = self.map_iteration(iterable) {
            return self.emit_for_map(f, &map, part, mutable, label);
        }
        if self.emit_for_zip_mut(f, iterable, label) {
            return;
        }
        let enumerate = is_call(iterable, &["enumerate"]);
        let inner = if enumerate {
            receiver(iterable)
        } else {
            Some(iterable.clone())
        };
        let Some(inner) = inner else { return };
        let (source, mut by_mut_ref) = match &inner {
            ast::Expr::RefExpr(r) => (r.expr(), r.mut_token().is_some()),
            e if is_call(e, &["iter", "iter_mut", "into_iter"]) && !self.own_iter_call(e) => {
                let mutable = matches!(e, ast::Expr::MethodCallExpr(m) if m.name_ref().is_some_and(|n| n.text() == "iter_mut"));
                (receiver(e), mutable)
            }
            other => (
                Some(other.clone()),
                self.types.mut_ref_loops.contains(&self.range_of(other.syntax())),
            ),
        };
        let Some(source) = source else { return };
        // Iterators (and enumerating anything but a Vec) loop over the whole expression.
        let mut enumerate = enumerate;
        let source_ty = self.ty_of(source.syntax()).unwrap_or(Ty::Unknown);
        // A custom iterator (`impl Iterator for Fib`) loops like any other iterator.
        let source_ty = match source_ty {
            Ty::Adt(n, _) if self.types.adts.iter_items.contains_key(&n) => Ty::Iter {
                elem: Box::new(self.types.adts.iter_items[&n].clone()),
                mutable: false,
            },
            // (An `IntoIterator` parameter is the runtime's iterator by now.)
            Ty::IntoIter(e) => Ty::Iter {
                elem: e,
                mutable: false,
            },
            other => other,
        };
        let source = match source_ty {
            Ty::Iter { mutable: false, .. } | Ty::Set(_)
                if !by_mut_ref && (enumerate || matches!(source_ty, Ty::Iter { .. })) =>
            {
                enumerate = false;
                by_mut_ref = false;
                iterable.clone()
            }
            _ => source,
        };
        let (sf, st) = self.emit_expr(&source, None);
        let custom_iter =
            matches!(&st, Ty::Adt(n, _) if self.types.adts.iter_items.contains_key(n));
        // An `iter_mut()` chain over JS objects loops over the objects themselves.
        let object_iter = matches!(&st, Ty::Iter { mutable: true, elem } if !elem.is_js_primitive() && **elem != Ty::Unknown);
        // A slot iterator (`v.iter_mut().filter(..)` of numbers or strings): the loop's names
        // were bound as slots, which read and write the elements.
        let slot_iter = matches!(&st, Ty::Iter { mutable: true, .. })
            && f.pat().is_some_and(|p| {
                p.syntax()
                    .descendants()
                    .filter_map(ast::Name::cast)
                    .any(|n| {
                        self.types
                            .slot_bindings
                            .contains(&self.range_of(n.syntax()))
                    })
            });
        if !custom_iter
            && !object_iter
            && !slot_iter
            && !matches!(
                st,
                Ty::Vec(_)
                    | Ty::Set(_)
                    | Ty::Iter { mutable: false, .. }
                    | Ty::IntoIter(_)
                    | Ty::Unknown
            )
        {
            self.unsupported(iterable.syntax(), "`for` over this iterator");
            return;
        }
        let Some(pat) = f.pat() else { return };
        self.scopes.push(HashMap::new());
        if by_mut_ref || enumerate {
            // Index loop. The collection needs a name that's safe to repeat.
            let place = if is_pure(&source) {
                sf.plain()
            } else {
                let t = self.temp();
                let mut decl = Frag::mapped(self.off(source.syntax()), format!("const {t} = "));
                decl.append(sf).push(";");
                self.line(decl);
                t
            };
            let i = self.temp();
            self.line(Frag::mapped(
                start,
                format!("{label}for (let {i} = 0; {i} < {place}.length; {i}++) {{"),
            ));
            self.out.indent += 1;
            let element = self.index_place(&source, &place, &i);
            let mut pc = PatCode::default();
            match (&pat, enumerate) {
                // `(i, x)`: bind the index and the element directly, without building a pair.
                (ast::Pat::TuplePat(tp), true) if tp.fields().count() == 2 => {
                    let fields: Vec<ast::Pat> = tp.fields().collect();
                    self.lower_pat(&fields[0], &i, &mut pc);
                    self.lower_pat(&fields[1], &element, &mut pc);
                }
                (_, true) => self.lower_pat(&pat, &format!("[{i}, {element}]"), &mut pc),
                (_, false) => self.lower_pat(&pat, &element, &mut pc),
            }
            if by_mut_ref {
                // Element bindings alias the Vec's slots; the index stays a copy.
                let (index_binds, element_binds): (Vec<_>, Vec<_>) =
                    pc.binds.into_iter().partition(|(_, _, js)| *js == i);
                self.emit_binds(index_binds);
                self.alias_binds(element_binds);
            } else {
                self.emit_binds(pc.binds);
            }
            self.out.indent -= 1;
        } else {
            let simple = match &pat {
                ast::Pat::IdentPat(ip) if ip.pat().is_none() => {
                    ip.name().map(|n| (n, ip.mut_token().is_some()))
                }
                _ => None,
            };
            let (var, var_off) = match &simple {
                Some((n, _)) => (self.fresh(n.text()), self.off(n.syntax())),
                None => (self.temp(), start),
            };
            let kw = if simple.as_ref().is_some_and(|(_, m)| *m) {
                "let"
            } else {
                "const"
            };
            let mut head = Frag::mapped(start, format!("{label}for ("));
            head.append(Frag::mapped(var_off, format!("{kw} {var}")))
                .push(" of ")
                .append(sf)
                .push(") {");
            self.line(head);
            match simple {
                Some((n, _)) => self.declare(
                    n.text(),
                    Some(n.syntax()),
                    Local {
                        js: var,
                        ty: Ty::Unknown,
                    },
                ),
                None => {
                    // Destructuring: bind the pattern against the element.
                    self.out.indent += 1;
                    let mut pc = PatCode::default();
                    self.lower_pat(&pat, &var, &mut pc);
                    self.emit_binds(pc.binds);
                    self.out.indent -= 1;
                }
            }
        }
        self.emit_body(f.loop_body());
        self.scopes.pop();
    }

    // ---------------------------------------------------------------- expressions

    /// Lower `e` to a JS expression. May write hoisted statements first.
    fn emit_expr(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> (Frag, Ty) {
        let start = self.off(e.syntax());
        let (frag, ty) = self.emit_expr_inner(e, expected);
        let ty = self.ty_of(e.syntax()).unwrap_or(ty);
        let r = self.range_of(e.syntax());
        // A slot-valued argument (`bump(s.at(0))`) to a `&mut i32` parameter: as a cell.
        if self.types.slot_cell_args.contains(&r) {
            return (frag.wrap("κσ.$slotCell(", ")").at(start), ty);
        }
        // `{:?}` of a Vec of slots or an item of one: a copy with the slots read.
        if let Some(shape) = self.types.slot_reads.get(&r) {
            let f = frag.wrap("κσ.$readSlots(", &format!(", {shape})"));
            return (f.at(start), ty);
        }
        // A slot-valued expression (`s.at(0)` of your `-> &mut i32`, `w.n` of `n: &mut i32`,
        // `o.unwrap()` of an `Option<&mut i32>`) read as its value, unless the slot is kept.
        if self.types.slot_values.contains(&r) && !self.types.slot_keeps.contains(&r) {
            return (frag.wrap("(", ")[1]").at(start), ty);
        }
        // A generic `-> &mut T` / `-> Option<&mut T>` returns a slot, whatever `T` is: with a
        // struct or collection for `T`, the object it holds.
        if let Some(&opt) = self.types.slot_unwraps.get(&r) {
            let f = if opt {
                frag.wrap("κσ.$unslot(", ")")
            } else {
                frag.wrap("(", ")[1]")
            };
            return (f.at(start), ty);
        }
        // What such a function returns where it isn't a slot: one holding the value.
        if let Some(&opt) = self.types.slot_wraps.get(&r) {
            let h = if opt { "κσ.$optValSlot(" } else { "κσ.$valSlot(" };
            return (frag.wrap(h, ")").at(start), ty);
        }
        // Auto-deref through `impl Deref`: `(stack).deref()`.
        if let Some((mutable, target)) = self.types.derefs.get(&self.range_of(e.syntax())).cloned()
        {
            // (`deref_mut` to a string or number returns a slot over it: read and written
            // through `[1]`, so `n.push('c')` is `(n).deref_mut()[1] += "c"`.)
            let cell = self.types.deref_slots.contains(&r);
            let m = match (mutable, cell) {
                (true, true) => "deref_mut()[1]",
                (true, false) => "deref_mut()",
                _ => "deref()",
            };
            return (frag.wrap("(", &format!(").{m}")).at(start), target);
        }
        // A number into a `dyn Trait` with impls on two number types: tagged with its own.
        if let Some(k) = self.types.dyn_nums.get(&r) {
            return (frag.wrap("κσ.$dynNum(", &format!(", \"{k}\")")).at(start), ty);
        }
        (frag.at(start), ty)
    }

    fn operand(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> (Frag, Ty) {
        let (f, t) = self.emit_expr(e, expected);
        (if needs_parens(e) { f.wrap("(", ")") } else { f }, t)
    }

    fn receiver(&mut self, e: &ast::Expr) -> (Frag, Ty) {
        if let Some((range, js, t)) = &self.recv_override
            && *range == self.range_of(e.syntax())
        {
            return (Frag::text(js.clone()), t.clone());
        }
        let (f, t) = self.emit_expr(e, None);
        (
            if receiver_needs_parens(e) {
                f.wrap("(", ")")
            } else {
                f
            },
            t,
        )
    }

    fn emit_expr_inner(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> (Frag, Ty) {
        match e {
            ast::Expr::Literal(l) => self.emit_literal(l, expected),
            // A `&mut` parameter that isn't a cell, handed on to a `&mut T`: a cell over it.
            ast::Expr::PathExpr(p) if self.types.slot_args.contains(&self.range_of(p.syntax())) => {
                let (pf, t) = self.emit_path(p);
                let mut f = Frag::text("κσ.$cell(() => ");
                f.append(pf.clone())
                    .push(", ($v) => { ")
                    .append(pf)
                    .push(" = $v; })");
                (f, t)
            }
            ast::Expr::PathExpr(p) => self.emit_path(p),
            ast::Expr::ParenExpr(p) => match p.expr() {
                Some(inner) => {
                    let (f, t) = self.emit_expr(&inner, expected);
                    (f.wrap("(", ")"), t)
                }
                None => (Frag::text("undefined"), Ty::Unit),
            },
            // `&mut String::new()` for a `&mut String` parameter: a cell of its own.
            ast::Expr::RefExpr(r) if self.types.temp_cells.contains(&self.range_of(r.syntax())) => {
                let Some(inner) = r.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                let (pf, t) = self.emit_expr(&inner, expected);
                (pf.wrap("[", "]"), t)
            }
            // `&mut node.left` for a `&mut Option<..>` parameter: a cell over the slot.
            ast::Expr::RefExpr(r) if self.types.slot_args.contains(&self.range_of(r.syntax())) => {
                let Some(inner) = r.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                let (pf, t) = self.emit_expr(&inner, expected);
                // (Arrow functions, so `&mut self.field` in a method keeps its `this`.)
                let mut f = Frag::text("κσ.$cell(() => ");
                f.append(pf.clone())
                    .push(", ($v) => { ")
                    .append(pf)
                    .push(" = $v; })");
                (f, t)
            }
            // `index_mut`'s `&mut self.cells[i]`: a cell reading and writing that place. (Arrow
            // functions, so `this` stays the receiver.)
            ast::Expr::RefExpr(r)
                if self.types.cell_returns.contains(&self.range_of(r.syntax())) =>
            {
                let Some(inner) = r.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                let (pf, t) = self.emit_expr(&inner, expected);
                let mut f = Frag::text("κσ.$cell(() => ");
                // `&mut *map.get_mut(k).unwrap()`: the entry, written with `insert`.
                if let Some((map, key)) = self.map_entry_place(&inner) {
                    let (mf, _) = self.emit_expr(&map, None);
                    let (kf, kt) = self.emit_expr(&key, None);
                    let kf = self.by_value(&key, kf, &kt);
                    f.append(pf)
                        .push(", ($v) => { ")
                        .append(mf)
                        .push(".$insert(")
                        .append(kf)
                        .push(", $v); })");
                    return (f, t);
                }
                f.append(pf.clone())
                    .push(", ($v) => { ")
                    .append(pf)
                    .push(" = $v; })");
                (f, t)
            }
            // `&mut x` of a local kept as a `&mut i32` value (`let r: &mut i32 = &mut x;`,
            // `Some(&mut x)`, `W { n: &mut x }`): its box, as a slot over it; `&mut r` /
            // `&mut *r` of a `&mut` local: that local's slot or cell passed on.
            ast::Expr::RefExpr(r)
                if self.types.slot_keeps.contains(&self.range_of(r.syntax())) =>
            {
                let Some(inner) = r.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                let mut inner = strip_parens_expr(&inner);
                if let ast::Expr::PrefixExpr(pe) = &inner
                    && pe.op_kind() == Some(UnaryOp::Deref)
                    && let Some(x) = pe.expr()
                {
                    inner = strip_parens_expr(&x);
                }
                let kept_local = matches!(&inner, ast::Expr::PathExpr(p)
                    if self.types.slot_keeps.contains(&self.range_of(p.syntax())));
                let (f, t) = self.emit_expr(&inner, expected);
                if kept_local {
                    (f, t)
                } else {
                    (f.wrap("κσ.$cellSlot(", ")"), t)
                }
            }
            // `let r = &mut v[i];` of a number or string: a slot over the element, made once
            // (`κσ.$placeSlot(v, i, loc)`; a field's is `κσ.$placeSlot(s, "count", null)`).
            ast::Expr::RefExpr(r)
                if self.types.place_slots.contains(&self.range_of(r.syntax())) =>
            {
                let ty = self.ty_of(r.syntax()).unwrap_or(Ty::Unknown);
                match r.expr().map(|e| strip_parens_expr(&e)) {
                    Some(ast::Expr::IndexExpr(ix)) => {
                        let (Some(b), Some(i)) = (ix.base(), ix.index()) else {
                            return (Frag::text("undefined"), ty);
                        };
                        let (bf, _) = self.emit_expr(&b, None);
                        let (xf, _) = self.emit_expr(&i, None);
                        let loc = js_string(&self.location(ix.syntax()));
                        let mut f = Frag::text("κσ.$placeSlot(");
                        f.append(bf).push(", ").append(xf).push(format!(", {loc})"));
                        (f, ty)
                    }
                    Some(ast::Expr::FieldExpr(fe)) => {
                        let (Some(b), Some(n)) = (fe.expr(), fe.name_ref()) else {
                            return (Frag::text("undefined"), ty);
                        };
                        let (bf, _) = self.emit_expr(&b, None);
                        let key = js_string(&adt_gen::prop(n.text()));
                        let mut f = Frag::text("κσ.$placeSlot(");
                        f.append(bf).push(format!(", {key}, null)"));
                        (f, ty)
                    }
                    _ => (Frag::text("undefined"), ty),
                }
            }
            // `&mut v[a..b]`: a view of that part of `v` (`κσ.$sliceMut`).
            ast::Expr::RefExpr(r)
                if self.types.slice_views.contains(&self.range_of(r.syntax())) =>
            {
                let Some(inner) = r.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                self.slice_view = true;
                let out = self.emit_expr(&inner, expected);
                self.slice_view = false;
                out
            }
            ast::Expr::RefExpr(r) => match r.expr() {
                Some(inner) => self.emit_expr(&inner, expected),
                None => (Frag::text("undefined"), Ty::Unknown),
            },
            ast::Expr::PrefixExpr(p) => self.emit_prefix(p, expected),
            ast::Expr::BinExpr(b) => self.emit_bin(b, expected),
            ast::Expr::CastExpr(c) => self.emit_cast(c),
            ast::Expr::CallExpr(c)
                if crate::typeck::result_ctor_call(c).is_some()
                    && self.lookup("Ok").is_none()
                    && !self.fns.contains_key("Ok") =>
            {
                self.emit_result_ctor(c, crate::typeck::result_ctor_call(c).unwrap())
            }
            ast::Expr::CallExpr(c)
                if crate::typeck::is_cell_new(c)
                    && !crate::typeck::cell_shadowed(c, &self.types.adts) =>
            {
                let arg = match c.arg_list().and_then(|a| a.args().next()) {
                    Some(a) => {
                        let (f, t) = self.emit_expr(&a, None);
                        self.by_value(&a, f, &t)
                    }
                    None => Frag::text("undefined"),
                };
                let kind = if c.syntax().text().to_string().contains("RefCell") {
                    "RefCell"
                } else {
                    "Cell"
                };
                let mut f = Frag::mapped(self.off(c.syntax()), "new κσ.RefCell(");
                f.append(arg).push(format!(", \"{kind}\")"));
                (f, self.ty_of(c.syntax()).unwrap_or(Ty::Unknown))
            }
            ast::Expr::CallExpr(c)
                if crate::typeck::is_box_new(c)
                    || crate::typeck::box_from_call(c).is_some()
                    || crate::typeck::is_rc_call(c)
                    || crate::typeck::is_pin_new(c, self.types.adts.defs.contains_key("Pin")) =>
            {
                // `Box::from(sq)` through your `impl From<Sq> for Box<dyn Shape>`.
                if let Some(conv) = self.conv_of(c.syntax())
                    && let Some(a) = c.arg_list().and_then(|a| a.args().next())
                {
                    let (f, t) = self.emit_expr(&a, None);
                    let f = self.by_value(&a, f, &t);
                    let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
                    return (self.convert(conv, f).at(self.off(c.syntax())), ty);
                }
                // `Box::new(x)` is `x`.
                match c.arg_list().and_then(|a| a.args().next()) {
                    Some(a) => {
                        let (f, t) = self.emit_expr(&a, expected);
                        (self.by_value(&a, f, &t), t)
                    }
                    None => (Frag::text("undefined"), Ty::Unknown),
                }
            }
            ast::Expr::CallExpr(c) if crate::typeck::is_default_call(c) => {
                let ty = self
                    .ty_of(c.syntax())
                    .or_else(|| expected.cloned())
                    .unwrap_or(Ty::Unknown);
                // (A generic struct: with the classes its values carry, `κσ.$cv(History, [..]).default()`.)
                let adt_default = match &ty {
                    Ty::Adt(..) => Some(format!("{}.default()", self.dict_of(&ty))),
                    _ => None,
                };
                let js = self
                    .let_array_default(c)
                    .or(adt_default)
                    .or_else(|| default_value(&ty))
                    .unwrap_or_else(|| "undefined".into());
                (Frag::mapped(self.off(c.syntax()), js), ty)
            }
            ast::Expr::CallExpr(c) => {
                let out = self.emit_call(c);
                self.some_unit(c.syntax(), out)
            }
            ast::Expr::MethodCallExpr(m) => {
                let out = self.emit_method(m);
                self.some_unit(m.syntax(), out)
            }
            ast::Expr::FieldExpr(fe) => {
                let out = self.emit_field(fe);
                self.field_move_out(fe, out)
            }
            ast::Expr::AwaitExpr(a) => {
                let Some(inner) = a.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                // `m.lock().await` on tokio's `Mutex`: the value in place (locking never waits).
                if let Some(lock) = self.lock_place(e) {
                    let (mut f, _) = self.emit_expr(&lock, None);
                    f.push(".__c.v");
                    let ty = self.ty_of(a.syntax()).unwrap_or(Ty::Unknown);
                    return (f, ty);
                }
                let (f, t) = self.receiver(&inner);
                let ty = match t {
                    Ty::Future(t) => *t,
                    // (A hand-written future: its `Output`.)
                    _ => self.ty_of(a.syntax()).unwrap_or(Ty::Unknown),
                };
                (Frag::text("await ").append(f).clone(), ty)
            }
            ast::Expr::MacroExpr(m) => match m.macro_call() {
                Some(mc) => self.emit_macro(&mc),
                None => (Frag::text("undefined"), Ty::Unknown),
            },
            ast::Expr::IfExpr(i) if if_is_ternary(i) => self.emit_ternary(i, expected),
            ast::Expr::BlockExpr(b)
                if matches!(b.modifier(), Some(ast::BlockModifier::Async(_))) =>
            {
                self.emit_async_block(b)
            }
            ast::Expr::BlockExpr(b) if simple_tail(b).is_some() => {
                let tail = simple_tail(b).unwrap();
                let (f, t) = self.emit_expr(&tail, expected);
                (f.wrap("(", ")"), t)
            }
            ast::Expr::IfExpr(_)
            | ast::Expr::BlockExpr(_)
            | ast::Expr::LoopExpr(_)
            | ast::Expr::MatchExpr(_) => {
                let t = self.temp();
                self.line(Frag::text(format!("let {t};")));
                let ty = self.emit_into(e, &Dest::Assign(t.clone()), expected);
                (Frag::text(t), ty)
            }
            ast::Expr::WhileExpr(_) | ast::Expr::ForExpr(_) => {
                self.emit_into(e, &Dest::Discard, None);
                (Frag::text("undefined"), Ty::Unit)
            }
            ast::Expr::ReturnExpr(_) | ast::Expr::BreakExpr(_) | ast::Expr::ContinueExpr(_) => {
                self.emit_jump(e);
                (Frag::text("undefined"), Ty::Never)
            }
            ast::Expr::IndexExpr(ix) => self.emit_index(ix),
            ast::Expr::RecordExpr(r) => self.emit_record(r),
            ast::Expr::ClosureExpr(c) => (self.emit_owning_closure(c), Ty::Unknown),
            ast::Expr::RangeExpr(r) => (
                self.emit_range_value(r),
                Ty::Iter {
                    elem: Box::new(Ty::Unknown),
                    mutable: false,
                },
            ),
            ast::Expr::TryExpr(t) => {
                // `x?`: `const $t = x; if ($t == null) return null;`, then `$t` is the value.
                let Some(inner) = t.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                let (f, ty) = self.emit_expr(&inner, None);
                let tmp = self.temp();
                let mut decl = Frag::mapped(self.off(inner.syntax()), format!("const {tmp} = "));
                decl.append(f).push(";");
                self.line(decl);
                let q = t
                    .question_mark_token()
                    .map(|q| self.base + usize::from(q.text_range().start()));
                // `Result`: return the `Err` itself; the value is `$t[0]`.
                if let Ty::Result(x, _) = ty {
                    // `?` converting the error with a `From` impl.
                    let early = match self.conv_of(t.syntax()) {
                        Some(conv) => {
                            let e = self.convert(conv, Frag::text(format!("{tmp}[0]")));
                            format!("if ({tmp}.$tag === 1) return κσ.Err({});", e.plain())
                        }
                        None => format!("if ({tmp}.$tag === 1) return {tmp};"),
                    };
                    self.line(match q {
                        Some(o) => Frag::mapped(o, early),
                        None => Frag::text(early),
                    });
                    return (Frag::text(format!("{tmp}[0]")), *x);
                }
                let early = format!("if ({tmp} == null) return null;");
                self.line(match q {
                    Some(o) => Frag::mapped(o, early),
                    None => Frag::text(early),
                });
                let value = match ty {
                    Ty::Option(x) => *x,
                    _ => Ty::Unknown,
                };
                if nullable_payload(&value) {
                    return (Frag::text(format!("κσ.$unsome({tmp})")), value);
                }
                (Frag::text(tmp), value)
            }
            ast::Expr::ArrayExpr(a) => self.emit_array(a),
            ast::Expr::TupleExpr(t) if t.fields().next().is_some() => {
                // `(a, b)` is `[a, b]`; elements taken from places are copied.
                let items: Vec<Frag> = t
                    .fields()
                    .map(|f| {
                        let (frag, ty) = self.emit_expr(&f, None);
                        self.by_value(&f, frag, &ty)
                    })
                    .collect();
                (
                    Frag::text("[")
                        .append(Frag::join(items, ", "))
                        .push("]")
                        .clone(),
                    Ty::Unknown,
                )
            }
            ast::Expr::TupleExpr(t) if t.fields().next().is_none() => {
                (Frag::text("undefined"), Ty::Unit)
            }
            other => {
                let what = match other {
                    ast::Expr::MatchExpr(_) => "`match`",
                    ast::Expr::ClosureExpr(_) => "closures",
                    ast::Expr::TupleExpr(_) => "tuples",
                    ast::Expr::RecordExpr(_) => "struct literals",
                    _ => "this expression",
                };
                self.unsupported(other.syntax(), what);
                (Frag::text("undefined"), Ty::Unknown)
            }
        }
    }

    fn emit_literal(&mut self, l: &ast::Literal, expected: Option<&Ty>) -> (Frag, Ty) {
        match l.kind() {
            LiteralKind::IntNumber(n) => {
                let ty = match n.suffix() {
                    Some(s) => Ty::from_name(s),
                    None => expected.cloned().unwrap_or(Ty::IntLit),
                };
                let value = match n.value() {
                    Ok(v) => v,
                    Err(_) => {
                        self.error(l.syntax(), "invalid integer literal");
                        0
                    }
                };
                // `i128` / `u128` are BigInts.
                let ty = match self.ty_of(l.syntax()) {
                    Some(t @ Ty::Int(k)) if k.is_big() => t,
                    _ => ty,
                };
                if matches!(ty, Ty::Int(k) if k.is_big()) {
                    return (Frag::text(format!("{value}n")), ty);
                }
                // (Powers of two and the like are exact: `0x4000_0000_0000_0000`.)
                if value > (1u128 << 53) && (value as f64) as u128 != value {
                    self.warn(
                        l.syntax(),
                        "integer literal exceeds 2^53 and loses precision as a JS number",
                    );
                }
                (Frag::text(value.to_string()), ty)
            }
            LiteralKind::FloatNumber(n) => {
                let f32 = matches!(self.ty_of(l.syntax()), Some(Ty::Float { f32: true }));
                let text = n.value_string();
                let text = if text.ends_with('.') {
                    format!("{text}0")
                } else {
                    text
                };
                let frag = if f32 {
                    Frag::text(format!("Math.fround({text})"))
                } else {
                    Frag::text(text)
                };
                (frag, Ty::Float { f32 })
            }
            LiteralKind::Bool(b) => (Frag::text(b.to_string()), Ty::Bool),
            LiteralKind::String(s) => match s.value() {
                Ok(v) => (Frag::text(js_string(&v)), Ty::Str),
                Err(_) => {
                    self.error(l.syntax(), "invalid string literal");
                    (Frag::text("\"\""), Ty::Str)
                }
            },
            LiteralKind::Char(c) => match c.value() {
                Ok(v) => (Frag::text(js_string(&v.to_string())), Ty::Char),
                Err(_) => {
                    self.error(l.syntax(), "invalid char literal");
                    (Frag::text("\"\""), Ty::Char)
                }
            },
            LiteralKind::Byte(b) => (
                Frag::text(b.value().unwrap_or(0).to_string()),
                Ty::Int(IntKind::U8),
            ),
            // `b"raw"`: an array of its bytes.
            LiteralKind::ByteString(b) => {
                let bytes = b.value().map(|v| v.into_owned()).unwrap_or_default();
                let items: Vec<String> = bytes.iter().map(|x| x.to_string()).collect();
                (
                    Frag::mapped(self.off(l.syntax()), format!("[{}]", items.join(", "))),
                    Ty::Vec(Box::new(Ty::Int(IntKind::U8))),
                )
            }
            _ => {
                self.unsupported(l.syntax(), "byte strings and C strings");
                (Frag::text("undefined"), Ty::Unknown)
            }
        }
    }

    /// A unit struct or variant with const parameters (`Tag::<7>`, `Buf::<4>::Empty`):
    /// constructed with them.
    fn unit_const_struct(&self, p: &ast::PathExpr, segs: &[String]) -> Option<(Frag, Ty)> {
        let crate::adt::Resolved::Ctor(adt, v) = self.resolve(segs)? else {
            return None;
        };
        if self.carried_names(&adt).is_empty() {
            return None;
        }
        let consts = self.const_args_of(p.syntax(), &adt);
        let variant = &self.types.adts.defs[&adt].variants[v];
        match variant.shape {
            crate::adt::Shape::Unit => {
                Some((self.construct(&adt, v, consts), Ty::Adt(adt, vec![])))
            }
            // `.map(Chunk)`: a function of the fields.
            crate::adt::Shape::Tuple => {
                let params: Vec<String> = (0..variant.fields.len())
                    .map(|k| format!("$f{k}"))
                    .collect();
                let mut args: Vec<Frag> = params.iter().map(|p| Frag::text(p.clone())).collect();
                args.extend(consts);
                let mut f = Frag::text(format!("(({}) => ", params.join(", ")));
                f.append(self.construct(&adt, v, args)).push(")");
                Some((f, Ty::Unknown))
            }
            crate::adt::Shape::Named => None,
        }
    }

    fn emit_path(&mut self, p: &ast::PathExpr) -> (Frag, Ty) {
        let Some(path) = p.path() else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        // `T::SIDES` / `Self::SIDES`: on the class.
        if let Some(class) = self.param_class(p)
            && let Some(k) = path.segment().and_then(|s| s.name_ref())
        {
            let ty = self.ty_of(p.syntax()).unwrap_or(Ty::Unknown);
            let mut out = Frag::text(class);
            out.push(member(k.text()));
            return (out, ty);
        }
        if let Some(name) = path.as_single_name_ref() {
            if let Some(local) = self.lookup(name.text()) {
                // A `&mut i32` local passed on as the `&mut` itself (`return r`, `W { n: r }`,
                // `q = r`, `Some(r)`): its slot, or its box as a slot over it.
                if self.types.slot_keeps.contains(&self.range_of(p.syntax())) {
                    if let Some(slot) = local.js.strip_suffix("[1]") {
                        return (Frag::text(slot.to_string()), local.ty.clone());
                    }
                    if let Some(cell) = local.js.strip_suffix("[0]") {
                        return (
                            Frag::text(format!("κσ.$cellSlot({cell})")),
                            local.ty.clone(),
                        );
                    }
                    // (A loop variable over a place, `a[$t1]`: a slot over that place.)
                    if local.js.ends_with(']') || local.js.contains('.') {
                        let js = local.js.clone();
                        return (
                            Frag::text(format!("κσ.$cell(() => {js}, ($v) => {{ {js} = $v; }})")),
                            local.ty.clone(),
                        );
                    }
                }
                // `&mut n` / a `&mut` parameter passed on: the cell itself.
                if self.types.cell_args.contains(&self.range_of(p.syntax()))
                    && let Some(cell) = local.js.strip_suffix("[0]")
                {
                    return (Frag::text(cell.to_string()), local.ty.clone());
                }
                // (A loop's `x` over `a.iter_mut()` that is the element itself, `a[$t1]`: a
                // cell over that place. Also for a `T` or struct element handed to a closure's
                // `&mut T` parameter, which takes a cell whatever `T` is.)
                if self.types.cell_args.contains(&self.range_of(p.syntax()))
                    && (local.js.ends_with(']') || local.js.contains('.'))
                {
                    let js = local.js.clone();
                    return (
                        Frag::text(format!("κσ.$cell(() => {js}, ($v) => {{ {js} = $v; }})")),
                        local.ty.clone(),
                    );
                }
                // A closure given to a std method (`it.map(f)`): dropped at the end of the
                // statement, with the adapter that owns it in Rust.
                if let Some(flag) = self.drop_flags.get(&local.js).cloned()
                    && self.types.move_sites.contains(&self.range_of(p.syntax()))
                    && matches!(local.ty, Ty::Closure { .. })
                    && self.std_method_arg(p)
                {
                    let (js, ty) = (local.js.clone(), local.ty.clone());
                    if !self.stmt_drops.iter().any(|(_, j)| *j == js) {
                        self.stmt_drops.push((flag, js.clone()));
                    }
                    return (Frag::text(js), ty);
                }
                // A move of a value that would be dropped here: its new owner drops it.
                if let Some(flag) = self.drop_flags.get(&local.js)
                    && self.types.move_sites.contains(&self.range_of(p.syntax()))
                {
                    return (
                        Frag::text(format!("({flag} = false, {})", local.js)),
                        local.ty.clone(),
                    );
                }
                return (Frag::text(local.js.clone()), local.ty.clone());
            }
            if let Some(f) = self.fns.get(name.text()) {
                return (Frag::text(f.js.clone()), Ty::Fn);
            }
            if let Some(js) = self.consts.get(name.text().to_string().as_str()) {
                let ty = self.ty_of(p.syntax()).unwrap_or(Ty::Unknown);
                return (Frag::text(js.clone()), ty);
            }
            if let Some(r) = self.unit_const_struct(p, &[name.text().to_string()]) {
                return r;
            }
            if let Some(r) = self.emit_adt_path(&[name.text().to_string()], None) {
                return r;
            }
            if name.text() == "None" {
                return (Frag::text("null"), Ty::Option(Box::new(Ty::Unknown)));
            }
            // `Some(x)` is `x` itself; `Ok`/`Err` are the runtime's constructors.
            let ctor = match name.text() {
                "Some" => Some("κσ.$some"),
                "Ok" => Some("κσ.Ok"),
                "Err" => Some("κσ.Err"),
                _ => None,
            };
            if let Some(js) = ctor {
                let ty = self.ty_of(p.syntax()).unwrap_or(Ty::Fn);
                return (Frag::text(js), ty);
            }
            self.error(
                p.syntax(),
                format!("cannot find value `{}` in this scope", name.text()),
            );
            return (Frag::text("undefined"), Ty::Unknown);
        }
        let segs: Vec<String> = path
            .segments()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect();
        if let [single] = self.strip_modules(&segs).as_slice() {
            if let Some(js) = self.consts.get(single) {
                let ty = self.ty_of(p.syntax()).unwrap_or(Ty::Unknown);
                return (Frag::text(js.clone()), ty);
            }
            if let Some(f) = self.fns.get(single) {
                return (Frag::text(f.js.clone()), Ty::Fn);
            }
        }
        if let Some(r) = self.unit_const_struct(p, &segs) {
            return r;
        }
        if let Some(r) = self.emit_adt_path(&segs, Some(&path)) {
            // An associated function as a value that takes classes (`Stack::<T>::make`): bound.
            if let Some(tys) = self
                .types
                .dict_args
                .get(&self.range_of(p.syntax()))
                .cloned()
            {
                let mut ds: Vec<String> = tys.iter().map(|t| self.dict_of(t)).collect();
                ds.extend(
                    self.dbg_arg_frags(p.syntax())
                        .into_iter()
                        .flat_map(|f| f.parts.into_iter().map(|(_, t)| t)),
                );
                let mut f = Frag::text("((...$a) => ");
                f.append(r.0).push(format!("(...$a, {}))", ds.join(", ")));
                return (f, r.1);
            }
            return r;
        }
        if matches!(
            segs.iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            [.., "String", "from"] | [.., "ToString", "to_string"]
        ) {
            // `.map_err(String::from)` through your `impl From<A> for String`.
            if let Some(conv @ crate::adt::Conv::From { .. }) = self.conv_of(p.syntax()) {
                let f = self.convert(conv, Frag::text("x"));
                return (f.wrap("((x) => ", ")"), Ty::Unknown);
            }
            return (Frag::text("String"), Ty::Unknown);
        }
        if segs.len() >= 2 && segs[segs.len() - 2] == "char" && segs[segs.len() - 1] == "from" {
            return (Frag::text("((b) => String.fromCharCode(b))"), Ty::Unknown);
        }
        let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
        if let Some((js, ty)) = std_const(std_tail(&segs)) {
            return (Frag::text(js), ty);
        }
        if let [.., "Ordering", v @ ("Less" | "Equal" | "Greater")] = segs.as_slice() {
            let n = match *v {
                "Less" => "-1",
                "Equal" => "0",
                _ => "1",
            };
            return (Frag::text(n), Ty::Ordering);
        }
        if let [.., "Option", "None"] = segs.as_slice() {
            return (Frag::text("null"), Ty::Option(Box::new(Ty::Unknown)));
        }
        if let Some(r) = self.emit_jsvalue_path(&segs) {
            return r;
        }
        if !self.type_errors {
            self.error(
                p.syntax(),
                format!(
                    "cannot find value `{}` in this scope",
                    path.syntax().text()
                ),
            );
        }
        (Frag::text("undefined"), Ty::Unknown)
    }

    fn emit_prefix(&mut self, p: &ast::PrefixExpr, expected: Option<&Ty>) -> (Frag, Ty) {
        let Some(inner) = p.expr() else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        // `-v` / `!v` with an `impl Neg` / `impl Not`.
        if let Some(conv @ crate::adt::Conv::Method { .. }) = self.conv_of(p.syntax()) {
            let (f, t) = self.emit_expr(&inner, None);
            let f = self.by_value(&inner, f, &t);
            let ty = self.ty_of(p.syntax()).unwrap_or(Ty::Unknown);
            return (self.convert(conv, f).at(self.off(p.syntax())), ty);
        }
        match p.op_kind() {
            Some(UnaryOp::Deref) => self.emit_expr(&inner, expected),
            // `-a` on a `T`: by its class.
            Some(UnaryOp::Neg) if matches!(self.ty_of(inner.syntax()), Some(Ty::Param(_))) => {
                let t = self.ty_of(inner.syntax()).unwrap_or(Ty::Unknown);
                let (f, _) = self.emit_expr(&inner, None);
                let dict = self.dict_of(&t);
                let mut out = Frag::mapped(self.off(p.syntax()), "κσ.$gneg(");
                out.append(f).push(format!(", {dict})"));
                (out, t)
            }
            // `!a` on a `T`: by its class (a bool, a number of its width, or a struct's `not`).
            Some(UnaryOp::Not) if matches!(self.ty_of(inner.syntax()), Some(Ty::Param(_))) => {
                let t = self.ty_of(inner.syntax()).unwrap_or(Ty::Unknown);
                let (f, _) = self.emit_expr(&inner, None);
                let dict = self.dict_of(&t);
                let mut out = Frag::mapped(self.off(p.syntax()), "κσ.$gnot(");
                out.append(f).push(format!(", {dict})"));
                (out, t)
            }
            Some(UnaryOp::Neg) => {
                let (f, t) = self.operand(&inner, expected);
                let neg = Frag::text("-").append(f).clone();
                match &t {
                    Ty::Int(k) if !k.is_signed() => {
                        self.error(
                            p.syntax(),
                            format!("cannot apply unary operator `-` to type `{}`", t.describe()),
                        );
                        (neg, t)
                    }
                    // `-i32::MIN` panics in debug builds.
                    Ty::Int(k) if overflow_checks() && !matches!(inner, ast::Expr::Literal(_)) => {
                        let (min, _) = int_bounds(*k);
                        let loc = js_string(&self.location(p.syntax()));
                        let mut f = Frag::text("κσ.$ckNeg(");
                        f.append(neg).push(format!(", {min}, {loc})"));
                        (f, t)
                    }
                    Ty::Int(k) if k.is_big() && !matches!(inner, ast::Expr::Literal(_)) => {
                        (wrap_big(neg, *k), t)
                    }
                    Ty::Int(k) if !matches!(inner, ast::Expr::Literal(_)) => match k.wrap_parts() {
                        Some((pre, post)) => (neg.wrap(pre, post), t),
                        None => (neg, t),
                    },
                    _ => (neg, t),
                }
            }
            Some(UnaryOp::Not) => {
                let (f, t) = self.operand(&inner, expected);
                match &t {
                    Ty::Bool | Ty::Unknown => (Frag::text("!").append(f).clone(), t),
                    Ty::Int(k) => {
                        let not = Frag::text("~").append(f.clone()).clone();
                        match k {
                            IntKind::U32 | IntKind::U8 | IntKind::U16 => {
                                let (pre, post) = k.wrap_parts().unwrap();
                                (not.wrap(pre, post), t)
                            }
                            k if k.is_narrow() => (not, t),
                            k if k.is_big() => (wrap_big(not, *k), t),
                            k => {
                                let mut g = Frag::text("κσ.$not64(");
                                g.append(f).push(format!(", {})", k.is_signed()));
                                (g, t)
                            }
                        }
                    }
                    Ty::IntLit => (Frag::text("~").append(f).clone(), Ty::IntLit),
                    _ => {
                        self.error(
                            p.syntax(),
                            format!("cannot apply unary operator `!` to type `{}`", t.describe()),
                        );
                        (f, t)
                    }
                }
            }
            None => (Frag::text("undefined"), Ty::Unknown),
        }
    }

    fn emit_bin(&mut self, b: &ast::BinExpr, expected: Option<&Ty>) -> (Frag, Ty) {
        let (Some(lhs), Some(rhs), Some(op)) = (b.lhs(), b.rhs(), b.op_kind()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        // An overloaded operator (`impl Add for V`): `lhs.add$0(rhs)`. `a += b` calls
        // `add_assign` on `a` itself, which changes it in place.
        if let Some(crate::adt::Conv::Method { key }) = self.conv_of(b.syntax()) {
            let assign = matches!(op, BinaryOp::Assignment { .. });
            let (lf, lt) = self.emit_expr(&lhs, None);
            let lf = if assign {
                lf
            } else {
                self.by_value(&lhs, lf, &lt)
            };
            let (rf, rt) = self.emit_expr(&rhs, None);
            let rf = self.by_value(&rhs, rf, &rt);
            let ty = if assign {
                Ty::Unit
            } else {
                self.ty_of(b.syntax()).unwrap_or(Ty::Unknown)
            };
            // (On a number: `$ext$f64.prototype.mul$0.call(2, v)`. The checker's type first: a
            // range value is a `Range` there, `r == span`.)
            let lt = self.raw_ty_of(lhs.syntax()).unwrap_or(lt);
            let mut f = if let Some(k) =
                crate::adt::ext_key_of(&lt).filter(|k| self.types.adts.ext_types.contains(*k))
                && self.types.adts.method(k, &key).is_some()
            {
                let mut f = Frag::text(format!("$ext${k}.prototype.{key}.call("));
                f.append(lf).push(", ").append(rf).push(")");
                f
            } else {
                let mut f = lf.wrap("(", ")");
                f.push(format!(".{key}(")).append(rf).push(")");
                f
            };
            // `a != b` through a `PartialEq<B>` impl's `eq`.
            if matches!(op, BinaryOp::CmpOp(ast::CmpOp::Eq { negated: true })) {
                f = f.wrap("!", "");
            }
            // `a < b` through a `PartialOrd<B>` impl's `partial_cmp`: `None` compares false.
            if key == "partial_cmp"
                && let BinaryOp::CmpOp(ast::CmpOp::Ord { ordering, strict }) = op
            {
                let o = match (ordering, strict) {
                    (ast::Ordering::Less, true) => "<",
                    (ast::Ordering::Less, false) => "<=",
                    (ast::Ordering::Greater, true) => ">",
                    (ast::Ordering::Greater, false) => ">=",
                };
                f = f.wrap("κσ.$pord(", &format!(", \"{o}\")"));
            }
            return (f.at(self.off(b.syntax())), ty);
        }
        match op {
            // `p = &mut y as *mut i32;` with `p` holding a cell: point it at y's.
            BinaryOp::Assignment { op: None }
                if self
                    .types
                    .ptrs
                    .repoints
                    .contains(&self.range_of(b.syntax())) =>
            {
                let cell = match &lhs {
                    ast::Expr::PathExpr(p) => p
                        .path()
                        .and_then(|p| p.as_single_name_ref())
                        .and_then(|n| self.lookup(n.text().to_string().as_str()))
                        .and_then(|l| l.js.strip_suffix("[0]").map(str::to_string)),
                    _ => None,
                };
                let (rf, _) = self.emit_expr(&rhs, None);
                let mut f = Frag::text(cell.unwrap_or_else(|| "undefined".into()));
                f.push(" = ").append(rf);
                (f, Ty::Unit)
            }
            // `r = &mut other;` / `r = s.at(1);` with `r` a `&mut i32` local: rebind its slot
            // (or cell) rather than write through it.
            BinaryOp::Assignment { op: None }
                if self
                    .types
                    .slot_assigns
                    .contains(&self.range_of(b.syntax())) =>
            {
                let target = match strip_parens_expr(&lhs) {
                    ast::Expr::PathExpr(p) => p
                        .path()
                        .and_then(|p| p.as_single_name_ref())
                        .and_then(|n| self.lookup(n.text().to_string().as_str()))
                        .map(|l| {
                            l.js.strip_suffix("[1]")
                                .or_else(|| l.js.strip_suffix("[0]"))
                                .unwrap_or(&l.js)
                                .to_string()
                        }),
                    _ => None,
                };
                let (rf, _) = self.emit_expr(&rhs, None);
                let mut f = Frag::text(target.unwrap_or_else(|| "undefined".into()));
                f.push(" = ").append(rf);
                (f, Ty::Unit)
            }
            BinaryOp::Assignment { op } => {
                let assignable = match strip_refs(&lhs) {
                    ast::Expr::PathExpr(_) | ast::Expr::FieldExpr(_) | ast::Expr::PrefixExpr(_) => {
                        true
                    }
                    // `v[i] = x` lowers to `v[κσ.$ix(v, i)] = x`, which evaluates `v` twice.
                    // (A view `v[a..b]` may be evaluated twice too: both write to `v`.)
                    ast::Expr::IndexExpr(ix) => ix.base().is_some_and(|b| {
                        is_pure(&b)
                            || self.same_vec_call(&b).is_some()
                            || self.call_base(&b)
                            || self
                                .types
                                .slice_views
                                .contains(&self.range_of(strip_parens_expr(&b).syntax()))
                    }),
                    // `(a, b) = (b, a)` is JS destructuring: `[a, b] = [b, a]`.
                    ast::Expr::TupleExpr(_) => op.is_none(),
                    _ => false,
                };
                if !assignable {
                    self.unsupported(lhs.syntax(), "assignment to this place");
                }
                let (lf, lt) = match &lhs {
                    // Destructuring targets are places, so no copies: `[a, b] = ...`.
                    ast::Expr::TupleExpr(t) => {
                        let targets: Vec<Frag> =
                            t.fields().map(|f| self.emit_expr(&f, None).0).collect();
                        let ty = self.ty_of(lhs.syntax()).unwrap_or(Ty::Unknown);
                        (
                            Frag::text("[")
                                .append(Frag::join(targets, ", "))
                                .push("]")
                                .clone(),
                            ty,
                        )
                    }
                    _ => match self.entry_slot_of(&lhs) {
                        Some(slot) => {
                            // Evaluate the map, key and default once: `const $t = κσ.$slot(..)`.
                            let t = self.temp();
                            let mut decl = Frag::text(format!("const {t} = "));
                            decl.append(slot).push(";");
                            self.line(decl);
                            (
                                Frag::text(format!("{t}[1]")),
                                self.ty_of(lhs.syntax()).unwrap_or(Ty::Unknown),
                            )
                        }
                        // `*opt.get_or_insert(0) += 1`: fill it, then write to the option.
                        None => match self.option_place(&lhs) {
                            Some((fill, place)) => {
                                self.line(fill.wrap("", ";"));
                                (place, self.ty_of(lhs.syntax()).unwrap_or(Ty::Unknown))
                            }
                            None => self.emit_assign_place(&lhs),
                        },
                    },
                };
                let (rf, rt) = self.operand(&rhs, Some(&lt));
                // Shift amounts don't unify with the shifted value.
                let ty = match op {
                    Some(ArithOp::Shl | ArithOp::Shr) => lt.clone(),
                    _ => Ty::unify(&lt, &rt).default_lit(),
                };
                let value = match op {
                    None => self.by_value(&rhs, rf, &rt),
                    // `a += b` on a `T`: `a = κσ.$gopAssign("+", a, b, $T, loc)`.
                    Some(op) if matches!(lt, Ty::Param(_)) => {
                        self.generic_op(b, op, lf.clone(), rf, &lt, "gopAssign")
                    }
                    Some(op) => match self.arith(b, op, lf.clone(), rf, &ty) {
                        // Plain JS operators keep the compound form (`x += y`).
                        Ok(f) => f,
                        Err((lf2, rf2, js_op)) => {
                            let mut f = lf2;
                            f.push(format!(" {js_op}= ")).append(rf2);
                            return (f, Ty::Unit);
                        }
                    },
                };
                // `*r = v` / `*self = v` with `r` a `&mut` to a struct or enum (or a `Vec`, map,
                // set or other collection): the JS variable holds the object itself, so replace
                // its contents in place.
                let through_ref = match strip_refs(&lhs) {
                    ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => {
                        matches!(
                            p.expr().map(|e| strip_refs(&e)),
                            Some(ast::Expr::PathExpr(_))
                        )
                    }
                    ast::Expr::PathExpr(_) => lf.plain() == "this",
                    _ => false,
                };
                if op.is_none()
                    && through_ref
                    && matches!(
                        lt,
                        Ty::Adt(..) | Ty::Vec(_) | Ty::Map(..) | Ty::Set(_) | Ty::Coll(..)
                    )
                {
                    let mut f = Frag::text("κσ.$replace(");
                    f.append(lf).push(", ").append(value).push(")");
                    return (f, Ty::Unit);
                }
                let mut f = lf;
                f.push(" = ").append(value);
                (f, Ty::Unit)
            }
            BinaryOp::LogicOp(op) => {
                let (lf, _) = self.operand(&lhs, Some(&Ty::Bool));
                // `a && { let x = f(); x > 1 }`: the right side's statements only run when the
                // left doesn't decide.
                if needs_hoist(rhs.syntax()) {
                    let t = self.temp();
                    let mut f = Frag::text(format!("let {t} = "));
                    f.append(lf).push(";");
                    self.line(f);
                    let not = if op == LogicOp::And { "" } else { "!" };
                    self.line(Frag::text(format!("if ({not}{t}) {{")));
                    self.out.indent += 1;
                    self.scopes.push(HashMap::new());
                    let (rf, _) = self.emit_expr(&rhs, Some(&Ty::Bool));
                    let mut f = Frag::text(format!("{t} = "));
                    f.append(rf).push(";");
                    self.line(f);
                    self.scopes.pop();
                    self.out.indent -= 1;
                    self.line(Frag::text("}"));
                    return (Frag::text(t), Ty::Bool);
                }
                let (rf, _) = self.operand(&rhs, Some(&Ty::Bool));
                let mut f = lf;
                f.push(if op == LogicOp::And { " && " } else { " || " })
                    .append(rf);
                (f, Ty::Bool)
            }
            BinaryOp::CmpOp(op) => {
                // `d == Dir::S` on an enum with a derived `PartialEq`: a tag compare.
                if let CmpOp::Eq { negated } = op
                    && let Some((other, v)) = self.unit_variant_cmp(&lhs, &rhs)
                {
                    let (of, _) = self.operand(&other, None);
                    let mut f = of;
                    f.push(if negated { ".$tag !== " } else { ".$tag === " })
                        .push(v.to_string());
                    return (f, Ty::Bool);
                }
                let (lf, lt) = self.operand(&lhs, None);
                let (rf, _) = self.operand(&rhs, Some(&lt));
                // `p == q` on a struct or enum with a derived `PartialEq`: its own `$eq`.
                if let (Ty::Adt(n, _), CmpOp::Eq { negated }) = (&lt, op)
                    && self.types.adts.defs.get(n).is_some_and(|d| {
                        d.derives("PartialEq") && !d.manual.iter().any(|m| m == "PartialEq")
                    })
                    && matches!(self.ty_of(rhs.syntax()), Some(Ty::Adt(n2, _)) if n2 == *n)
                {
                    let mut f = if matches!(
                        strip_parens_expr(&lhs),
                        ast::Expr::PathExpr(_)
                            | ast::Expr::FieldExpr(_)
                            | ast::Expr::MethodCallExpr(_)
                            | ast::Expr::CallExpr(_)
                            | ast::Expr::IndexExpr(_)
                    ) {
                        lf
                    } else {
                        lf.wrap("(", ")")
                    };
                    f.push(".$eq(").append(rf).push(")");
                    return (if negated { f.wrap("!", "") } else { f }, Ty::Bool);
                }
                // Tuples and Vecs order lexicographically, like Rust's derived `Ord`; `None` is below
                // every `Some` (an NS field's type is the checker's).
                let option = matches!(self.ty_of(lhs.syntax()), Some(Ty::Option(_)));
                let ordered = matches!(
                    (&lt, op),
                    (
                        Ty::Vec(_) | Ty::Tuple(_) | Ty::Adt(..) | Ty::Param(_),
                        CmpOp::Ord { .. }
                    )
                ) || (option && matches!(op, CmpOp::Ord { .. }));
                if ordered {
                    let mut f = Frag::text("κσ.$cmp(");
                    f.append(lf)
                        .push(", ")
                        .append(rf)
                        .push(format!(") {} 0", op));
                    return (f, Ty::Bool);
                }
                if let (
                    Ty::Vec(_)
                    | Ty::Option(_)
                    | Ty::Tuple(_)
                    | Ty::Set(_)
                    | Ty::Map(..)
                    | Ty::Adt(..)
                    | Ty::Result(..)
                    | Ty::Param(_)
                    | Ty::Iter { .. },
                    CmpOp::Eq { negated },
                ) = (&lt, op)
                {
                    let mut f = Frag::text(if negated { "!κσ.$eq(" } else { "κσ.$eq(" });
                    f.append(lf).push(", ").append(rf).push(")");
                    return (f, Ty::Bool);
                }
                let js = match op {
                    CmpOp::Eq { negated: false } => "===".to_string(),
                    CmpOp::Eq { negated: true } => "!==".to_string(),
                    other => other.to_string(),
                };
                let mut f = lf;
                f.push(format!(" {js} ")).append(rf);
                (f, Ty::Bool)
            }
            // `a + b` on a `T`: by `T`'s class (`$T`), which knows the number kind or calls a
            // struct's own operator.
            BinaryOp::ArithOp(op) if matches!(self.ty_of(lhs.syntax()), Some(Ty::Param(_))) => {
                let lt = self.ty_of(lhs.syntax()).unwrap_or(Ty::Unknown);
                let (lf, _) = self.operand(&lhs, None);
                let (rf, _) = self.operand(&rhs, None);
                let f = self.generic_op(b, op, lf, rf, &lt, "gop");
                (f, lt)
            }
            BinaryOp::ArithOp(op) => {
                let (lf, lt) = self.operand(&lhs, expected);
                let rhs_expected = if lt == Ty::IntLit {
                    expected.cloned().unwrap_or(Ty::IntLit)
                } else {
                    lt.clone()
                };
                let (rf, rt) = self.operand(&rhs, Some(&rhs_expected));
                let mut ty = Ty::unify(&lt, &rt);
                if ty == Ty::IntLit
                    && let Some(t @ Ty::Int(_)) = expected
                {
                    ty = t.clone();
                }
                if matches!(op, ArithOp::Shl | ArithOp::Shr) {
                    // Shift amounts don't unify with the shifted value.
                    ty = lt.clone();
                }
                let result = match self.arith(b, op, lf, rf, &ty) {
                    Ok(f) => f,
                    Err((l, r, js_op)) => {
                        let mut f = l;
                        f.push(format!(" {js_op} ")).append(r);
                        f
                    }
                };
                (result, ty)
            }
        }
    }

    /// `κσ.$gop("+", a, b, $T, loc)` (or `$gopAssign`): an operator on a type parameter's values.
    fn generic_op(
        &mut self,
        b: &ast::BinExpr,
        op: ArithOp,
        l: Frag,
        r: Frag,
        t: &Ty,
        helper: &str,
    ) -> Frag {
        let dict = self.dict_of(t);
        let loc = js_string(&self.location(b.syntax()));
        let mut f = Frag::mapped(self.off(b.syntax()), format!("κσ.${helper}(\"{op}\", "));
        f.append(l)
            .push(", ")
            .append(r)
            .push(format!(", {dict}, {loc})"));
        f
    }

    /// Typed lowering of an arithmetic operator. `Err` means "plain JS operator is correct", so
    /// callers can keep compound-assignment syntax.
    fn arith(
        &mut self,
        b: &ast::BinExpr,
        op: ArithOp,
        l: Frag,
        r: Frag,
        ty: &Ty,
    ) -> Result<Frag, (Frag, Frag, String)> {
        // Debug-build semantics: integer overflow panics like rustc's `attempt to add with overflow`.
        if overflow_checks()
            && matches!(ty, Ty::Int(_) | Ty::IntLit)
            && let Some(k) = ty.int_kind()
        {
            let loc = js_string(&self.location(b.syntax()));
            let (min, max) = int_bounds(k);
            let checked = |name: &str, extra: &str, l: Frag, r: Frag| {
                let mut f = Frag::text(format!("κσ.${name}("));
                f.append(l)
                    .push(", ")
                    .append(r)
                    .push(format!("{extra}, {min}, {max}, {loc})"));
                f
            };
            match op {
                ArithOp::Add => return Ok(checked("ckArith", ", \"+\"", l, r)),
                ArithOp::Sub => return Ok(checked("ckArith", ", \"-\"", l, r)),
                ArithOp::Mul => return Ok(checked("ckArith", ", \"*\"", l, r)),
                ArithOp::Div => return Ok(checked("ckDiv", ", false", l, r)),
                ArithOp::Rem => return Ok(checked("ckDiv", ", true", l, r)),
                // A shift by the width or more: the amount is checked, then the usual shift.
                ArithOp::Shl | ArithOp::Shr => {
                    let what = if op == ArithOp::Shl {
                        "shift left"
                    } else {
                        "shift right"
                    };
                    let bits = k.bits();
                    let mut amount = Frag::text("κσ.$ckShift(");
                    amount
                        .append(r)
                        .push(format!(", {bits}, \"{what}\", {loc})"));
                    return self.arith_unchecked(b, op, l, amount, ty);
                }
                _ => {}
            }
        }
        self.arith_unchecked(b, op, l, r, ty)
    }

    fn arith_unchecked(
        &mut self,
        b: &ast::BinExpr,
        op: ArithOp,
        l: Frag,
        r: Frag,
        ty: &Ty,
    ) -> Result<Frag, (Frag, Frag, String)> {
        let plain = |l: Frag, r: Frag| Err((l, r, op.to_string()));
        let call = |name: &str, l: Frag, r: Frag| {
            let mut f = Frag::text(format!("{name}("));
            f.append(l).push(", ").append(r).push(")");
            f
        };
        match ty {
            Ty::Str if op == ArithOp::Add => plain(l, r),
            Ty::Bool if matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor) => {
                let mut f = Frag::text("Boolean(");
                f.append(l).push(format!(" {op} ")).append(r).push(")");
                Ok(f)
            }
            Ty::Float { f32 } => {
                if matches!(
                    op,
                    ArithOp::Add | ArithOp::Sub | ArithOp::Mul | ArithOp::Div | ArithOp::Rem
                ) {
                    if *f32 {
                        let mut f = Frag::text("Math.fround(");
                        f.append(l).push(format!(" {op} ")).append(r).push(")");
                        Ok(f)
                    } else {
                        plain(l, r)
                    }
                } else {
                    self.error(
                        b.syntax(),
                        format!(
                            "no implementation for `{} {op} {}`",
                            ty.describe(),
                            ty.describe()
                        ),
                    );
                    plain(l, r)
                }
            }
            // `i128` / `u128`: BigInt arithmetic, wrapped to the width.
            Ty::Int(k) if k.is_big() => {
                let signed = k.is_signed();
                let bits = k.bits();
                let mask = bits - 1;
                let wrap = |f: Frag| wrap_big(f, *k);
                let both = |sep: &str| {
                    let mut f = l.clone();
                    f.push(sep).append(r.clone());
                    f
                };
                Ok(match op {
                    ArithOp::Add => wrap(both(" + ")),
                    ArithOp::Sub => wrap(both(" - ")),
                    ArithOp::Mul => wrap(both(" * ")),
                    // A positive literal divisor can't panic: plain BigInt `/` (truncating) and `%`.
                    ArithOp::Div | ArithOp::Rem if nonzero_literal(b.rhs()) => {
                        let sep = if op == ArithOp::Div { " / " } else { " % " };
                        both(sep).wrap("(", ")")
                    }
                    // (`MIN / -1` panics, in release builds too.)
                    ArithOp::Div | ArithOp::Rem => {
                        let name = if op == ArithOp::Div {
                            "div128"
                        } else {
                            "rem128"
                        };
                        let mut f = Frag::text(format!("κσ.${name}("));
                        f.append(l)
                            .push(", ")
                            .append(r)
                            .push(format!(", {signed}, {bits})"));
                        f
                    }
                    ArithOp::BitAnd => both(" & ").wrap("(", ")"),
                    ArithOp::BitOr => both(" | ").wrap("(", ")"),
                    ArithOp::BitXor => both(" ^ ").wrap("(", ")"),
                    // (A literal amount is masked here: `h << 5n`.)
                    ArithOp::Shl | ArithOp::Shr => {
                        let amount = match b.rhs().map(|e| strip_parens_expr(&e)) {
                            Some(ast::Expr::Literal(lit)) if matches!(lit.kind(), LiteralKind::IntNumber(n) if n.value().is_ok()) =>
                            {
                                let LiteralKind::IntNumber(n) = lit.kind() else {
                                    unreachable!()
                                };
                                Frag::text(format!("{}n", n.value().unwrap_or(0) & mask as u128))
                            }
                            _ => {
                                let mut f = Frag::text("(BigInt(");
                                f.append(r.clone()).push(format!(") & {mask}n)"));
                                f
                            }
                        };
                        let mut f = l.clone();
                        f.push(if op == ArithOp::Shl { " << " } else { " >> " })
                            .append(amount);
                        if op == ArithOp::Shl {
                            wrap(f)
                        } else {
                            f.wrap("(", ")")
                        }
                    }
                })
            }
            Ty::Int(_) | Ty::IntLit => {
                let k = ty.int_kind().unwrap();
                let wrap = |f: Frag| match k.wrap_parts() {
                    Some((pre, post)) => f.wrap(pre, post),
                    None => f,
                };
                let narrow = k.is_narrow();
                let res = match op {
                    ArithOp::Add | ArithOp::Sub if narrow => {
                        let mut f = l;
                        f.push(format!(" {op} ")).append(r);
                        wrap(f)
                    }
                    ArithOp::Mul if narrow => {
                        let f = call("Math.imul", l, r);
                        if k == IntKind::I32 { f } else { wrap(f) }
                    }
                    ArithOp::Add | ArithOp::Sub | ArithOp::Mul => return plain(l, r),
                    // A nonzero literal divisor can't panic: plain JS.
                    ArithOp::Div if nonzero_literal(b.rhs()) => {
                        let mut f = Frag::text("Math.trunc(");
                        f.append(l).push(" / ").append(r).push(")");
                        if narrow { wrap(f) } else { f }
                    }
                    ArithOp::Rem if nonzero_literal(b.rhs()) => {
                        let mut f = l;
                        f.push(" % ").append(r);
                        f.wrap("(", ")")
                    }
                    ArithOp::Div => {
                        let f = call("κσ.$idiv", l, r);
                        if narrow { wrap(f) } else { f }
                    }
                    ArithOp::Rem => call("κσ.$irem", l, r),
                    // `x & 1023` on a 64-bit kind: JS's `&` keeps the right low bits of `x`.
                    ArithOp::BitAnd
                        if !narrow
                            && (b.lhs().is_some_and(|e| small_mask_literal(&e))
                                || b.rhs().is_some_and(|e| small_mask_literal(&e))) =>
                    {
                        let mut f = l;
                        f.push(" & ").append(r);
                        f.wrap("(", ")")
                    }
                    // 64-bit kinds: exact through BigInt (within the 2^53 a JS number holds),
                    // with a 32-bit fast path in the runtime.
                    _ if !narrow => {
                        let signed = k.is_signed();
                        let (name, extra) = match op {
                            ArithOp::Shl => ("shl64", format!(", {signed}")),
                            ArithOp::Shr => ("shr64", String::new()),
                            ArithOp::BitAnd => ("bit64", format!(", \"&\", {signed}")),
                            ArithOp::BitOr => ("bit64", format!(", \"|\", {signed}")),
                            _ => ("bit64", format!(", \"^\", {signed}")),
                        };
                        let mut f = Frag::text(format!("κσ.${name}("));
                        f.append(l).push(", ").append(r).push(format!("{extra})"));
                        f
                    }
                    ArithOp::Shl => {
                        let mut f = l;
                        f.push(" << ").append(r);
                        if k == IntKind::I32 { f } else { wrap(f) }
                    }
                    ArithOp::Shr => {
                        let mut f = l;
                        f.push(if k.is_signed() { " >> " } else { " >>> " })
                            .append(r);
                        f
                    }
                    ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor => {
                        let mut f = l;
                        f.push(format!(" {op} ")).append(r);
                        if k == IntKind::U32 { wrap(f) } else { f }
                    }
                };
                Ok(res)
            }
            _ => plain(l, r),
        }
    }

    fn emit_cast(&mut self, c: &ast::CastExpr) -> (Frag, Ty) {
        let (Some(inner), Some(target)) = (c.expr(), c.ty()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        // `&mut x as *mut i32`: the same reference (a cell for numbers).
        if crate::typeck::is_ptr_type(&target) {
            return self.emit_expr(&inner, None);
        }
        // The type checker's type knows trait objects and closure types (`Box<dyn Score>`).
        let to = match self.ty_of(c.syntax()) {
            Some(t @ (Ty::Dyn(_) | Ty::Closure { .. })) => t,
            _ => self.lower_type(&target),
        };
        let (f, from) = self.operand(&inner, None);
        let from = from.default_lit();
        let big = |k: &IntKind| k.is_big();
        let bits = |k: &IntKind| k.bits();
        let out = match (&from, &to) {
            // A number kind's value (an integer within ±2^53) into a BigInt kind: `BigInt(x)`,
            // wrapped when a negative value goes into an unsigned kind (`-1i32 as u64`).
            (Ty::Int(k0), Ty::Int(k))
                if big(k)
                    && !big(k0)
                    && !matches!(
                        strip_parens_expr(&inner),
                        ast::Expr::Literal(_) | ast::Expr::PrefixExpr(_)
                    ) =>
            {
                let f = f.wrap("BigInt(", ")");
                if k0.is_signed() && !k.is_signed() {
                    wrap_big(f, *k)
                } else {
                    f
                }
            }
            // Into and out of the BigInt types.
            (_, Ty::Int(k)) if big(k) => {
                f.wrap("κσ.$to128(", &format!(", {}, {})", k.is_signed(), k.bits()))
            }
            (Ty::Int(k), Ty::Float { f32: true }) if big(k) => f.wrap("Math.fround(Number(", "))"),
            (Ty::Int(k), Ty::Float { .. }) if big(k) => f.wrap("Number(", ")"),
            (Ty::Int(k), Ty::Int(k2)) if big(k) => f.wrap(
                "κσ.$from128(",
                &format!(", {}, {})", bits(k2), k2.is_signed()),
            ),
            (Ty::Int(_), Ty::Int(k)) => match k.wrap_parts() {
                Some((pre, post)) => f.wrap(pre, post),
                None => f,
            },
            (Ty::Float { .. } | Ty::Unknown, Ty::Int(k)) => {
                let (min, max) = k.min_max();
                let mut g = Frag::text("κσ.$f2i(");
                g.append(f).push(format!(", {min}, {max})"));
                g
            }
            (Ty::Bool, Ty::Int(_)) => f.wrap("(", " ? 1 : 0)"),
            (Ty::Adt(n, _), Ty::Int(_)) => self.enum_discriminant(f, n),
            (Ty::Char, Ty::Int(k)) => {
                let g = f.wrap("", ".codePointAt(0)");
                match k.wrap_parts() {
                    Some((pre, post)) => g.wrap(pre, post),
                    None => g,
                }
            }
            (Ty::Int(IntKind::U8), Ty::Char) => f.wrap("String.fromCodePoint(", ")"),
            (Ty::Int(_) | Ty::Float { .. } | Ty::Unknown, Ty::Float { f32: true }) => {
                f.wrap("Math.fround(", ")")
            }
            (Ty::Int(_) | Ty::Float { .. } | Ty::Unknown, Ty::Float { f32: false }) => f,
            (a, b) if a == b => f,
            // `as Box<dyn Trait>` / `as Box<dyn Fn(..)>`: the same JS value.
            (_, Ty::Dyn(_) | Ty::Closure { .. }) => f,
            (a, b) => {
                self.error(
                    c.syntax(),
                    format!(
                        "non-primitive cast: `{}` as `{}`",
                        a.describe(),
                        b.describe()
                    ),
                );
                f
            }
        };
        (out, to)
    }

    /// Tuples are values in Rust but shared arrays in JS. When one is read from a place (a local,
    /// field or element) into a by-value position, copy it so later mutation can't alias.
    fn by_value(&self, e: &ast::Expr, f: Frag, ty: &Ty) -> Frag {
        // `&x` / `&mut x` pass the value itself (a reference), not a copy.
        let mut inner = e.clone();
        while let ast::Expr::ParenExpr(p) = &inner {
            match p.expr() {
                Some(i) => inner = i,
                None => break,
            }
        }
        if matches!(inner, ast::Expr::RefExpr(_)) {
            return f;
        }
        // A `&mut` parameter handed on (as its cell).
        if let ast::Expr::PathExpr(p) = &inner {
            let r = self.range_of(p.syntax());
            if self.types.cell_args.contains(&r) || self.types.slot_args.contains(&r) {
                return f;
            }
        }
        let place = matches!(
            strip_refs(e),
            ast::Expr::PathExpr(_)
                | ast::Expr::FieldExpr(_)
                | ast::Expr::IndexExpr(_)
                | ast::Expr::PrefixExpr(_)
        );
        // (A `[T; N]` of `Copy` values copies too, where a `Vec` would move.)
        let array = self
            .types
            .array_reads
            .contains(&self.range_of(strip_refs(e).syntax()));
        if place && (array || self.copied_on_read(ty)) {
            // (A `Copy` struct's derived `$clone`, without the dispatch in `κσ.$clone`.)
            if !array
                && let Ty::Adt(n, _) = ty
                && self
                    .types
                    .adts
                    .defs
                    .get(n)
                    .is_some_and(|d| d.derives("Clone"))
            {
                return f.wrap("(", ").$clone()");
            }
            Frag::text("κσ.$clone(").append(f).push(")").clone()
        } else {
            f
        }
    }

    /// `d == Dir::S` / `Dir::S != d`: the other operand and the unit variant's index, when the
    /// enum's `PartialEq` is derived (a tag compare is exact).
    fn unit_variant_cmp(&self, lhs: &ast::Expr, rhs: &ast::Expr) -> Option<(ast::Expr, usize)> {
        for (variant, other) in [(rhs, lhs), (lhs, rhs)] {
            let ast::Expr::PathExpr(p) = strip_parens_expr(variant) else {
                continue;
            };
            let segs = adt_gen::path_segments(p.path());
            if segs.len() < 2 {
                continue;
            }
            let Some(crate::adt::Resolved::Ctor(adt, v)) = self.resolve(&segs) else {
                continue;
            };
            let Some(def) = self.types.adts.defs.get(&adt) else {
                continue;
            };
            if def.is_enum
                && def.derives("PartialEq")
                && !def.manual.iter().any(|m| m == "PartialEq")
                && def.variants[v].shape == crate::adt::Shape::Unit
                && self.carried_names(&adt).is_empty()
                && matches!(self.ty_of(other.syntax()), Some(Ty::Adt(n, _)) if n == adt)
            {
                return Some((other.clone(), v));
            }
        }
        None
    }

    /// `b.slice_mut()` / `b.inner()` indexed and assigned to: a call giving a `Vec` or slice,
    /// put in a temp first (`emit_assign_place`).
    fn call_base(&self, base: &ast::Expr) -> bool {
        matches!(
            strip_parens_expr(base),
            ast::Expr::MethodCallExpr(_) | ast::Expr::CallExpr(_)
        ) && self.same_vec_call(base).is_none()
            && matches!(self.ty_of(base.syntax()), Some(Ty::Vec(_)))
    }

    /// The place of an assignment. `grid[i][j] = ..` / `+= ..`: the row goes in a temp first,
    /// where the inline bounds checks would read it several times. So does a call's `Vec` or
    /// slice (`b.slice_mut()[0] = 1`), which mustn't run twice.
    fn emit_assign_place(&mut self, lhs: &ast::Expr) -> (Frag, Ty) {
        if let ast::Expr::IndexExpr(ix) = strip_parens_expr(lhs)
            && let Some(base) = ix.base()
            && (matches!(strip_parens_expr(&base), ast::Expr::IndexExpr(_)) && is_pure(&base)
                || self.call_base(&base))
            && !self
                .types
                .slice_views
                .contains(&self.range_of(strip_parens_expr(&base).syntax()))
            && self.recv_override.is_none()
            && matches!(self.ty_of(base.syntax()), Some(Ty::Vec(_)))
        {
            let (bf, bt) = self.emit_expr(&base, None);
            if let Ty::Vec(_) = bt {
                let t = self.temp();
                let mut decl = Frag::mapped(self.off(base.syntax()), format!("const {t} = "));
                decl.append(bf).push(";");
                self.line(decl);
                self.recv_override = Some((self.range_of(base.syntax()), t, bt));
                let out = self.emit_expr(lhs, None);
                self.recv_override = None;
                return out;
            }
        }
        self.emit_expr(lhs, None)
    }

    fn emit_args(&mut self, args: Option<ast::ArgList>, expected: &[Ty]) -> Vec<Frag> {
        let views = std::mem::take(&mut self.view_args);
        let Some(args) = args else { return vec![] };
        args.args()
            .enumerate()
            .map(|(i, a)| {
                let exp = expected.get(i).cloned();
                // `f(&v[a..b])` into a parameter that can take a view: no copy.
                if views.get(i) == Some(&true) && is_range_of_pure(&a) {
                    self.slice_view = true;
                }
                let f = self.emit_arg(&a, exp.as_ref());
                self.slice_view = false;
                f
            })
            .collect()
    }

    fn emit_arg(&mut self, a: &ast::Expr, expected: Option<&Ty>) -> Frag {
        let exp = expected.cloned();
        let (f, ty) = self.emit_expr(a, exp.as_ref());
        let f = self.by_value(a, f, &ty);
        // (`impl Into<X>` parameter: `X::from(arg)`.)
        match self.conv_of(a.syntax()) {
            Some(conv) if self.types.into_args.contains(&self.range_of(a.syntax())) => {
                self.convert(conv, f)
            }
            _ => f,
        }
    }

    /// The place of a `&mut place` argument that would otherwise be passed as a getter/setter
    /// cell (`&mut self.n`, `&mut v[i]`, `&mut local`): it's pure (no side effects, cheap), so
    /// it can be read into a temp and written back.
    fn write_back_place(&self, a: &ast::Expr) -> Option<ast::Expr> {
        let ast::Expr::RefExpr(r) = strip_parens_expr(a) else {
            return None;
        };
        if !self.types.slot_args.contains(&self.range_of(r.syntax())) {
            return None;
        }
        let inner = strip_parens_expr(&r.expr()?);
        (matches!(
            inner,
            ast::Expr::PathExpr(_) | ast::Expr::FieldExpr(_) | ast::Expr::IndexExpr(_)
        ) && is_pure(&inner))
        .then_some(inner)
    }

    /// The `&mut place` arguments of a call that can be write-back temps (`write_back_call`),
    /// when the callee can't keep the borrow (`FnSig::writeback` / `wb_methods`).
    fn write_back_places(&self, args: Option<ast::ArgList>) -> Option<Vec<Option<ast::Expr>>> {
        let places: Vec<Option<ast::Expr>> = args
            .map(|l| l.args().map(|a| self.write_back_place(&a)).collect())
            .unwrap_or_default();
        places.iter().any(Option::is_some).then_some(places)
    }

    /// `f(&mut self.n, x)` with `f` unable to keep the borrow (`FnSig::writeback`): instead of a
    /// getter/setter cell, the place is read into a one-element array that `f` reads and writes,
    /// and written back when `f` returns or panics (Rust's borrow rules keep anything else from
    /// seeing the place meanwhile; a panic that unwinds through a `Drop` sees the written value,
    /// as in Rust):
    /// `(($wb0, $wb1) => { try { return f($wb0, $wb1); } finally { this.n = $wb0[0]; } })([this.n], x)`.
    /// The arguments are evaluated outside, in order, so an `await` in one stays in its function.
    /// (V8 inlines the arrow; about 50x faster than `κσ.$cell`.) `callee` is `f` or `recv.m`;
    /// `node` is the call, for its dictionary and `{:?}` arguments.
    fn write_back_call(
        &mut self,
        node: &SyntaxNode,
        arg_list: Option<ast::ArgList>,
        callee: Frag,
        params: &[Ty],
        places: Vec<Option<ast::Expr>>,
    ) -> Frag {
        let args: Vec<ast::Expr> = arg_list.map(|l| l.args().collect()).unwrap_or_default();
        let mut outer = Vec::new();
        let mut names = Vec::new();
        let mut writes = Frag::text("");
        for (i, (a, place)) in args.iter().zip(places).enumerate() {
            let name = format!("$wb{i}");
            match place {
                Some(p) => {
                    let (pf, _) = self.emit_expr(&p, None);
                    outer.push(pf.clone().wrap("[", "]"));
                    writes.push(" ").append(pf).push(format!(" = {name}[0];"));
                }
                None => outer.push(self.emit_arg(a, params.get(i))),
            }
            names.push(name);
        }
        let mut inner: Vec<Frag> = names.iter().map(|n| Frag::text(n.clone())).collect();
        if let Some(tys) = self.types.dict_args.get(&self.range_of(node)).cloned() {
            for t in &tys {
                inner.push(Frag::text(self.dict_of(t)));
            }
        }
        inner.extend(self.dbg_arg_frags(node));
        let mut f = Frag::text(format!("(({}) => {{ try {{ return ", names.join(", ")));
        f.append(callee)
            .push("(")
            .append(Frag::join(inner, ", "))
            .push("); } finally {")
            .append(writes)
            .push(" } })(")
            .append(Frag::join(outer, ", "))
            .push(")");
        f
    }

    /// A call of the function `name` (local or imported).
    fn emit_fn_call(&mut self, c: &ast::CallExpr, name: &str, callee_off: usize) -> (Frag, Ty) {
        let Some(sig) = self.fns.get(name) else {
            if name == "drop" {
                let args = self.emit_args(c.arg_list(), &[]);
                let mut f = Frag::mapped(callee_off, "κσ.$drop(");
                f.append(Frag::join(args, ", ")).push(")");
                return (f, Ty::Unit);
            }
            let callee = c
                .expr()
                .map(|e| e.syntax().clone())
                .unwrap_or_else(|| c.syntax().clone());
            // (The type checker reported it, with the closer name or the std item.)
            if !self.type_errors {
                self.error(
                    &callee,
                    format!("cannot find function `{name}` in this scope"),
                );
            }
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let (js, params, ret, is_async, writeback) = (
            sig.js.clone(),
            sig.params.clone(),
            sig.ret.clone(),
            sig.is_async,
            sig.writeback,
        );
        let n_args = c.arg_list().map(|a| a.args().count()).unwrap_or(0);
        if n_args != params.len() {
            self.error(
                c.syntax(),
                format!(
                    "this function takes {} argument(s) but {} were supplied",
                    params.len(),
                    n_args
                ),
            );
        }
        // `f(&mut self.n)`: the place goes in a write-back temp (see `write_back_call`).
        if writeback && let Some(places) = self.write_back_places(c.arg_list()) {
            let callee = Frag::mapped(callee_off, js);
            let f = self.write_back_call(c.syntax(), c.arg_list(), callee, &params, places);
            let ty = if is_async {
                Ty::Future(Box::new(ret))
            } else {
                ret
            };
            return (f, ty);
        }
        self.view_args = self.view_params.get(name).cloned().unwrap_or_default();
        let mut args = self.emit_args(c.arg_list(), &params);
        // The classes a generic function takes for its `T::make()` calls.
        if let Some(tys) = self
            .types
            .dict_args
            .get(&self.range_of(c.syntax()))
            .cloned()
        {
            for t in &tys {
                args.push(Frag::text(self.dict_of(t)));
            }
        }
        args.extend(self.dbg_arg_frags(c.syntax()));
        let mut f = Frag::mapped(callee_off, js);
        f.push("(").append(Frag::join(args, ", ")).push(")");
        let ty = if is_async {
            Ty::Future(Box::new(ret))
        } else {
            ret
        };
        (f, ty)
    }

    /// The JS class a two-segment path `T::x` starts from: `$T` for a type parameter whose class
    /// the function takes, or `this` / `this.constructor` for `Self` in a trait's default method.
    fn param_class(&self, p: &ast::PathExpr) -> Option<String> {
        let path = p.path()?;
        let q = path.qualifier()?;
        if q.qualifier().is_some() {
            return None;
        }
        let first = q.segment()?.name_ref()?.text().to_string();
        if self.dict_scope.contains(&first) {
            return Some(format!("${first}"));
        }
        // (A blanket impl's associated function: `T::make()` / `Self::make()` on `this`.)
        if let Some(t) = &self.blanket_this
            && (first == *t || first == "Self")
        {
            return Some("this".into());
        }
        // (`Self::make()` in a method of `$ext$i32`: its class, `this` in a static one.)
        if first == "Self"
            && let Some(k) = &self.ext_class
        {
            return Some(if self.lookup("self").is_some() {
                format!("$ext${k}")
            } else {
                "this".into()
            });
        }
        // (An `impl<T>`'s `T` in a method with `self`: `this.$T`.)
        if let Some(js) = const_js_of(&first) {
            return Some(js);
        }
        if first == "Self" && self.in_trait_default {
            return Some(
                if self.lookup("self").is_some() {
                    "this.constructor"
                } else {
                    "this"
                }
                .into(),
            );
        }
        None
    }

    /// The `{:?}` formatters a generic call passes (`$dbg$T` of the callee), by its range.
    pub(super) fn dbg_arg_frags(&self, node: &SyntaxNode) -> Vec<Frag> {
        self.types
            .dbg_args
            .get(&self.range_of(node))
            .map(|tys| {
                tys.iter()
                    .map(|t| Frag::text(debug_fn(t).unwrap_or_else(|| "String".into())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `$ext$Make$all.make.call(<ty's class>, ..)`: a blanket impl's associated function, with
    /// the type's class as `this` (its `T` and `Self`).
    fn emit_ext_dict_call(
        &mut self,
        c: &ast::CallExpr,
        key: &str,
        dict: &Ty,
        f: &str,
    ) -> (Frag, Ty) {
        let sig = self.types.adts.method(key, f).cloned();
        let params = sig.as_ref().map(|s| s.params.clone()).unwrap_or_default();
        let ty = self
            .ty_of(c.syntax())
            .unwrap_or_else(|| sig.map(|s| s.ret).unwrap_or(Ty::Unknown));
        let args = self.emit_args(c.arg_list(), &params);
        let mut out = Frag::mapped(
            self.off(c.syntax()),
            format!(
                "$ext${key}.{}.call({}",
                adt_gen::prop(f),
                self.dict_of(dict)
            ),
        );
        for a in args {
            out.push(", ").append(a);
        }
        out.push(")");
        (out, ty)
    }

    /// Whether some blanket impl has associated functions, which a type's class then needs
    /// (`T::make()` for a `T` only the blanket impl gives `make`).
    fn blanket_statics(&self) -> bool {
        self.types.adts.blankets.keys().any(|tr| {
            self.types
                .adts
                .methods
                .get(&format!("{tr}$all"))
                .is_some_and(|ms| ms.values().any(|m| m.receiver.is_none()))
        })
    }

    /// The class (or built-in stand-in) passed for a type parameter's `T::make()` calls, with
    /// the blanket impls' associated functions it lacks (`κσ.$bdict`).
    fn dict_of(&self, t: &Ty) -> String {
        let d = self.dict_of_own(t);
        let wrap = match t {
            Ty::ConstVal(_) => false,
            Ty::Param(_) => false,
            Ty::Adt(n, _) => self.types.adts.blankets.keys().any(|tr| {
                self.types
                    .adts
                    .methods
                    .get(&format!("{tr}$all"))
                    .into_iter()
                    .flat_map(|ms| ms.iter())
                    .any(|(f, m)| m.receiver.is_none() && self.types.adts.method(n, f).is_none())
            }),
            _ => true,
        };
        if wrap && d != "undefined" && self.blanket_statics() {
            format!("κσ.$bdict($extRegs, {d})")
        } else {
            d
        }
    }

    fn dict_of_own(&self, t: &Ty) -> String {
        match t {
            // (A blanket impl's `T` in its associated functions: their `this`.)
            Ty::Param(p) if self.blanket_this.as_deref() == Some(p.as_str()) => "this".into(),
            // A const argument: its value.
            Ty::ConstVal(v) => self.const_js(v),
            Ty::Param(p) if const_js_of(p).is_some() => const_js_of(p).unwrap(),
            Ty::Adt(n, args) => {
                // (With the classes its values carry: `Default::default()` makes one.)
                let stored: Vec<(usize, String)> = self
                    .types
                    .stored
                    .get(n)
                    .into_iter()
                    .flatten()
                    .filter_map(|&i| Some((i, self.dict_of(args.get(i)?))))
                    .collect();
                class_view(self.class(n), args, &stored)
            }
            Ty::Param(p) if self.dict_scope.contains(p) => format!("${p}"),
            // (The number kind: generic `/` truncates for integers, results wrap.)
            // (And your trait impls' associated functions, `T::zero()`.)
            Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => {
                let name = match t {
                    Ty::IntLit => "i32".to_string(),
                    other => other.describe(),
                };
                match crate::adt::ext_key(&name).filter(|k| self.types.adts.ext_types.contains(*k))
                {
                    Some(key) => format!("κσ.$numT(\"{name}\", $ext${key})"),
                    None => format!("κσ.$numT(\"{name}\")"),
                }
            }
            Ty::Str => "κσ.$prims.str".into(),
            Ty::Bool => "κσ.$prims.bool".into(),
            Ty::Char => "κσ.$prims.char".into(),
            // (With the element's class when it has one, for `impl<T> S<Vec<T>>`.)
            Ty::Vec(e) | Ty::Option(e) => {
                let prim = if matches!(t, Ty::Vec(_)) { "vec" } else { "option" };
                match self.dict_of(e) {
                    d if d == "undefined" => format!("κσ.$prims.{prim}"),
                    d => format!("κσ.$withElem(κσ.$prims.{prim}, {d})"),
                }
            }
            // (A tuple's elements', for its impl's methods: see `κσ.$dcallT`.)
            Ty::Tuple(es) if !es.is_empty() => format!(
                "κσ.$tupT([{}])",
                es.iter().map(|e| self.dict_of(e)).collect::<Vec<_>>().join(", ")
            ),
            // (`()`: `N::default()` for `Graph::<(), u32>::from_edges`'s new nodes.)
            Ty::Unit => "κσ.$prims.unit".into(),
            _ => "undefined".into(),
        }
    }

    /// A const argument as written in Rust (`3`, `true`, `'a'`, `SIZE`) in JS.
    fn const_js(&self, v: &str) -> String {
        self.consts
            .get(v)
            .cloned()
            .unwrap_or_else(|| const_value_js(v))
    }

    /// `const N: usize` parameters of `f`, with its `impl`'s (`outer`: name, JS, type): values
    /// in the body, and what `Buf<N>` passes on. Returns the lines that read a passed one off
    /// an argument when the caller couldn't say it (`$N ??= a?.length;`).
    pub(super) fn enter_consts(
        &mut self,
        f: &ast::Fn,
        outer: Vec<(String, String, Ty)>,
    ) -> Vec<String> {
        let mut consts = outer;
        consts.extend(self.extra_consts.clone());
        for p in f
            .generic_param_list()
            .into_iter()
            .flat_map(|l| l.generic_params())
        {
            if let ast::GenericParam::ConstParam(cp) = p
                && let Some(n) = cp.name()
            {
                let ty = cp.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown);
                let n = n.text().to_string();
                consts.push((n.clone(), format!("${n}"), ty));
            }
        }
        let mut lines = Vec::new();
        // An `IntoIterator` parameter: whatever iterable came in, as the runtime's iterator.
        for p in f.param_list().into_iter().flat_map(|pl| pl.params()) {
            if let Some(ast::Pat::IdentPat(ip)) = p.pat()
                && let Some(n) = ip.name()
                && let Some(local) = self.lookup(n.text().to_string().as_str())
                && matches!(local.ty, Ty::IntoIter(_))
            {
                lines.push(format!("{0} = κσ.$intoIter({0});", local.js));
            }
        }
        // An `R: Read` parameter given bytes (`&[u8]`): a `Cursor` over them, which reads.
        for p in f.param_list().into_iter().flat_map(|pl| pl.params()) {
            if let Some(ast::Pat::IdentPat(ip)) = p.pat()
                && let Some(n) = ip.name()
                && p.ty().is_some_and(|t| byte_reader(f, &t))
                && let Some(local) = self.lookup(n.text().to_string().as_str())
            {
                lines.push(format!(
                    "if (Array.isArray({0})) {0} = IoCursor.new({0});",
                    local.js
                ));
            }
        }
        for (n, js, ty) in consts {
            if js.starts_with('$') && !js.contains('(') {
                let from = f
                    .param_list()
                    .into_iter()
                    .flat_map(|pl| pl.params())
                    .find_map(|p| {
                        let suffix = crate::adt::const_path(&p.ty()?, &n, &self.types.adts)?;
                        let ast::Pat::IdentPat(ip) = p.pat()? else {
                            return None;
                        };
                        let local = self.lookup(ip.name()?.text().to_string().as_str())?;
                        Some(format!("{}{suffix}", local.js))
                    });
                if let Some(from) = from {
                    lines.push(format!("{js} ??= {from};"));
                }
            }
            CONST_SCOPE.with(|s| s.borrow_mut().insert(n.clone(), js.clone()));
            // An `i128` / `u128` parameter is a BigInt in the body, whichever number came in.
            let js = match ty {
                Ty::Int(k) if k.is_big() => format!("BigInt({js})"),
                _ => js,
            };
            self.declare(&n, None, Local { js, ty });
        }
        lines
    }

    /// The const arguments of a struct value's type (`Buf<4>` → `4`), for its constructor.
    /// Also the classes its values carry for type parameters (`u8`'s, for `this.$T`).
    pub(super) fn const_args_of(&self, node: &SyntaxNode, adt: &str) -> Vec<Frag> {
        let names = self.carried_names(adt);
        if names.is_empty() {
            return Vec::new();
        }
        let args = match self.ty_of(node) {
            Some(Ty::Adt(_, args)) => args,
            Some(Ty::Closure { ret, .. }) => match *ret {
                Ty::Adt(_, args) => args,
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        let params = &self.types.adts.defs[adt].type_params;
        names
            .iter()
            .map(|n| {
                let a = params.iter().position(|p| p == n).and_then(|i| args.get(i));
                Frag::text(a.map_or_else(|| "undefined".into(), |a| self.dict_of(a)))
            })
            .collect()
    }

    /// `crate::util::x` / `util::x` → `x` when `util` is a loaded module.
    fn strip_modules(&self, segs: &[String]) -> Vec<String> {
        let mut i = 0;
        while i + 1 < segs.len()
            && (matches!(segs[i].as_str(), "crate" | "self")
                || self.types.adts.modules.contains(&segs[i]))
        {
            i += 1;
        }
        if let Some(key) = typeck::qualified_key(segs, i)
            && (self.fns.contains_key(&key) || self.consts.contains_key(&key))
        {
            return vec![key];
        }
        segs[i..].to_vec()
    }

    /// A bundled crate's call (`κσ.$rand.gen(rng, κσ.$numT("u8"))`): the receiver, the
    /// arguments (`gen_range`'s range as its two ends and whether it's inclusive), then the
    /// classes of the types it records.
    fn emit_crate_call(
        &mut self,
        node: &SyntaxNode,
        recv: Option<&ast::Expr>,
        args: Vec<ast::Expr>,
    ) -> Option<(Frag, Ty)> {
        use ra_ap_syntax::ast::RangeItem;
        let cc = self.types.crate_calls.get(&self.range_of(node))?.clone();
        // `rng.gen::<Dir>()` with your `impl Distribution<Dir> for Standard`.
        if cc.js == "rand.gen"
            && matches!(cc.tys.first(), Some(Ty::Adt(..)))
            && self.types.adts.defs.contains_key("RandStandard")
            && let Some(r) = recv
        {
            let mut f = Frag::mapped(self.off(node), "new RandStandard().sample(");
            f.append(self.emit_expr(r, None).0).push(")");
            return Some((f, self.ty_of(node).unwrap_or(Ty::Unknown)));
        }
        let mut parts = Vec::new();
        if let Some(r) = recv {
            parts.push(self.emit_expr(r, None).0);
        }
        for a in &args {
            match crate::typeck::strip_parens(a) {
                ast::Expr::RangeExpr(r)
                    if matches!(cc.js.as_str(), "rand.range" | "rand.randomRange")
                        && r.start().is_some()
                        && r.end().is_some() =>
                {
                    let elem = cc.tys.first().cloned();
                    parts.push(self.emit_expr(&r.start().unwrap(), elem.as_ref()).0);
                    parts.push(self.emit_expr(&r.end().unwrap(), elem.as_ref()).0);
                    let incl = r.op_kind() == Some(RangeOp::Inclusive);
                    parts.push(Frag::text(incl.to_string()));
                }
                _ => parts.push(self.emit_expr(a, None).0),
            }
        }
        for t in &cc.tys {
            parts.push(Frag::text(self.dict_of(t)));
        }
        // (`tuple_windows()`, `collect_tuple()`: the tuple's size.)
        if let Some(Ty::Tuple(ts)) = cc.tys.first()
            && cc.js.starts_with("itertools.")
        {
            parts.pop();
            parts.push(Frag::text(ts.len().to_string()));
        }
        let mut f = Frag::mapped(self.off(node), format!("κσ.${}(", cc.js));
        f.append(Frag::join(parts, ", ")).push(")");
        Some((f, self.ty_of(node).unwrap_or(Ty::Unknown)))
    }

    fn emit_call(&mut self, c: &ast::CallExpr) -> (Frag, Ty) {
        if let Some(out) = self.emit_crate_call(
            c.syntax(),
            None,
            c.arg_list().map(|l| l.args().collect()).unwrap_or_default(),
        ) {
            return out;
        }
        // `T::make(..)` with `T`'s class at hand (`$T.make(..)`), `Self::make(..)` in a trait's
        // default method.
        // (`T::len(x)` with a `self` method: a method call on `x`, below.)
        if let Some(ast::Expr::PathExpr(p)) = c.expr()
            && !self
                .types
                .qualified_calls
                .contains_key(&self.range_of(c.syntax()))
            && let Some(class) = self.param_class(&p)
            && let Some(f) = p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
        {
            let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
            let args = self.emit_args(c.arg_list(), &[]);
            // `T::from(x)` with `T: From<X>`: the class's conversion from `X`.
            // (`T::try_from(x)` with `T: TryFrom<X>`: its `try_from`, or `from` as `Ok`.)
            if let Some(src) = self.types.param_froms.get(&self.range_of(c.syntax())) {
                let helper = if f.text() == "try_from" {
                    "ttryfrom"
                } else {
                    "tfrom"
                };
                let mut out = Frag::text(format!("κσ.${helper}({class}, {}, ", js_string(src)));
                out.append(Frag::join(args, ", ")).push(")");
                return (out.at(self.off(c.syntax())), ty);
            }
            let mut out = Frag::text(class);
            out.push(member(f.text()))
                .push("(")
                .append(Frag::join(args, ", "))
                .push(")");
            return (out.at(self.off(c.syntax())), ty);
        }
        // `Pilot::fly(&h)` → `h.Pilot$fly()`; `<Human as Wizard>::name()` → `Human.Wizard$name()`.
        if let Some(q) = self
            .types
            .qualified_calls
            .get(&self.range_of(c.syntax()))
            .cloned()
        {
            let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
            // `<i32 as Make>::make()`: your impl for the built-in type, or a blanket impl's.
            match self.conv_of(c.syntax()) {
                Some(crate::adt::Conv::ExtDict { key, dict }) => {
                    let (f, _) = self.emit_ext_dict_call(c, &key, &dict, &q.method);
                    return (f, ty);
                }
                Some(crate::adt::Conv::Ext { key }) if !q.receiver => {
                    let args = self.emit_args(c.arg_list(), &[]);
                    let mut out = Frag::mapped(
                        self.off(c.syntax()),
                        format!("$ext${key}.{}(", adt_gen::prop(&q.method)),
                    );
                    out.append(Frag::join(args, ", ")).push(")");
                    return (out, ty);
                }
                // `Shout::shout(&5)`: `$ext$Shout$all.prototype.shout.call(5, ..)`.
                Some(crate::adt::Conv::Ext { key }) => {
                    let all: Vec<ast::Expr> =
                        c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
                    let Some(recv) = all.first() else {
                        return (Frag::text("undefined"), ty);
                    };
                    let sig = self.types.adts.method(&key, &q.method).cloned();
                    let recv = strip_refs(recv);
                    let (rf, rt) = self.emit_expr(&recv, None);
                    let rf = if sig
                        .as_ref()
                        .is_some_and(|s| adt_gen::cell_self(s, &adt_gen::ext_self_ty(&key)))
                    {
                        self.recv_cell(&recv, rf)
                    } else if sig
                        .as_ref()
                        .is_some_and(|s| s.receiver == Some(crate::adt::Receiver::Value))
                    {
                        self.by_value(&recv, rf, &rt)
                    } else {
                        rf
                    };
                    let params = sig.map(|s| s.params).unwrap_or_default();
                    let mut parts = vec![rf];
                    for (a, p) in all
                        .iter()
                        .skip(1)
                        .zip(params.iter().chain(std::iter::repeat(&Ty::Unknown)))
                    {
                        let (f, t) = self.emit_expr(a, Some(p));
                        parts.push(self.by_value(a, f, &t));
                    }
                    if key.ends_with("$all") {
                        parts.push(Frag::text(debug_fn(&rt).unwrap_or_else(|| "String".into())));
                    }
                    let mut out = Frag::mapped(
                        self.off(c.syntax()),
                        format!("$ext${key}.prototype.{}.call(", adt_gen::prop(&q.method)),
                    );
                    out.append(Frag::join(parts, ", ")).push(")");
                    return (out, ty);
                }
                _ => {}
            }
            let key = if q.alias {
                format!("{}${}", q.trait_name, q.method)
            } else {
                q.method.clone()
            };
            let args: Vec<ast::Expr> = c.arg_list().map(|a| a.args().collect()).unwrap_or_default();
            let mut rest = Vec::new();
            let mut head = match (&q.adt, q.receiver) {
                (_, true) => {
                    let (f, _) = match args.first() {
                        Some(a) => self.receiver(&strip_refs(a)),
                        None => (Frag::text("undefined"), Ty::Unknown),
                    };
                    f
                }
                (Some(adt), false) => Frag::text(self.class(adt)),
                // `<Self as Trait>::f()`: the class is `this` in a static method.
                (None, false) if q.on_self => Frag::text(if self.lookup("self").is_some() {
                    "this.constructor"
                } else {
                    "this"
                }),
                (None, false) => Frag::text("undefined"),
            };
            for a in args.iter().skip(usize::from(q.receiver)) {
                let (f, t) = self.emit_expr(a, None);
                rest.push(self.by_value(a, f, &t));
            }
            head.push(member(&key))
                .push("(")
                .append(Frag::join(rest, ", "))
                .push(")");
            return (head.at(self.off(c.syntax())), ty);
        }
        let Some(ast::Expr::PathExpr(callee)) = c.expr() else {
            // `make_adder(1)(2)`: the callee evaluates to a JS function.
            let Some(e) = c.expr() else {
                return (Frag::text("undefined"), Ty::Unknown);
            };
            let (mut f, _) = self.receiver(&e);
            let args = self.emit_args(c.arg_list(), &[]);
            f.push("(").append(Frag::join(args, ", ")).push(")");
            return (f, Ty::Unknown);
        };
        let path = callee.path().unwrap();
        let callee_off = self.off(callee.syntax());
        let is_some = match path.as_single_name_ref() {
            Some(n) => {
                n.text() == "Some"
                    && self.lookup("Some").is_none()
                    && !self.fns.contains_key("Some")
            }
            None => path_is(Some(path.clone()), "Some"),
        };
        if is_some {
            // `Some(x)` is `x` itself, or a marker when `x` is an `Option` or `()`.
            let first = c.arg_list().and_then(|l| l.args().next());
            let nullable = first
                .as_ref()
                .and_then(|a| self.ty_of(a.syntax()))
                .is_some_and(|t| nullable_payload(&t));
            let arg = self
                .emit_args(c.arg_list(), &[])
                .into_iter()
                .next()
                .unwrap_or_else(|| Frag::text("undefined"));
            if nullable {
                return (
                    arg.wrap("κσ.$some(", ")"),
                    Ty::Option(Box::new(Ty::Unknown)),
                );
            }
            return (arg.wrap("(", ")"), Ty::Option(Box::new(Ty::Unknown)));
        }
        let std_alias = path.as_single_name_ref().and_then(|n| {
            let n = n.text().to_string();
            (self.lookup(&n).is_none() && !self.fns.contains_key(&n))
                .then(|| self.types.std_fns.get(&n).cloned())
                .flatten()
        });
        if let Some(name) = path.as_single_name_ref()
            && std_alias.is_none()
        {
            // A local holding a closure or function (the type checker rejects other locals).
            if let Some(local) = self.lookup(name.text()).cloned() {
                let args = self.emit_args(c.arg_list(), &[]);
                let mut f = Frag::mapped(callee_off, local.js);
                f.push("(").append(Frag::join(args, ", ")).push(")");
                return (f, Ty::Unknown);
            }
            if !self.fns.contains_key(name.text().to_string().as_str())
                && let Some(r) = self.emit_adt_call(c, &[name.text().to_string()])
            {
                return r;
            }
            if let Some(r) = self.emit_js_bound(c) {
                return r;
            }
            if !self.fns.contains_key(name.text().to_string().as_str())
                && let Some(r) = self.emit_swb_call(c, &[&*name.text().to_string()])
            {
                return r;
            }
            return self.emit_fn_call(c, name.text().to_string().as_str(), callee_off);
        }
        if let Some(r) = self.emit_array_try_from(c, &path) {
            return r;
        }
        let segs: Vec<String> = std_alias.unwrap_or_else(|| {
            path.segments()
                .filter_map(|s| s.name_ref())
                .map(|n| n.text().to_string())
                .collect()
        });
        // (`std::vec::Vec::new()`: as `Vec::new()`.)
        let segs = crate::ty::strip_prelude_modules(segs);
        // `i32::make()` through a blanket impl.
        if let Some(crate::adt::Conv::ExtDict { key, dict }) = self.conv_of(c.syntax())
            && let Some(f) = segs.last()
        {
            return self.emit_ext_dict_call(c, &key, &dict, f);
        }
        if let [single] = self.strip_modules(&segs).as_slice()
            && self.fns.contains_key(single)
        {
            return self.emit_fn_call(c, single, callee_off);
        }
        // `TokenStream::from_iter(tokens)`
        if let [.., ty, f] = segs.as_slice()
            && ty == "TokenStream"
            && f == "from_iter"
            && self.types.adts.defs.contains_key("TokenStream")
        {
            let a = match c.arg_list().and_then(|l| l.args().next()) {
                Some(a) => self.emit_expr(&a, None).0,
                None => Frag::text("undefined"),
            };
            let text = a.wrap("κσ.$tokensJoin(", ")");
            return (
                self.construct("TokenStream", 0, vec![text]),
                Ty::Adt("TokenStream".into(), vec![]),
            );
        }
        if let Some(r) = self.emit_extern_enum_call(c, &segs) {
            return r;
        }
        if let Some(r) = self.emit_adt_call(c, &segs) {
            return r;
        }
        {
            let ss: Vec<&str> = segs.iter().map(String::as_str).collect();
            // `i32::zero()`: the trait impl's associated function on the built-in type.
            if let (Some(crate::adt::Conv::Ext { key }), [_, f]) =
                (self.conv_of(c.syntax()), ss.as_slice())
            {
                let sig = self.types.adts.method(&key, f).cloned();
                let params = sig.as_ref().map(|s| s.params.clone()).unwrap_or_default();
                let ret = sig.map(|s| s.ret).unwrap_or(Ty::Unknown);
                let args = self.emit_args(c.arg_list(), &params);
                let mut out = Frag::mapped(
                    self.off(c.syntax()),
                    format!("$ext${key}.{}(", adt_gen::prop(f)),
                );
                out.append(Frag::join(args, ", ")).push(")");
                return (out, ret);
            }
            if let Some(r) = self.emit_js_bound(c) {
                return r;
            }
            if let Some(r) = self.emit_js_call(c, &ss) {
                return r;
            }
            if let Some(r) = self.emit_swb_call(c, &ss) {
                return r;
            }
            if let Some(r) = self.emit_iface_call(c, &ss) {
                return r;
            }
            if let Some(r) = self.emit_std_call(c, &ss) {
                return r;
            }
            if let Some(r) = self.emit_gaps_call(c, &ss) {
                return r;
            }
        }
        let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
        let tail = match segs.as_slice() {
            ["std" | "core", rest @ ..] => rest,
            all => all,
        };
        if let Some(r) = self.emit_coll_call(c, tail) {
            return r;
        }
        match tail {
            ["String", "new"] => (Frag::text("\"\""), Ty::Str),
            [.., "HashMap", "new" | "with_capacity"] => (
                Frag::text("new κσ.HashMap()"),
                Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
            ),
            [.., "HashMap", "from"] => {
                let a = self.emit_args(c.arg_list(), &[]);
                let arg = a.into_iter().next().unwrap_or_else(|| Frag::text("[]"));
                (
                    Frag::text("κσ.HashMap.from(").append(arg).push(")").clone(),
                    Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown)),
                )
            }
            [.., "HashSet", "new" | "with_capacity"] => (
                Frag::text("new κσ.HashSet()"),
                Ty::Set(Box::new(Ty::Unknown)),
            ),
            [.., "HashSet", "from"] => {
                let a = self.emit_args(c.arg_list(), &[]);
                let arg = a.into_iter().next().unwrap_or_else(|| Frag::text("[]"));
                (
                    Frag::text("κσ.HashSet.from(").append(arg).push(")").clone(),
                    Ty::Set(Box::new(Ty::Unknown)),
                )
            }
            // Capacity is only a hint; JS arrays grow on their own.
            ["Vec" | "vec", "new" | "with_capacity"] => {
                (Frag::text("[]"), Ty::Vec(Box::new(Ty::Unknown)))
            }
            ["Vec" | "vec", "from"] => {
                let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
                let Some(arg) = c.arg_list().and_then(|l| l.args().next()) else {
                    return (Frag::text("[]"), ty);
                };
                let (f, at) = self.emit_expr(&arg, None);
                let f = if at == Ty::Str {
                    f.wrap("κσ.$utf8(", ")")
                } else {
                    f.wrap("κσ.$clone(", ")")
                };
                (f, ty)
            }
            ["String", "from"] => {
                let a = self.emit_args(c.arg_list(), &[Ty::Str]);
                let a = a.into_iter().next().unwrap_or_else(|| Frag::text("\"\""));
                // Through your `impl From<Id> for String`.
                let a = match self.conv_of(c.syntax()) {
                    Some(conv @ crate::adt::Conv::From { .. }) if matches!(&conv, crate::adt::Conv::From { adt, .. } if adt == "String") => {
                        self.convert(conv, a)
                    }
                    _ => a,
                };
                (a, Ty::Str)
            }
            ["cmp", f @ ("max" | "min")] => {
                let a = self.emit_args(c.arg_list(), &[]);
                let ty = self.ty_of(c.syntax()).unwrap_or(Ty::Unknown);
                // Numbers compare as numbers; anything else by `Ord` (strings, tuples, structs).
                let callee = if ty.is_numeric() && !matches!(ty, Ty::Int(k) if k.is_big()) {
                    format!("Math.{f}(")
                } else {
                    format!("κσ.${f}(")
                };
                let mut fr = Frag::text(callee);
                fr.append(Frag::join(a, ", ")).push(")");
                (fr, ty)
            }
            [ty_name, "from"] if Ty::from_name(ty_name).is_numeric() => {
                let ty = Ty::from_name(ty_name);
                let a = self.emit_args(c.arg_list(), &[]);
                (a.into_iter().next().unwrap_or_else(|| Frag::text("0")), ty)
            }
            // A `js` type's missing associated function: the type checker said so.
            [.., t, _] if bindings::is_js_type(t) => (Frag::text("undefined"), Ty::Unknown),
            _ => {
                if !self.type_errors {
                    self.error(
                        callee.syntax(),
                        format!(
                            "cannot find function `{}` in this scope",
                            path.syntax().text()
                        ),
                    );
                }
                (Frag::text("undefined"), Ty::Unknown)
            }
        }
    }

    fn emit_field(&mut self, fe: &ast::FieldExpr) -> (Frag, Ty) {
        let (Some(recv), Some(name)) = (fe.expr(), fe.name_ref()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let (rf, rt) = self.receiver(&recv);
        if let Ty::Adt(n, _) = &rt {
            return self.emit_adt_field(rf, n, &name);
        }
        if let Some(i) = name.as_tuple_field() {
            if let Ty::Tuple(es) = &rt {
                let ty = es.get(i).cloned().unwrap_or(Ty::Unknown);
                let mut f = rf;
                f.append(Frag::mapped(self.off(name.syntax()), format!("[{i}]")));
                return (f, ty);
            }
            self.unsupported(fe.syntax(), "fields of this type");
        }
        match rt {
            Ty::Extern(type_name) => {
                let js = extern_js_name(&type_name, name.text(), false);
                let mut f = rf;
                f.append(Frag::mapped(self.off(name.syntax()), member(&js)));
                // `#[array]`: an array-like property (`children`).
                let b = bindings::current();
                let array = b
                    .types
                    .get(&type_name)
                    .and_then(|t| t.fields.get(&bare(name.text())))
                    .is_some_and(|fd| fd.array);
                let f = if array {
                    f.wrap("κσ.$arrayOf(", ")")
                } else {
                    f
                };
                // (A `HashMap` field is a JS `Map` or object.)
                let declared = b
                    .types
                    .get(&type_name)
                    .and_then(|t| t.fields.get(&bare(name.text())))
                    .map(|fd| fd.ty.clone());
                let f = match &declared {
                    Some(t) => iface_gen::from_js(f, t),
                    None => f,
                };
                (f, Ty::Unknown)
            }
            Ty::Unknown => {
                let mut f = rf;
                f.push(format!(".{}", name.text()));
                (f, Ty::Unknown)
            }
            // A range value's `start` / `end` (the runtime's range iterator has them).
            Ty::Iter { elem, .. }
                if matches!(name.text(), "start" | "end")
                    && matches!(self.raw_ty_of(recv.syntax()), Some(Ty::Range(..))) =>
            {
                let mut f = rf.wrap("κσ.$rangeOf(", ")");
                f.append(Frag::mapped(
                    self.off(name.syntax()),
                    format!(".{}", name.text()),
                ));
                (f, *elem)
            }
            other => {
                self.error(
                    fe.syntax(),
                    format!("no field `{}` on type `{}`", name.text(), other.describe()),
                );
                (rf, Ty::Unknown)
            }
        }
    }

    /// The file declares a `pub` trait, which a file loading it may implement for built-in types.
    fn has_pub_traits(&self) -> bool {
        self.file_traits
            .iter()
            .any(|t| ast::HasVisibility::visibility(t).is_some())
    }

    /// `method` is a method of one of the file's `pub` traits: a call through a type parameter
    /// goes through the registry, which a loading file's impls for built-in types join.
    fn pub_trait_method(&self, method: &str) -> bool {
        self.file_traits
            .iter()
            .filter(|t| ast::HasVisibility::visibility(*t).is_some())
            .flat_map(|t| t.assoc_item_list().into_iter().flat_map(|l| l.assoc_items()))
            .any(|i| matches!(i, ast::AssocItem::Fn(f) if f.name().is_some_and(|n| n.text() == method)))
    }

    /// A trait method called on a value whose type is erased (a type parameter or `dyn Trait`):
    /// `x.method(..)`, or, when the file implements the method for a built-in type (`impl Money
    /// for f64`), `κσ.$dcall(x, "method", ..)`, which also finds those.
    fn dyn_call(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: &Frag,
        method: &str,
        name_off: usize,
    ) -> Frag {
        // (`Trait$method`, when another trait has a method of that name too.)
        let key = self.types.method_keys.get(&self.range_of(m.syntax())).cloned();
        let method = key.as_deref().unwrap_or(method);
        let args = self.emit_args(m.arg_list(), &[]);
        // `w.write_str(s)` with `W: fmt::Write`: `w` may be a `String` or a `Formatter`, so the
        // call goes through its cell.
        if matches!(method, "write_str" | "write_char")
            && self.types.adts.traits.contains_key("FmtWrite")
            && let Some(recv) = m.receiver()
            && let Some(cell) = self.writer_cell(&recv)
        {
            let mut f = Frag::mapped(name_off, format!("κσ.$fmtWriteCall({cell}, "));
            f.push(js_string(&adt_gen::prop(method)));
            for a in args {
                f.push(", ").append(a);
            }
            f.push(")");
            return f;
        }
        // `x.money()` on a `T` whose class is at hand, with impls on both an integer and a float
        // type: by the class (`κσ.$dcallT`, or `$dcallMutT` with the receiver's cell for a
        // `&mut self` method).
        let by_class = if self
            .types
            .class_dispatch
            .contains(&self.range_of(m.syntax()))
            && let Some(recv) = m.receiver()
            && let Some(Ty::Param(p)) = self.ty_of(recv.syntax())
        {
            Some(self.dict_of(&Ty::Param(p))).filter(|c| c != "undefined")
        } else {
            None
        };
        let ext = self
            .types
            .adts
            .ext_types
            .iter()
            .any(|k| self.types.adts.method(k, method).is_some())
            || self.pub_trait_method(method);
        // A `&mut self` method of a string or number impl: hand over the receiver's cell.
        let mut_ext = self.types.adts.ext_types.iter().any(|k| {
            self.types
                .adts
                .method(k, method)
                .is_some_and(|s| s.receiver == Some(crate::adt::Receiver::MutRef))
        });
        if mut_ext
            && let Some(recv) = m.receiver()
            && matches!(
                strip_refs(&recv),
                ast::Expr::PathExpr(_) | ast::Expr::FieldExpr(_) | ast::Expr::IndexExpr(_)
            )
            // (`self` in a trait's default method mixed into your struct: an object.)
            && !(strip_refs(&recv).syntax().text() == "self"
                && self.lookup("self").is_some_and(|l| l.js == "this"))
        {
            let cell = self.recv_cell(&recv, rf.clone());
            let mut f = match &by_class {
                Some(cls) => Frag::mapped(name_off, format!("κσ.$dcallMutT($extRegs, {cls}, ")),
                None => Frag::mapped(name_off, "κσ.$dcallMut($extRegs, "),
            };
            f.append(cell)
                .push(", ")
                .push(js_string(&adt_gen::prop(method)));
            for a in args {
                f.push(", ").append(a);
            }
            f.push(")");
            return f;
        }
        if let Some(cls) = &by_class {
            let mut f = Frag::mapped(name_off, format!("κσ.$dcallT($extRegs, {cls}, "));
            f.append(rf.clone())
                .push(format!(", {}", js_string(&adt_gen::prop(method))));
            for a in args {
                f.push(", ").append(a);
            }
            f.push(")");
            return f;
        }
        if ext {
            let mut f = Frag::mapped(name_off, "κσ.$dcall($extRegs, ");
            f.append(rf.clone())
                .push(format!(", {}", js_string(&adt_gen::prop(method))));
            for a in args {
                f.push(", ").append(a);
            }
            f.push(")");
            return f;
        }
        let mut f = rf.clone();
        f.push(".")
            .append(Frag::mapped(name_off, adt_gen::prop(method)))
            .push("(")
            .append(Frag::join(args, ", "))
            .push(")");
        f
    }

    /// A cell over a method receiver's place, for a `&mut self` method of a string or number: the
    /// variable's own cell (a `&mut` parameter, `self` in such a method), a getter/setter over a
    /// place, or a cell of its own for a temporary.
    fn recv_cell(&self, recv: &ast::Expr, rf: Frag) -> Frag {
        let e = strip_refs(recv);
        // (`self` too, which isn't a name reference.)
        if let ast::Expr::PathExpr(p) = &e
            && let Some(n) = p.path().map(|p| p.syntax().text().to_string())
            && !n.contains("::")
            && let Some(cell) = self
                .lookup(n.as_str())
                .and_then(|l| l.js.strip_suffix("[0]").map(str::to_string))
        {
            return Frag::text(cell);
        }
        // (Arrow functions, so a `this.field` place keeps its `this`.)
        if matches!(
            e,
            ast::Expr::PathExpr(_) | ast::Expr::FieldExpr(_) | ast::Expr::IndexExpr(_)
        ) {
            let mut f = Frag::text("κσ.$cell(() => ");
            f.append(rf.clone())
                .push(", ($v) => { ")
                .append(rf)
                .push(" = $v; })");
            return f;
        }
        rf.wrap("[", "]")
    }

    /// The cell of a `w: &mut W` parameter (`W: fmt::Write`) that `write!` or `w.write_str(..)`
    /// writes through.
    fn writer_cell(&self, e: &ast::Expr) -> Option<String> {
        let ast::Expr::PathExpr(p) = crate::typeck::strip_parens(e) else {
            return None;
        };
        let name = p.path()?.as_single_name_ref()?;
        let local = self.lookup(name.text())?;
        if !matches!(local.ty, Ty::Param(_)) {
            return None;
        }
        local.js.strip_suffix("[0]").map(str::to_string)
    }

    /// `v[a..b].sort()` and the other in-place slice methods on a range of a Vec.
    fn emit_slice_mut(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        ix: &ast::IndexExpr,
    ) -> (Frag, Ty) {
        let (Some(base), Some(ast::Expr::RangeExpr(r))) = (ix.base(), ix.index()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let lo_pure = r.start().is_none_or(|e| is_pure(&e));
        if !is_pure(&strip_refs(&base)) || !lo_pure {
            self.unsupported(m.syntax(), "this in-place method on a range (assign the Vec and the range start to variables first)");
        }
        // The copy, with the usual range checks.
        let (copy, ty) = self.emit_index(ix);
        let s = self.temp();
        let mut decl = Frag::mapped(self.off(ix.syntax()), format!("const {s} = "));
        decl.append(copy).push(";");
        self.line(decl);
        self.recv_override = Some((self.range_of(recv.syntax()), s.clone(), ty));
        let (call, ret) = self.emit_method(m);
        self.recv_override = None;
        let out = self.temp();
        let mut line = Frag::text(format!("const {out} = "));
        line.append(call).push(";");
        self.line(line);
        let (bf, _) = self.receiver(&base);
        let lo = match r.start() {
            Some(e) => self.emit_expr(&e, None).0,
            None => Frag::text("0"),
        };
        let mut back = Frag::text("κσ.$writeBack(");
        back.append(bf)
            .push(", ")
            .append(lo)
            .push(format!(", {s});"));
        self.line(back);
        (Frag::text(out), ret)
    }

    /// `m.lock().unwrap()` (or `read` / `write` / `get_mut` / `try_*`, then `unwrap` / `expect`, or
    /// `unwrap_or_else`, whose closure never runs since nothing is poisoned)
    /// on the lock prelude's `Mutex` / `RwLock`: the lock. Its value is `<lock>.__c.v`, a place,
    /// so `*m.lock().unwrap() += 1` and a guard in a `let` write through.
    fn lock_place(&self, e: &ast::Expr) -> Option<ast::Expr> {
        // (tokio's: `m.lock().await`, `m.write().await`.)
        if let ast::Expr::AwaitExpr(aw) = strip_parens_expr(e) {
            let ast::Expr::MethodCallExpr(lk) = strip_parens_expr(&aw.expr()?) else {
                return None;
            };
            if !lk
                .name_ref()
                .is_some_and(|n| matches!(&*n.text(), "lock" | "read" | "write"))
            {
                return None;
            }
            let lock = lk.receiver()?;
            let Some(Ty::Adt(n, _)) = self.raw_ty_of(lock.syntax()) else {
                return None;
            };
            let def = self.types.adts.defs.get(&n)?;
            let prelude = matches!(n.as_str(), "TokioMutex" | "TokioRwLock")
                && def.variants.first().is_some_and(|v| v.field("__c").is_some());
            return prelude.then_some(lock);
        }
        let ast::Expr::MethodCallExpr(un) = strip_parens_expr(e) else {
            return None;
        };
        if !un
            .name_ref()
            .is_some_and(|n| matches!(&*n.text(), "unwrap" | "expect" | "unwrap_or_else"))
        {
            return None;
        }
        let ast::Expr::MethodCallExpr(lk) = strip_parens_expr(&un.receiver()?) else {
            return None;
        };
        if !lk.name_ref().is_some_and(|n| {
            matches!(
                &*n.text(),
                "lock" | "read" | "write" | "get_mut" | "try_lock" | "try_read" | "try_write"
            )
        }) {
            return None;
        }
        let lock = lk.receiver()?;
        let Some(Ty::Adt(n, _)) = self.raw_ty_of(lock.syntax()) else {
            return None;
        };
        // (Only the prelude's: a `struct Mutex` of your own has its own methods.)
        let def = self.types.adts.defs.get(&n)?;
        let prelude = matches!(
            n.as_str(),
            "Mutex" | "RwLock" | "TokioMutex" | "TokioRwLock"
        )
            && def.variants.first().is_some_and(|v| v.field("__c").is_some());
        prelude.then_some(lock)
    }

    fn emit_method(&mut self, m: &ast::MethodCallExpr) -> (Frag, Ty) {
        let (Some(recv), Some(name)) = (m.receiver(), m.name_ref()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        if let Some(lock) = self.lock_place(&ast::Expr::MethodCallExpr(m.clone())) {
            let (mut f, _) = self.emit_expr(&lock, None);
            f.push(".__c.v");
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            return (f, ty);
        }
        // `m.lock()` not unwrapped (`if let Ok(mut g) = m.lock()`): an `Ok` whose payload is
        // the lock's value in place, so a guard bound by the pattern writes through.
        if matches!(
            &*name.text(),
            "lock" | "read" | "write" | "get_mut" | "try_lock" | "try_read" | "try_write"
        ) && m.arg_list().is_none_or(|l| l.args().next().is_none())
            && let Some(Ty::Adt(n, _)) = self.raw_ty_of(recv.syntax())
            && matches!(n.as_str(), "Mutex" | "RwLock")
            && self
                .types
                .adts
                .defs
                .get(&n)
                .and_then(|d| d.variants.first())
                .is_some_and(|v| v.field("__c").is_some())
        {
            let (f, _) = self.emit_expr(&recv, None);
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            return (f.wrap("κσ.$lockRes(", ")").at(self.off(name.syntax())), ty);
        }
        let name_off = self.off(name.syntax());
        let method = name.text().to_string();
        let node = m.syntax().clone();
        // `s.get_or_insert_with(String::new).push_str(..)`: a JS string can't change in place, so
        // the option is filled first and the method works on it as a place.
        if self.recv_override.is_none()
            && matches!(
                method.as_str(),
                "push_str"
                    | "push"
                    | "insert"
                    | "insert_str"
                    | "clear"
                    | "truncate"
                    | "pop"
                    | "remove"
                    | "retain"
                    | "extend"
                    | "drain"
                    | "replace_range"
                    | "make_ascii_uppercase"
                    | "make_ascii_lowercase"
            )
            && matches!(self.ty_of(recv.syntax()), Some(Ty::Str))
            && let Some((fill, place)) = self.option_place(&recv)
        {
            self.line(fill.wrap("", ";"));
            self.recv_override = Some((self.range_of(recv.syntax()), place.plain(), Ty::Str));
            let out = self.emit_method(m);
            self.recv_override = None;
            return out;
        }
        // `x.type_id()` (its type's name), `any.is::<T>()` / `downcast_ref::<T>()` (a test).
        if let Some(r) = self.emit_any_method(m, &recv, &method) {
            return r;
        }
        // A range value's own methods.
        if let Some(Ty::Range(elem, incl)) = self.raw_ty_of(recv.syntax()) {
            let helper = match method.as_str() {
                "contains" => Some(("κσ.$rangeHas(", Ty::Bool)),
                "is_empty" => Some(("κσ.$rangeEmpty(", Ty::Bool)),
                "len" => Some(("κσ.$rangeLen(", Ty::Int(IntKind::Usize))),
                "start" | "end" if incl == RangeKind::Inclusive => {
                    Some(("κσ.$rangeOf(", (*elem).clone()))
                }
                _ => None,
            };
            if let Some((head, ty)) = helper {
                let (rf, _) = self.emit_expr(&recv, None);
                let mut f = Frag::mapped(self.off(m.syntax()), head);
                f.append(rf);
                if method == "contains" {
                    let a = m.arg_list().and_then(|l| l.args().next());
                    let x = match a {
                        Some(a) => {
                            let (xf, xt) = self.emit_expr(&a, None);
                            self.by_value(&a, xf, &xt)
                        }
                        None => Frag::text("undefined"),
                    };
                    f.push(", ").append(x);
                }
                f.push(")");
                if method == "start" || method == "end" {
                    f.push(format!(".{method}"));
                }
                return (f, ty);
            }
        }
        if let Some(out) = self.emit_crate_call(
            m.syntax(),
            Some(&recv),
            m.arg_list().map(|l| l.args().collect()).unwrap_or_default(),
        ) {
            return out;
        }
        // `boxed.as_ref()` of a `dyn Trait`: the value (boxes are erased).
        if matches!(self.ty_of(recv.syntax()), Some(Ty::Dyn(_)))
            && matches!(method.as_str(), "as_ref" | "as_mut" | "borrow")
            && m.arg_list().is_none_or(|l| l.args().next().is_none())
        {
            return self.emit_expr(&recv, None);
        }
        // The map entry prelude's `&mut V`: the map's slot for the key, a place.
        if let Some(f) = self.prelude_entry_slot(m) {
            return (f.wrap("", "[1]"), self.ty_of(m.syntax()).unwrap_or(Ty::Unknown));
        }
        // `stream.extend(tokens)` on a proc-macro crate's `TokenStream`.
        if method == "extend"
            && matches!(self.ty_of(recv.syntax()), Some(Ty::Adt(n, _)) if n == "TokenStream")
            && self.types.adts.defs.contains_key("TokenStream")
        {
            let (rf, _) = self.emit_expr(&recv, None);
            let a = match m.arg_list().and_then(|l| l.args().next()) {
                Some(a) => self.emit_expr(&a, None).0,
                None => Frag::text("undefined"),
            };
            let mut f = rf.wrap("(", ").__push(κσ.$tokensJoin(");
            f.append(a).push("))");
            return (f, Ty::Unit);
        }
        // `p.is_null()`, `p.as_ref()`, .. on a raw pointer (a cell for numbers).
        if let Some(&in_cell) = self.types.ptrs.methods.get(&self.range_of(&node)) {
            let (rf, _) = self.emit_expr(&recv, None);
            let ty = self.ty_of(&node).unwrap_or(Ty::Unknown);
            let f = match (method.as_str(), in_cell) {
                ("is_null", _) => rf.wrap("(", " == null)"),
                ("as_ref" | "as_mut", true) => rf.wrap("κσ.$ptrGet(", ")"),
                ("read", true) => rf.wrap("(", ")[0]"),
                ("write", true) => {
                    let v = match m.arg_list().and_then(|a| a.args().next()) {
                        Some(a) => {
                            let (f, t) = self.emit_expr(&a, None);
                            self.by_value(&a, f, &t)
                        }
                        None => Frag::text("undefined"),
                    };
                    let mut f = rf.wrap("(", ")[0] = ");
                    f.append(v);
                    f.wrap("(", ", undefined)")
                }
                _ => rf,
            };
            return (f.at(name_off), ty);
        }
        if let Some(f) = self.iter_mut_for_each(m) {
            return (f, Ty::Unit);
        }
        if let Some(f) = self.entry_and_modify(m) {
            return (f, Ty::Unknown);
        }
        if let Some(slot) = self.entry_slot(m) {
            // The value lives at `slot[1]`, so writes through it reach the map.
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            return (slot.wrap("", "[1]"), ty);
        }
        let rt = self.ty_of(recv.syntax()).unwrap_or(Ty::Unknown);
        // `x.hash(state)`: `κσ.$hashInto(x, state)` (a type's own `hash` is called from there).
        if method == "hash"
            && args_len(m) == 1
            && !matches!(&rt, Ty::Adt(n, _) if self.types.adts.method(n.as_str(), "hash").is_some())
        {
            let (f, _) = self.emit_expr(&recv, None);
            let st = m.arg_list().and_then(|l| l.args().next());
            let s = match st {
                Some(a) => self.emit_expr(&a, None).0,
                None => Frag::text("undefined"),
            };
            let mut out = Frag::mapped(self.off(m.syntax()), "κσ.$hashInto(");
            out.append(f).push(", ").append(s).push(")");
            return (out, Ty::Unit);
        }
        if let Ty::Hasher = rt {
            let a = m.arg_list().and_then(|l| l.args().next());
            let arg = match a {
                Some(a) => self.emit_expr(&a, None).0,
                None => Frag::text(""),
            };
            let (rf, _) = self.emit_expr(&recv, None);
            let js = if method == "finish" {
                "$finish"
            } else {
                "$write"
            };
            let mut out = rf;
            out.push(format!(".{js}(")).append(arg).push(")");
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            return (out, ty);
        }
        if method == "try_into"
            && let Some(conv) = self.conv_of(m.syntax())
        {
            let (f, from) = self.emit_expr(&recv, None);
            let f = self.by_value(&recv, f, &from);
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            return (self.convert(conv, f).at(self.off(m.syntax())), ty);
        }
        if method == "into"
            && let Some(conv) = self.conv_of(m.syntax())
        {
            let (f, from) = self.emit_expr(&recv, None);
            let f = self.by_value(&recv, f, &from);
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            let mut out = self.convert(conv, f);
            // A smaller integer into a BigInt kind (`i32` into `i128`).
            if let Ty::Int(to) = &ty
                && to.is_big()
                && matches!(from.default_lit(), Ty::Int(k) if !k.is_big())
            {
                out = out.wrap(
                    "κσ.$to128(",
                    &format!(", {}, {})", to.is_signed(), to.bits()),
                );
            }
            return (out.at(self.off(m.syntax())), ty);
        }
        // `(lo..hi).contains(&x)`: a bounds check, no iterator.
        if method == "contains"
            && let ast::Expr::RangeExpr(r) = strip_refs(&recv)
        {
            use ra_ap_syntax::ast::RangeItem;
            let bound = |cg: &mut Self, e: Option<ast::Expr>| match e {
                Some(e) => cg.emit_expr(&e, None).0,
                None => Frag::text("null"),
            };
            let lo = bound(self, r.start());
            let hi = bound(self, r.end());
            let x = self
                .emit_args(m.arg_list(), &[])
                .into_iter()
                .next()
                .unwrap_or_else(|| Frag::text("undefined"));
            let incl = r.op_kind() == Some(ra_ap_syntax::ast::RangeOp::Inclusive);
            let mut f = Frag::mapped(self.off(m.syntax()), "κσ.$inRange(");
            f.append(x)
                .push(", ")
                .append(lo)
                .push(", ")
                .append(hi)
                .push(format!(", {incl})"));
            return (f, Ty::Bool);
        }
        // A user trait method on a built-in type: `$ext$f64.prototype.money.call(x)`. Inside a
        // default method compiled for a built-in type, `self`'s type decides it.
        let ext = match self.conv_of(m.syntax()) {
            Some(crate::adt::Conv::Ext { key }) => Some(key),
            _ => match strip_refs(&recv) {
                ast::Expr::PathExpr(p)
                    if p.path()
                        .and_then(|p| p.as_single_name_ref())
                        .is_some_and(|n| n.text() == "self") =>
                {
                    self.lookup("self")
                        .and_then(|l| crate::adt::ext_key_of(&l.ty))
                        .filter(|k| {
                            self.types.adts.ext_types.contains(*k)
                                && self.types.adts.method(k, &method).is_some()
                        })
                        .map(str::to_string)
                }
                _ => None,
            },
        };
        if let Some(key) = ext {
            // (One of several impls of a generic trait: its key.)
            let method = self
                .types
                .method_keys
                .get(&self.range_of(m.syntax()))
                .cloned()
                .unwrap_or(method);
            let sig = self.types.adts.method(&key, &method).cloned();
            let (rf, rt) = self.emit_expr(&recv, None);
            // (Only a `self` receiver takes a copy: `&mut self` writes into the value itself, or
            // into its place through a cell when it's a string or number.)
            let rf = if sig
                .as_ref()
                .is_some_and(|s| adt_gen::cell_self(s, &adt_gen::ext_self_ty(&key)))
            {
                self.recv_cell(&recv, rf)
            } else if sig
                .as_ref()
                .is_some_and(|s| s.receiver != Some(crate::adt::Receiver::Value))
            {
                rf
            } else {
                self.by_value(&recv, rf, &rt)
            };
            let params = sig.as_ref().map(|s| s.params.clone()).unwrap_or_default();
            let mut args = self.emit_args(m.arg_list(), &params);
            // A blanket impl's method takes the receiver's `{:?}` formatter.
            if key.ends_with("$all") {
                let d = debug_fn(&rt).unwrap_or_else(|| "String".into());
                args.push(Frag::text(d));
            }
            // (`impl<T> Tr for Option<T>`'s methods that use `T`'s class: handed over. A
            // `Vec`'s or tuple's alike.)
            let elem = if self.types.elem_methods.contains(&format!("{key}::{method}")) {
                match &rt {
                    Ty::Option(e) | Ty::Vec(e) => Some(self.dict_of(e)),
                    Ty::Tuple(es) => Some(format!(
                        "[{}]",
                        es.iter().map(|e| self.dict_of(e)).collect::<Vec<_>>().join(", ")
                    )),
                    _ => None,
                }
            } else {
                None
            };
            let mut f = match elem {
                Some(d) => Frag::mapped(
                    self.off(m.syntax()),
                    format!(
                        "κσ.$callElem({d}, $ext${key}.prototype.{}, ",
                        adt_gen::prop(&method)
                    ),
                ),
                None => Frag::mapped(
                    self.off(m.syntax()),
                    format!("$ext${key}.prototype.{}.call(", adt_gen::prop(&method)),
                ),
            };
            f.append(Frag::join([vec![rf], args].concat(), ", "))
                .push(")");
            let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            return (f, ty);
        }
        // `v[a..b].sort()`: sort a copy of the range, then write it back into `v`.
        if let ast::Expr::IndexExpr(ix) = strip_refs(&recv)
            && matches!(ix.index(), Some(ast::Expr::RangeExpr(_)))
            && crate::ty::slice_mut_method(&method)
            && self.recv_override.is_none()
        {
            return self.emit_slice_mut(m, &recv, &ix);
        }
        let (rf, rt) = self.receiver(&recv);
        // `VecDeque` etc.: their own methods here; shared ones as the view type's.
        let raw = match self.raw_ty_of(recv.syntax()) {
            Some(t @ Ty::Coll(..)) => t,
            _ => rt.clone(),
        };
        if let Ty::Coll(kind, _) = raw
            && let Some(r) = self.emit_coll_method(m, kind, rf.clone(), &method)
        {
            return r;
        }
        let rt = rt.view();
        // A custom iterator's value used as an iterator: `Fib { .. }.take(10)`.
        let (rf, rt) = match &rt {
            Ty::Adt(n, _)
                if self.types.adts.iter_items.contains_key(n)
                    && self.types.adts.method(n, &method).is_none()
                    && !crate::adt::own_method(&method) =>
            {
                let elem = self.types.adts.iter_items[n].clone();
                (
                    rf.wrap("κσ.$iterOf(", ")"),
                    Ty::Iter {
                        elem: Box::new(elem),
                        mutable: false,
                    },
                )
            }
            _ => (rf, rt),
        };

        if let Ty::Extern(type_name) = &rt {
            let b = bindings::current();
            let sig = b
                .types
                .get(type_name)
                .and_then(|t| t.methods.get(&bare(&method)));
            // `clone()` of a JS value: the same handle (a DOM node), or a copy of an array.
            if sig.is_none() && matches!(method.as_str(), "clone" | "to_owned") {
                return (rf.wrap("κσ.$clone(", ")"), rt.clone());
            }
            // `v.as_string()`: the runtime's `κσ.$jsv.asString(v)` (any JS value has them). A
            // bindings enum's readers (`arg.as_f64()`, `res.into_result()`) and a `#[runtime]` method
            // (`flags.get_str("target")`) are the same helpers.
            if let Some(s) = sig
                && (type_name == "JsValue" || b.is_enum(type_name) || s.runtime)
            {
                let args = self.emit_args(m.arg_list(), &[]);
                let mut f = Frag::mapped(name_off, format!("κσ.$jsv.${}(", s.js));
                f.append(rf);
                for a in args {
                    f.push(", ").append(a);
                }
                f.push(")");
                return (f, s.ret.clone());
            }
            // NS API: snake_case Rust names map to the camelCase JS API, kept as a literal
            // `ns.method(...)` member call so Bitburner's RAM calculator sees it.
            let mut args = self.emit_args(m.arg_list(), &[]);
            // (A React node parameter, `ns.print_raw(x)`: Rust's numbers as text, `Option`s and
            // `Vec`s of nodes as React takes them.)
            if let Some(s) = sig {
                let exprs: Vec<ast::Expr> =
                    m.arg_list().map(|l| l.args().collect()).unwrap_or_default();
                for (i, (f, e)) in args.iter_mut().zip(&exprs).enumerate() {
                    let node_param = s
                        .params
                        .get(i)
                        .is_some_and(|p| matches!(&p.ty, Ty::Extern(n) if n == "ReactNode"));
                    let as_is = match self.ty_of(e.syntax()) {
                        None | Some(Ty::Unknown | Ty::Str) => true,
                        Some(Ty::Extern(n)) => n == "ReactNode" || n == "JsValue",
                        _ => false,
                    };
                    if node_param && !as_is {
                        *f = std::mem::replace(f, Frag::text("")).wrap("κσ.$react.node(", ")");
                    }
                }
            }
            let mut f = rf;
            let js = extern_js_name(type_name, &method, true);
            f.append(Frag::mapped(name_off, member(&js)));
            // `#[getter]`: a property; `#[array]`: an array-like result (`querySelectorAll`).
            if !sig.is_some_and(|s| s.getter) {
                f.push("(").append(Frag::join(args, ", ")).push(")");
            }
            if sig.is_some_and(|s| s.array) {
                f = f.wrap("κσ.$arrayOf(", ")");
            }
            // (A `HashMap` result is a JS `Map` or object; awaited first when async.)
            if let Some(s) = sig
                && iface_gen::is_js_map(&s.ret)
            {
                f = if s.is_async {
                    f.wrap("(", ").then((r) => κσ.$mapOfJs(r))")
                } else {
                    iface_gen::from_js(f, &s.ret)
                };
            }
            return (f, Ty::Unknown);
        }

        if let Ty::Adt(n, _) = &rt
            && let Some(r) = self.emit_adt_method(m, &rf, n, &method)
        {
            return r;
        }
        if let Ty::RefCell(inner) = &rt {
            let name_off = self.off(name.syntax());
            let ret = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            let arg = m
                .arg_list()
                .and_then(|a| a.args().next())
                .map(|a| {
                    let (f, t) = self.emit_expr(&a, None);
                    self.by_value(&a, f, &t)
                })
                .unwrap_or_else(|| Frag::text("undefined"));
            let f = match method.as_str() {
                // The value lives at `.v`, which stays assignable: `*c.borrow_mut() += 1`.
                "borrow" | "borrow_mut" | "get" | "into_inner" => {
                    let mut f = rf.clone();
                    f.push(".").append(Frag::mapped(name_off, "v"));
                    f
                }
                "set" => {
                    let mut f = Frag::text("(");
                    f.append(rf.clone())
                        .push(".v = ")
                        .append(arg)
                        .push(", undefined)");
                    f
                }
                "replace" | "take" => {
                    let new = if method == "take" {
                        Frag::text(default_value(inner).unwrap_or_else(|| "undefined".into()))
                    } else {
                        arg
                    };
                    let mut f = Frag::mapped(name_off, "κσ.$cellReplace(");
                    f.append(rf.clone()).push(", ").append(new).push(")");
                    f
                }
                // Through an `Rc` (erased), cloning shares the cell.
                _ => rf.clone(),
            };
            return (f, ret);
        }
        if let Ty::Param(_) = &rt {
            let name_off = self.off(name.syntax());
            let ret = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
            let arg = m
                .arg_list()
                .and_then(|a| a.args().next())
                .map(|a| self.emit_expr(&a, None).0)
                .unwrap_or_else(|| Frag::text("undefined"));
            let helper = |js: &str, with_arg: bool| {
                let mut f = Frag::mapped(name_off, format!("κσ.${js}("));
                f.append(rf.clone());
                if with_arg {
                    f.push(", ").append(arg.clone());
                }
                f.push(")");
                f
            };
            let f = match method.as_str() {
                "clone" => helper("clone", false),
                "to_string" => helper("disp", false),
                "eq" => helper("eq", true),
                "ne" => helper("eq", true).wrap("!", ""),
                "cmp" | "partial_cmp" => helper("cmp", true),
                "max" => helper("max", true),
                "min" => helper("min", true),
                // `self.0.fmt(f)`: `Display::fmt` / `Debug::fmt` delegation.
                "fmt" if args_len(m) == 1 => {
                    let debug = crate::typeck::delegated_fmt_is_debug(m.syntax(), true, true);
                    self.fmt_delegate(rf.clone(), &rt, arg.clone(), debug)
                }
                // A method from a trait bound: dynamic dispatch.
                _ => self.dyn_call(m, &rf, &method, name_off),
            };
            return (f, ret);
        }
        if let Ty::Dyn(_) = &rt {
            // `dyn Trait`: dynamic dispatch.
            let name_off = self.off(name.syntax());
            let f = self.dyn_call(m, &rf, &method, name_off);
            return (f, self.ty_of(m.syntax()).unwrap_or(Ty::Unknown));
        }
        if let Some(r) = self.emit_method_ext(m, &rf, &rt, &method) {
            return r;
        }
        if let Ty::Map(k, v) = &rt {
            let (k, v) = ((**k).clone(), (**v).clone());
            return self.emit_map_method(m, rf, &k, &v, &method);
        }
        if let Ty::Set(elem) = &rt {
            let elem = (**elem).clone();
            return self.emit_set_method(m, rf, &elem, &method);
        }
        if let Ty::Tuple(_) = &rt
            && matches!(method.as_str(), "clone" | "to_owned")
        {
            return (Frag::text("κσ.$clone(").append(rf).push(")").clone(), rt);
        }
        if let Ty::Option(inner) = &rt {
            let inner = (**inner).clone();
            return self.emit_option_method(m, &recv, rf, &inner, &method);
        }
        if let Ty::Vec(elem) = &rt {
            let elem = (**elem).clone();
            return self.emit_vec_method(m, rf, &elem, &method);
        }

        // NS values without a binding are untyped; NS numbers are f64, so assume that for
        // float-only method names.
        let rt = match rt {
            Ty::Unknown if is_float_method(&method) => Ty::Float { f32: false },
            Ty::IntLit => Ty::Int(IntKind::I32),
            t => t,
        };
        // `small.into()` / `.try_into()`-free widening to a BigInt kind: convert.
        if method == "into"
            && let Ty::Int(from) = &rt
            && !from.is_big()
            && let Some(Ty::Int(to)) = self.ty_of(m.syntax())
            && to.is_big()
        {
            let f = rf.wrap(
                "κσ.$to128(",
                &format!(", {}, {})", to.is_signed(), to.bits()),
            );
            return (f, Ty::Int(to));
        }
        // An iterator's clone goes on over what's left by itself.
        if method == "clone" && matches!(rt, Ty::Iter { .. }) {
            return (rf.wrap("", ".$clone()"), rt);
        }
        if matches!(method.as_str(), "clone" | "to_owned" | "as_str" | "into") && rt != Ty::Unknown
            || (method == "into_owned" && rt == Ty::Str)
        {
            return (rf, rt);
        }
        let arg_ty = match &rt {
            Ty::Int(_) if method == "pow" => Ty::Int(IntKind::U32),
            Ty::Float { .. } if method == "powi" => Ty::Int(IntKind::I32),
            Ty::Str => Ty::Str,
            t => t.clone(),
        };
        let args = self.emit_args(m.arg_list(), &[arg_ty.clone(), arg_ty]);
        let a = |i: usize| {
            args.get(i)
                .cloned()
                .unwrap_or_else(|| Frag::text("undefined"))
        };
        let fcall = |name: &str, args: Vec<Frag>| {
            let mut f = Frag::mapped(name_off, format!("{name}("));
            f.append(Frag::join(args, ", ")).push(")");
            f
        };
        let mcall = |rf: Frag, name: &str, args: Vec<Frag>| {
            let mut f = rf;
            f.push(".")
                .append(Frag::mapped(name_off, format!("{name}(")));
            f.append(Frag::join(args, ", ")).push(")");
            f
        };

        match (&rt, method.as_str()) {
            (Ty::Float { .. }, m) if is_float_method(m) => {
                let math = |n: &str| fcall(&format!("Math.{n}"), vec![rf.clone()]);
                match m {
                    "sqrt" | "abs" | "floor" | "ceil" | "trunc" | "exp" | "sin" | "cos" | "tan"
                    | "asin" | "acos" | "atan" | "cbrt" | "log10" | "log2" => (math(m), rt.clone()),
                    "ln" => (math("log"), rt.clone()),
                    "round" => (fcall("κσ.$round", vec![rf.clone()]), rt.clone()),
                    "powi" | "powf" => (
                        rf.clone().wrap("(", " ** ").append(a(0)).push(")").clone(),
                        rt.clone(),
                    ),
                    "min" | "max" => (
                        fcall(&format!("Math.{m}"), vec![rf.clone(), a(0)]),
                        rt.clone(),
                    ),
                    "atan2" => (fcall("Math.atan2", vec![rf.clone(), a(0)]), rt.clone()),
                    "clamp" => (fcall("κσ.$clamp", vec![rf.clone(), a(0), a(1)]), rt.clone()),
                    "is_nan" => (fcall("Number.isNaN", vec![rf.clone()]), Ty::Bool),
                    "is_finite" => (fcall("Number.isFinite", vec![rf.clone()]), Ty::Bool),
                    "to_string" if rt == Ty::Float { f32: true } => {
                        (fcall("κσ.$fmtF32", vec![rf.clone()]), Ty::Str)
                    }
                    "to_string" => (fcall("κσ.$fmtF", vec![rf.clone()]), Ty::Str),
                    _ => unreachable!(),
                }
            }
            // BigInt kinds: `κσ.$big(name, x, bits, signed, ..args)` (debug builds check `abs`
            // and `pow` the same way).
            (Ty::Int(k), mn) if k.is_big() => {
                let n_args = m.arg_list().map(|l| l.args().count()).unwrap_or(0);
                let mut all = vec![
                    Frag::text(js_string(mn)),
                    rf.clone(),
                    Frag::text(format!("{}, {}", k.bits(), k.is_signed())),
                ];
                all.extend((0..n_args).map(a));
                if overflow_checks() && matches!(mn, "abs" | "pow") {
                    all.push(Frag::text(js_string(&self.location(m.syntax()))));
                }
                let ret = if mn == "to_string" {
                    Ty::Str
                } else {
                    rt.clone()
                };
                (fcall("κσ.$big", all), ret)
            }
            // Debug builds: `i32::MIN.abs()` and an overflowing `pow` panic.
            (Ty::Int(k), "abs" | "pow")
                if overflow_checks() && (*k != IntKind::U128 || method == "pow") =>
            {
                let (min, max) = int_bounds(*k);
                let loc = js_string(&self.location(m.syntax()));
                let f = if method == "abs" {
                    fcall(
                        "κσ.$ckAbs",
                        vec![rf.clone(), Frag::text(min), Frag::text(loc)],
                    )
                } else {
                    fcall(
                        "κσ.$ckPow",
                        vec![
                            rf.clone(),
                            a(0),
                            Frag::text(min),
                            Frag::text(max),
                            Frag::text(loc),
                        ],
                    )
                };
                (f, rt.clone())
            }
            (Ty::Int(k), m) => {
                let k = *k;
                let wrap = |f: Frag| match k.wrap_parts() {
                    Some((pre, post)) => f.wrap(pre, post),
                    None => f,
                };
                match m {
                    "abs" => (wrap(fcall("Math.abs", vec![rf.clone()])), rt.clone()),
                    "pow" if k.is_narrow() => (
                        wrap(fcall("κσ.$ipow32", vec![rf.clone(), a(0)])),
                        rt.clone(),
                    ),
                    "pow" => (
                        rf.clone().wrap("(", " ** ").append(a(0)).push(")").clone(),
                        rt.clone(),
                    ),
                    "min" | "max" => (
                        fcall(&format!("Math.{m}"), vec![rf.clone(), a(0)]),
                        rt.clone(),
                    ),
                    "clamp" => (fcall("κσ.$clamp", vec![rf.clone(), a(0), a(1)]), rt.clone()),
                    "signum" => (fcall("Math.sign", vec![rf.clone()]), rt.clone()),
                    "rem_euclid" => (fcall("κσ.$remEuclid", vec![rf.clone(), a(0)]), rt.clone()),
                    "to_string" => (fcall("String", vec![rf.clone()]), Ty::Str),
                    _ => self.no_method(&node, &method, &rt),
                }
            }
            (Ty::Str, m) => match m {
                "len" => (
                    fcall("κσ.$utf8len", vec![rf.clone()]),
                    Ty::Int(IntKind::Usize),
                ),
                "is_empty" => (rf.clone().wrap("(", ".length === 0)"), Ty::Bool),
                "to_string" => (rf.clone(), Ty::Str),
                "to_uppercase" => (mcall(rf.clone(), "toUpperCase", vec![]), Ty::Str),
                "to_lowercase" => (mcall(rf.clone(), "toLowerCase", vec![]), Ty::Str),
                "trim" => (mcall(rf.clone(), "trim", vec![]), Ty::Str),
                "trim_start" => (mcall(rf.clone(), "trimStart", vec![]), Ty::Str),
                "trim_end" => (mcall(rf.clone(), "trimEnd", vec![]), Ty::Str),
                "contains" => (mcall(rf.clone(), "includes", vec![a(0)]), Ty::Bool),
                "starts_with" => (mcall(rf.clone(), "startsWith", vec![a(0)]), Ty::Bool),
                "ends_with" => (mcall(rf.clone(), "endsWith", vec![a(0)]), Ty::Bool),
                "replace" => (mcall(rf.clone(), "replaceAll", vec![a(0), a(1)]), Ty::Str),
                "repeat" => (mcall(rf.clone(), "repeat", vec![a(0)]), Ty::Str),
                "as_bytes" | "into_bytes" => (
                    fcall("κσ.$utf8", vec![rf.clone()]),
                    Ty::Vec(Box::new(Ty::Int(IntKind::U8))),
                ),
                "push_str" | "push" => {
                    // Lowered as `s += x`, so the receiver must be an assignable JS place.
                    if !self.place_receiver(&recv) {
                        self.unsupported(&node, "`push_str` on this expression");
                    }
                    let mut f = rf.clone();
                    f.push(" += ").append(a(0));
                    (f, Ty::Unit)
                }
                "write_str" | "write_char" => {
                    if !is_pure(&strip_refs(&recv)) {
                        self.unsupported(&node, "`write_str` on this expression");
                    }
                    let mut f = Frag::text("(");
                    f.append(rf.clone())
                        .push(" += ")
                        .append(a(0))
                        .push(", κσ.Ok(undefined))");
                    let fmt_result = Ty::Result(
                        Box::new(Ty::Unit),
                        Box::new(Ty::Extern("fmt::Error".into())),
                    );
                    (f, fmt_result)
                }
                "extend" => {
                    if !self.place_receiver(&recv) {
                        self.unsupported(&node, "`extend` on this expression");
                    }
                    let mut f = rf.clone();
                    f.push(" += Array.from(").append(a(0)).push(").join(\"\")");
                    (f, Ty::Unit)
                }
                _ => self.no_method(&node, &method, &rt),
            },
            (Ty::Bool | Ty::Char, "to_string") => (fcall("String", vec![rf.clone()]), Ty::Str),
            (Ty::Unknown, _) => (mcall(rf.clone(), &method, args.clone()), Ty::Unknown),
            _ => self.no_method(&node, &method, &rt),
        }
    }

    fn no_method(&mut self, node: &SyntaxNode, method: &str, ty: &Ty) -> (Frag, Ty) {
        self.error(
            node,
            format!("no method named `{method}` found for `{}` (the spike's std surface is small; see docs/FEATURES.md)", ty.describe()),
        );
        (Frag::text("undefined"), Ty::Unknown)
    }

    // ---------------------------------------------------------------- patterns

    /// A name for the value being matched that's safe to repeat in conditions: the expression
    /// itself when it's pure, otherwise a `const` temporary.
    fn subject(&mut self, e: &ast::Expr) -> String {
        // `if let Some(n) = self.timer.as_mut()`: the `Option` is the place's value, so match on
        // the place (no temporary), and `n` aliases it (see `mark_mut_ref_bindings`).
        if let ast::Expr::MethodCallExpr(m) = strip_parens_expr(e)
            && m.name_ref().is_some_and(|n| n.text() == "as_mut")
            && let Some(r) = m.receiver()
            && is_pure(&r)
            && matches!(self.ty_of(r.syntax()), Some(Ty::Option(_)))
        {
            return self.subject(&r);
        }
        let (f, _) = self.emit_expr(e, None);
        // `match &prog[pc]` on a struct or enum: the element once, so the arms' tests don't
        // repeat the bounds check. (Bindings into its fields stay places of that same object.)
        let indexed = matches!(strip_refs(e), ast::Expr::IndexExpr(_))
            && matches!(self.ty_of(e.syntax()), Some(Ty::Adt(..)));
        if is_pure(e) && !indexed {
            let text = f.plain();
            return if needs_parens(e) {
                format!("({text})")
            } else {
                text
            };
        }
        let t = self.temp();
        let mut decl = Frag::mapped(self.off(e.syntax()), format!("const {t} = "));
        decl.append(f).push(";");
        self.line(decl);
        t
    }

    /// Lower a pattern matched against `subject` into test conditions and bindings.
    fn lower_pat(&mut self, p: &ast::Pat, subject: &str, out: &mut PatCode) {
        match p {
            ast::Pat::WildcardPat(_) => {}
            ast::Pat::ParenPat(pp) => {
                if let Some(inner) = pp.pat() {
                    self.lower_pat(&inner, subject, out);
                }
            }
            ast::Pat::RefPat(rp) => {
                if let Some(inner) = rp.pat() {
                    self.lower_pat(&inner, subject, out);
                }
            }
            ast::Pat::IdentPat(ip) if is_none_binding(ip) => {
                out.conds.push(Frag::text(format!("{subject} == null")))
            }
            ast::Pat::IdentPat(ip) if self.unit_struct_pat(ip).is_some() => {
                self.lower_adt_pat(p, subject, out);
            }
            ast::Pat::IdentPat(ip)
                if ip.pat().is_none()
                    && ip.mut_token().is_none()
                    && ip.name().is_some_and(|n| {
                        self.consts.contains_key(n.text().to_string().as_str())
                    }) =>
            {
                let js = self.consts[ip.name().unwrap().text().to_string().as_str()].clone();
                let primitive = self.ty_of(p.syntax()).is_some_and(|t| t.is_js_primitive());
                out.conds.push(Frag::text(if primitive {
                    format!("{subject} === {js}")
                } else {
                    format!("κσ.$eq({subject}, {js})")
                }));
            }
            ast::Pat::IdentPat(ip) => {
                if let Some(sub) = ip.pat() {
                    self.lower_pat(&sub, subject, out);
                }
                if let Some(n) = ip.name() {
                    out.binds
                        .push((n, ip.mut_token().is_some(), subject.to_string()));
                }
            }
            ast::Pat::PathPat(pp)
                if crate::typeck::ordering_variant(pp.path(), &self.types.adts.defs).is_some() =>
            {
                let n = crate::typeck::ordering_variant(pp.path(), &self.types.adts.defs).unwrap();
                out.conds.push(Frag::text(format!("{subject} === {n}")));
            }
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => {
                out.conds.push(Frag::text(format!("{subject} == null")))
            }
            // `S::ZERO`, `<S as Tr>::TOP`, `u8::MAX`: compared with the constant's value.
            ast::Pat::PathPat(pp) if self.const_pat_js(pp).is_some() => {
                let js = self.const_pat_js(pp).unwrap();
                let primitive = self.ty_of(p.syntax()).is_some_and(|t| t.is_js_primitive());
                out.conds.push(Frag::text(if primitive {
                    format!("{subject} === {js}")
                } else {
                    format!("κσ.$eq({subject}, {js})")
                }));
            }
            ast::Pat::TupleStructPat(tp) if crate::typeck::result_variant(tp.path()).is_some() => {
                let v = crate::typeck::result_variant(tp.path()).unwrap();
                self.lower_result_pat(tp, v, subject, out);
            }
            ast::Pat::TupleStructPat(tp) if path_is(tp.path(), "Some") => {
                // `Some(x)` is represented by `x` itself (a marker for `Some(None)`).
                out.conds.push(Frag::text(format!("{subject} != null")));
                if let Some(f) = tp.fields().next() {
                    let nested = self.ty_of(f.syntax()).is_some_and(|t| nullable_payload(&t));
                    if nested {
                        self.lower_pat(&f, &format!("κσ.$unsome({subject})"), out);
                    } else {
                        self.lower_pat(&f, subject, out);
                    }
                }
            }
            ast::Pat::LiteralPat(lp) => {
                let lit = self.pat_literal(lp);
                out.conds
                    .push(Frag::text(format!("{subject} === ")).append(lit).clone());
            }
            ast::Pat::RangePat(rp) => {
                let is_char = matches!(self.ty_of(p.syntax()), Some(Ty::Char));
                // Chars compare by code point, so astral characters order correctly.
                let value = if is_char {
                    format!("{subject}.codePointAt(0)")
                } else {
                    subject.to_string()
                };
                let bound = |cg: &mut Self, b: Option<ast::Pat>| -> Option<Frag> {
                    let lp = match b? {
                        ast::Pat::LiteralPat(lp) => lp,
                        // `0..=LIMIT`
                        ast::Pat::IdentPat(ip) => {
                            let js = ip.name().and_then(|n| {
                                cg.consts.get(n.text().to_string().as_str()).cloned()
                            });
                            let Some(js) = js else {
                                cg.unsupported(ip.syntax(), "this range pattern bound");
                                return None;
                            };
                            return Some(Frag::text(if is_char {
                                format!("{js}.codePointAt(0)")
                            } else {
                                js
                            }));
                        }
                        // `S::LO..S::HI`, `..=u8::MAX`
                        ast::Pat::PathPat(pp) => {
                            let js = cg.const_pat_js(&pp).or_else(|| {
                                // (`char::MAX`.)
                                let segs = crate::adt::const_pat_segs(pp.path()?);
                                (segs == ["char", "MAX"]).then(|| "\"\\u{10ffff}\"".to_string())
                            });
                            let Some(js) = js else {
                                cg.unsupported(pp.syntax(), "this range pattern bound");
                                return None;
                            };
                            return Some(Frag::text(if is_char {
                                format!("{js}.codePointAt(0)")
                            } else {
                                js
                            }));
                        }
                        _ => return None,
                    };
                    if is_char && let Some(LiteralKind::Char(c)) = lp.literal().map(|l| l.kind()) {
                        return Some(Frag::text((c.value().unwrap_or('\0') as u32).to_string()));
                    }
                    Some(cg.pat_literal(&lp))
                };
                if let Some(lo) = bound(self, rp.start()) {
                    out.conds
                        .push(Frag::text(format!("{value} >= ")).append(lo).clone());
                }
                if let Some(hi) = bound(self, rp.end()) {
                    let op = if rp.op_kind() == Some(RangeOp::Inclusive) {
                        "<="
                    } else {
                        "<"
                    };
                    out.conds
                        .push(Frag::text(format!("{value} {op} ")).append(hi).clone());
                }
            }
            ast::Pat::OrPat(op) => {
                // Each alternative's condition and bindings; a binding takes its value from
                // the alternative that matched (`c1 ? a : c2 ? b : c`).
                let mut alts: Vec<(Option<Frag>, Vec<Bind>)> = Vec::new();
                for alt in op.pats() {
                    let mut pc = PatCode::default();
                    self.lower_pat(&alt, subject, &mut pc);
                    let cond = pc.condition();
                    let always = cond.is_none();
                    alts.push((cond, pc.binds));
                    // (An irrefutable alternative: the rest never get a turn.)
                    if always {
                        break;
                    }
                }
                let Some((_, first_binds)) = alts.first().cloned() else {
                    return;
                };
                for (name, mutable, _) in first_binds {
                    let n = name.text().to_string();
                    let value_in = |binds: &[Bind]| {
                        binds
                            .iter()
                            .find(|(m, _, _)| m.text() == n.as_str())
                            .map(|(_, _, v)| v.clone())
                            .unwrap_or_else(|| "undefined".into())
                    };
                    let (_, last_binds) = alts.last().unwrap();
                    let mut value = value_in(last_binds);
                    for (cond, binds) in alts.iter().rev().skip(1) {
                        if let Some(c) = cond {
                            value = format!("({}) ? {} : {value}", c.plain(), value_in(binds));
                        }
                    }
                    if alts.len() > 1 {
                        value = format!("({value})");
                    }
                    out.binds.push((name, mutable, value));
                }
                if alts.iter().all(|(c, _)| c.is_some()) {
                    let conds: Vec<Frag> = alts.into_iter().filter_map(|(c, _)| c).collect();
                    out.conds.push(Frag::join(conds, " || ").wrap("(", ")"));
                }
            }
            ast::Pat::TuplePat(tp) => {
                // Elements are `subject[i]`; `..` skips the middle, so later fields count from the end.
                let fields: Vec<ast::Pat> = tp.fields().collect();
                let rest = fields
                    .iter()
                    .position(|f| matches!(f, ast::Pat::RestPat(_)));
                let n = match self.ty_of(p.syntax()) {
                    Some(Ty::Tuple(es)) => es.len(),
                    _ => fields.len() - usize::from(rest.is_some()),
                };
                for (i, f) in fields.iter().enumerate() {
                    let index = match rest {
                        Some(r) if i == r => continue,
                        Some(r) if i > r => n - (fields.len() - i),
                        _ => i,
                    };
                    self.lower_pat(f, &format!("{subject}[{index}]"), out);
                }
            }
            ast::Pat::SlicePat(sp) => {
                // Length test, then elements from the front and (after `..`) from the back.
                let pats: Vec<ast::Pat> = sp.pats().collect();
                let is_rest = |f: &ast::Pat| {
                    matches!(f, ast::Pat::RestPat(_))
                        || matches!(f, ast::Pat::IdentPat(ip) if matches!(ip.pat(), Some(ast::Pat::RestPat(_))))
                };
                let rest_at = pats.iter().position(is_rest);
                let fixed = pats.len() - usize::from(rest_at.is_some());
                out.conds.push(Frag::text(match rest_at {
                    Some(_) => format!("{subject}.length >= {fixed}"),
                    None => format!("{subject}.length === {fixed}"),
                }));
                for (i, f) in pats.iter().enumerate() {
                    match rest_at {
                        Some(r) if i == r => {
                            if let ast::Pat::IdentPat(ip) = f
                                && let Some(n) = ip.name()
                            {
                                let after = pats.len() - r - 1;
                                out.binds.push((
                                    n,
                                    ip.mut_token().is_some(),
                                    format!("{subject}.slice({r}, {subject}.length - {after})"),
                                ));
                            }
                        }
                        Some(r) if i > r => {
                            let from_end = pats.len() - i;
                            self.lower_pat(
                                f,
                                &format!("{subject}[{subject}.length - {from_end}]"),
                                out,
                            );
                        }
                        _ => self.lower_pat(f, &format!("{subject}[{i}]"), out),
                    }
                }
            }
            other if self.lower_adt_pat(other, subject, out) => {}
            other => {
                let what = match other {
                    ast::Pat::SlicePat(_) => "slice patterns (`[first, .., last]`)",
                    ast::Pat::RecordPat(_) | ast::Pat::TupleStructPat(_) => {
                        "struct and enum patterns"
                    }
                    ast::Pat::PathPat(_) => "constant and enum patterns",
                    _ => "this pattern",
                };
                self.unsupported(other.syntax(), what);
            }
        }
    }

    /// A path pattern naming a constant (see `Checker::const_path_pat`): its value as JS.
    fn const_pat_js(&self, pp: &ast::PathPat) -> Option<String> {
        let segs = crate::adt::const_pat_segs(pp.path()?);
        if let [name] = segs.as_slice() {
            return self.consts.get(name).cloned();
        }
        let tail: Vec<&str> = segs.iter().map(String::as_str).collect();
        if let Some((js, _)) = crate::ty::std_const(crate::ty::std_tail(&tail)) {
            return Some(js.to_string());
        }
        match self.resolve(&segs)? {
            crate::adt::Resolved::Assoc(adt, m)
                if self.types.adts.method(&adt, &m).is_none()
                    && self
                        .types
                        .adts
                        .assoc_consts
                        .get(&adt)
                        .is_some_and(|c| c.contains_key(&m)) =>
            {
                Some(format!("{}.{}", self.class(&adt), escape_ident(&m)))
            }
            _ => None,
        }
    }

    fn pat_literal(&mut self, lp: &ast::LiteralPat) -> Frag {
        let Some(l) = lp.literal() else {
            return Frag::text("undefined");
        };
        let (mut f, _) = self.emit_literal(&l, None);
        // A BigInt subject: `0n`, so `===` matches.
        if matches!(l.kind(), LiteralKind::IntNumber(_))
            && matches!(self.ty_of(lp.syntax()), Some(Ty::Int(k)) if k.is_big())
            && !f.parts.last().is_some_and(|(_, t)| t.ends_with('n'))
        {
            f.push("n");
        }
        if lp.minus_token().is_some() {
            Frag::text("-").append(f).clone()
        } else {
            f
        }
    }

    /// Bind each name to its place directly, with no copy, so writes through it reach the
    /// original (`for (i, x) in v.iter_mut().enumerate() { *x += i }`).
    fn alias_binds(&mut self, binds: Vec<(ast::Name, bool, String)>) {
        for (name, _, place) in binds {
            self.declare(
                name.text(),
                Some(name.syntax()),
                Local {
                    js: place,
                    ty: Ty::Unknown,
                },
            );
        }
    }

    /// `const x = subject;` for each binding, in the current scope.
    fn emit_binds(&mut self, binds: Vec<(ast::Name, bool, String)>) {
        for (name, mutable, js_value) in binds {
            // A `&mut` binding into the matched place aliases it.
            let r = (
                self.off(name.syntax()),
                self.base + usize::from(name.syntax().text_range().end()),
            );
            if self.types.mut_bindings.contains(&r) {
                self.alias_binds(vec![(name, mutable, js_value)]);
                continue;
            }
            let js = self.fresh(name.text());
            let kw = if mutable { "let" } else { "const" };
            let boxed = self.is_boxed(name.syntax());
            let value = if boxed {
                format!("[{js_value}]")
            } else {
                js_value
            };
            self.line(Frag::mapped(
                self.off(name.syntax()),
                format!("{kw} {js} = {value};"),
            ));
            let js = if boxed { format!("{js}[0]") } else { js };
            self.declare(
                name.text(),
                Some(name.syntax()),
                Local {
                    js,
                    ty: Ty::Unknown,
                },
            );
        }
    }

    /// `let PAT = EXPR;` for patterns other than a plain binding, and `let PAT = EXPR else { .. };`.
    fn emit_pattern_let(&mut self, l: &ast::LetStmt) {
        // `let (a, b);`: each name is assigned later.
        if let (Some(pat), None) = (l.pat(), l.initializer()) {
            for ip in pat.syntax().descendants().filter_map(ast::IdentPat::cast) {
                let Some(name) = ip.name() else { continue };
                let js = self.fresh(name.text());
                self.line(Frag::mapped(self.off(name.syntax()), format!("let {js};")));
                let ty = self.ty_of(name.syntax()).unwrap_or(Ty::Unknown);
                self.declare(name.text(), Some(name.syntax()), Local { js, ty });
            }
            return;
        }
        let (Some(pat), Some(init)) = (l.pat(), l.initializer()) else {
            return;
        };
        let subject = self.subject(&init);
        let mut pc = PatCode::default();
        self.lower_pat(&pat, &subject, &mut pc);
        if let (Some(le), Some(c)) = (l.let_else(), pc.condition()) {
            let mut head = Frag::mapped(self.off(le.syntax()), "if (!(");
            head.append(c).push(")) {");
            self.line(head);
            self.out.indent += 1;
            if let Some(b) = le.block_expr() {
                self.emit_block_body(&b, &Dest::Discard, None);
            }
            self.out.indent -= 1;
            self.close_line("}", le.block_expr().as_ref());
        }
        self.emit_binds(pc.binds);
    }

    /// `match` becomes an if/else chain. With guards it becomes a labeled block instead, so a
    /// guard can see its arm's bindings and fall through to later arms when it fails.
    fn emit_match(&mut self, m: &ast::MatchExpr, dest: &Dest, expected: Option<&Ty>) -> Ty {
        let start = self.off(m.syntax());
        let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
        let Some(scrutinee) = m.expr() else { return ty };
        let subject = self.subject(&scrutinee);
        let arms: Vec<ast::MatchArm> = m
            .match_arm_list()
            .map(|l| l.arms().collect())
            .unwrap_or_default();
        let guarded = arms.iter().any(|a| a.guard().is_some());
        let label = if guarded { Some(self.temp()) } else { None };
        if let Some(label) = &label {
            self.line(Frag::mapped(start, format!("{label}: {{")));
            self.out.indent += 1;
        }
        // A guarded arm whose `|` alternatives bind variables is tried one alternative at a
        // time, as in Rust: `Rect(w, _) | Rect(_, w) if w > 100.0` re-checks the guard with the
        // second alternative's `w`.
        let units: Vec<(ast::MatchArm, ast::Pat)> = arms
            .iter()
            .filter_map(|arm| Some((arm.clone(), arm.pat()?)))
            .flat_map(|(arm, pat)| match strip_or(&pat) {
                Some(op)
                    if arm.guard().is_some()
                        && op
                            .syntax()
                            .descendants()
                            .any(|n| ast::IdentPat::can_cast(n.kind())) =>
                {
                    op.pats().map(|p| (arm.clone(), p)).collect::<Vec<_>>()
                }
                _ => vec![(arm, pat)],
            })
            .collect();
        let mut first = true;
        for (arm, pat) in &units {
            let arm_off = self.off(arm.syntax());
            let mut pc = PatCode::default();
            self.lower_pat(pat, &subject, &mut pc);
            let cond = pc.condition();
            let catch_all = cond.is_none() && arm.guard().is_none();
            let head = match (&label, first, &cond) {
                (Some(_), _, Some(c)) => Frag::text("if (").append(c.clone()).push(") {").clone(),
                (Some(_), _, None) => Frag::text("{"),
                (None, true, Some(c)) => Frag::text("if (").append(c.clone()).push(") {").clone(),
                (None, true, None) => Frag::text("{"),
                (None, false, Some(c)) => Frag::text("} else if (")
                    .append(c.clone())
                    .push(") {")
                    .clone(),
                (None, false, None) => Frag::text("} else {"),
            };
            self.line(head.at(arm_off));
            self.out.indent += 1;
            self.scopes.push(HashMap::new());
            self.emit_binds(pc.binds);
            let guard = arm.guard().and_then(|g| g.condition());
            if let Some(g) = &guard {
                let (gf, _) = self.emit_expr(g, Some(&Ty::Bool));
                self.line(
                    Frag::mapped(self.off(g.syntax()), "if (")
                        .append(gf)
                        .push(") {")
                        .clone(),
                );
                self.out.indent += 1;
            }
            if let Some(body) = arm.expr() {
                match &body {
                    ast::Expr::BlockExpr(b) if b.label().is_none() && b.modifier().is_none() => {
                        self.emit_block_body(b, dest, expected);
                    }
                    _ => {
                        self.emit_into(&body, dest, expected);
                    }
                }
            }
            if let Some(label) = &label {
                self.line(Frag::text(format!("break {label};")));
            }
            if guard.is_some() {
                self.out.indent -= 1;
                self.line(Frag::text("}"));
            }
            self.scopes.pop();
            self.out.indent -= 1;
            if label.is_some() {
                self.line(Frag::text("}"));
            }
            first = false;
            if catch_all {
                break; // later arms are unreachable
            }
        }
        if let Some(_label) = &label {
            self.out.indent -= 1;
            self.line(Frag::text("}"));
        } else if !first {
            self.line(Frag::text("}"));
        }
        ty
    }

    // ---------------------------------------------------------------- Option

    fn emit_option_method(
        &mut self,
        m: &ast::MethodCallExpr,
        recv: &ast::Expr,
        rf: Frag,
        inner: &Ty,
        method: &str,
    ) -> (Frag, Ty) {
        // (Before the arguments are emitted: `Vec::new` there isn't a value.)
        if matches!(
            method,
            "get_or_insert" | "get_or_insert_with" | "get_or_insert_default" | "insert"
        ) {
            if !is_pure(&strip_refs(recv)) {
                self.unsupported(
                    m.syntax(),
                    &format!("`Option::{method}` on this expression"),
                );
            }
            let f = self.option_fill(m, rf, inner, method);
            let f = if nullable_payload(inner) {
                f.wrap("κσ.$unsome(", ")")
            } else {
                f
            };
            return (f, inner.clone());
        }
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        // rustc's `#[track_caller]` location for a method call is the method name.
        let loc = js_string(&self.location_at(name_off));
        let args = self.emit_args(m.arg_list(), &[]);
        let a = |i: usize| {
            args.get(i)
                .cloned()
                .unwrap_or_else(|| Frag::text("undefined"))
        };
        let opt = Ty::Option(Box::new(inner.clone()));
        let helper = |name: &str, rest: Vec<Frag>, loc: Option<&str>| {
            let mut all = vec![rf.clone()];
            all.extend(rest);
            if let Some(l) = loc {
                all.push(Frag::text(l));
            }
            let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
            f.append(Frag::join(all, ", ")).push(")");
            f
        };
        // `take`/`replace` read and write the receiver, so it must be an assignable JS place.
        let swap = |cg: &mut Self, value: Frag| {
            if !is_pure(&strip_refs(recv)) {
                cg.unsupported(
                    m.syntax(),
                    &format!("`Option::{method}` on this expression"),
                );
            }
            let mut f = Frag::text("[");
            f.append(rf.clone())
                .push(", ")
                .append(rf.clone())
                .push(" = ")
                .append(value)
                .push("][0]");
            f
        };
        // `*map.get_mut(k).unwrap()`: the slot's value, a place writes go to.
        let slot = self
            .types
            .slot_calls
            .contains(&self.range_of(strip_parens_expr(recv).syntax()));
        // (`let r = map.get_mut(k).unwrap();` keeps the slot: `r` is bound to it.)
        let into_let = m
            .syntax()
            .parent()
            .is_some_and(|p| ast::LetStmt::can_cast(p.kind()));
        // (Kept as the slot: `bump(o.as_mut().unwrap())`, `return v.get_mut(i).unwrap()`.)
        let kept = self.types.slot_keeps.contains(&self.range_of(m.syntax()));
        if slot && matches!(method, "unwrap" | "expect") && !into_let && !kept {
            let f = match method {
                "unwrap" => helper("unwrap", vec![], Some(&loc)),
                _ => {
                    let msg = self
                        .emit_args(m.arg_list(), &[])
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| Frag::text("\"\""));
                    helper("expect", vec![msg], Some(&loc))
                }
            };
            return (f.wrap("", "[1]"), inner.clone());
        }
        // `*m.get_mut(k).unwrap_or(&mut 0) += 1`: the slot, or the default's.
        if slot && method == "unwrap_or" && !into_let && !kept {
            let mut f = rf.clone().wrap("(", " ?? ");
            f.append(a(0)).push(")[1]");
            return (f, inner.clone());
        }
        let nested = nullable_payload(inner);
        match method {
            "is_some" => (rf.clone().wrap("(", " != null)"), Ty::Bool),
            "is_none" => (rf.clone().wrap("(", " == null)"), Ty::Bool),
            // (`Option<Option<T>>`: what the `Some` holds.)
            "unwrap_or" if nested => (helper("unwrapOr", vec![a(0)], None), inner.clone()),
            "unwrap_or_default" if nested => (
                helper("unwrapOr", vec![Frag::text("null")], None),
                inner.clone(),
            ),
            "replace" if nested => {
                let v = a(0).wrap("κσ.$some(", ")");
                (swap(self, v), opt)
            }
            "unwrap" => (helper("unwrap", vec![], Some(&loc)), inner.clone()),
            "expect" => (helper("expect", vec![a(0)], Some(&loc)), inner.clone()),
            "unwrap_or" => {
                let mut f = rf.clone().wrap("(", " ?? ");
                f.append(a(0)).push(")");
                (f, inner.clone())
            }
            "unwrap_or_default" => match default_value(inner) {
                Some(d) => (rf.clone().wrap("(", &format!(" ?? {d})")), inner.clone()),
                None => self.no_method(m.syntax(), method, &opt),
            },
            "take" => (swap(self, Frag::text("null")), opt),
            "replace" => (swap(self, a(0)), opt),
            // `*self.timer.as_mut().unwrap() += 1`: a slot over the place (null for `None`).
            "as_mut" if self.types.slot_calls.contains(&self.range_of(m.syntax())) => {
                let f = self.opt_slot(recv).unwrap_or(rf);
                (f, opt)
            }
            "as_ref" | "as_mut" | "as_deref" | "cloned" | "copied" | "upgrade" => (rf, opt),
            "flatten" => (
                rf.wrap("κσ.$unsome(", ")"),
                self.ty_of(m.syntax()).unwrap_or(Ty::Unknown),
            ),
            "iter" | "into_iter" | "iter_mut" => (
                rf.wrap("κσ.$optIter(", ")"),
                self.ty_of(m.syntax()).unwrap_or(Ty::Unknown),
            ),
            "transpose" => {
                let ty = self.ty_of(m.syntax()).unwrap_or(Ty::Unknown);
                (helper("optTranspose", vec![], None), ty)
            }
            "clone" | "to_owned" => {
                let f = if inner.is_js_primitive() {
                    rf
                } else {
                    helper("clone", vec![], None)
                };
                (f, opt)
            }
            _ => self.no_method(m.syntax(), method, &opt),
        }
    }

    /// `opt.get_or_insert_with(f)` / `get_or_insert(v)` / `get_or_insert_default()` /
    /// `insert(v)` on the option at `rf` (an assignable place): `(opt ??= v)`, which fills it
    /// and gives what it holds (`κσ.$some`-wrapped when that's an `Option` too).
    fn option_fill(&mut self, m: &ast::MethodCallExpr, rf: Frag, inner: &Ty, method: &str) -> Frag {
        let arg = m.arg_list().and_then(|a| a.args().next());
        let value = match method {
            // (A constructor path, `Vec::new`, makes the type's default.)
            "get_or_insert_with"
                if !arg.as_ref().is_some_and(|a| {
                    matches!(a, ast::Expr::ClosureExpr(_))
                        || matches!(self.ty_of(a.syntax()), Some(Ty::Closure { .. }))
                }) =>
            {
                Frag::text(default_value(inner).unwrap_or_else(|| "undefined".into()))
            }
            "get_or_insert_with" => self.fn_arg(arg).wrap("(", ")()"),
            "get_or_insert_default" => {
                Frag::text(default_value(inner).unwrap_or_else(|| "undefined".into()))
            }
            _ => match arg {
                Some(a) => self.emit_expr(&a, Some(inner)).0,
                None => Frag::text("undefined"),
            },
        };
        let value = if nullable_payload(inner) {
            value.wrap("κσ.$some(", ")")
        } else {
            value
        };
        let op = if method == "insert" { " = " } else { " ??= " };
        let mut f = Frag::mapped(self.off(m.syntax()), "(");
        f.append(rf).push(op).append(value).push(")");
        f
    }

    /// Whether a method can assign to its receiver `recv`: a place expression, or one standing
    /// in for a place (`recv_override`).
    pub(super) fn place_receiver(&self, recv: &ast::Expr) -> bool {
        // (`m.get_mut(k).unwrap()` of a slot: its `[1]`, a place.)
        if let ast::Expr::MethodCallExpr(u) = strip_parens_expr(recv)
            && u.name_ref().is_some_and(|n| {
                matches!(n.text().to_string().as_str(), "unwrap" | "expect" | "unwrap_or" | "unwrap_or_else")
            })
            && let Some(x) = u.receiver()
            && self
                .types
                .slot_calls
                .contains(&self.range_of(strip_parens_expr(&x).syntax()))
        {
            return true;
        }
        // (A slot read through, `r.unwrap()` of a kept `ok_or`: `(..)[1]`.)
        let r = self.range_of(strip_parens_expr(recv).syntax());
        if self.types.slot_values.contains(&r) && !self.types.slot_keeps.contains(&r) {
            return true;
        }
        is_pure(&strip_refs(recv))
            || self
                .recv_override
                .as_ref()
                .is_some_and(|(r, _, _)| *r == self.range_of(recv.syntax()))
    }

    /// `*opt.get_or_insert(0) += 1` / `let r = opt.get_or_insert_with(Vec::new);`: the option
    /// as a place (a local, field or element), after `(opt ??= v)` fills it. `None` for other
    /// expressions.
    fn option_place(&mut self, e: &ast::Expr) -> Option<(Frag, Frag)> {
        let e = match strip_parens_expr(e) {
            ast::Expr::PrefixExpr(p) if p.op_kind() == Some(ast::UnaryOp::Deref) => {
                strip_parens_expr(&p.expr()?)
            }
            other => other,
        };
        let ast::Expr::MethodCallExpr(m) = e else {
            return None;
        };
        let method = m.name_ref()?.text().to_string();
        if !matches!(
            method.as_str(),
            "get_or_insert" | "get_or_insert_with" | "get_or_insert_default" | "insert"
        ) {
            return None;
        }
        let recv = m.receiver()?;
        let Some(Ty::Option(inner)) = self.ty_of(recv.syntax()) else {
            return None;
        };
        if nullable_payload(&inner) || !is_pure(&strip_refs(&recv)) {
            return None;
        }
        let (rf, _) = self.receiver(&recv);
        let fill = self.option_fill(&m, rf.clone(), &inner, &method);
        Some((fill, rf))
    }

    // ---------------------------------------------------------------- HashMap

    /// `map.entry(k).or_insert(d)` / `.or_default()` / `.or_insert_with(Vec::new)`: the map's
    /// `[key, value]` storage slot for `k`, inserting the default first if the key is missing.
    /// The map entry prelude's methods that hand out `&mut V` (`e.get_mut()`, `e.into_mut()`,
    /// `vacant.insert(v)`, `entry.or_insert(v)` of a kept entry): the map's slot for the key.
    fn prelude_entry_slot(&mut self, m: &ast::MethodCallExpr) -> Option<Frag> {
        let recv = m.receiver()?;
        let Some(Ty::Adt(n, _)) = self.ty_of(recv.syntax()) else {
            return None;
        };
        let mode = match (n.as_str(), &*m.name_ref()?.text()) {
            ("StdOccupiedEntry", "get_mut" | "into_mut") => "get",
            ("StdVacantEntry", "insert") | ("StdMapEntry", "or_insert") => "insert",
            ("StdMapEntry", "or_insert_with") => "lazy",
            _ => return None,
        };
        let (rf, _) = self.emit_expr(&recv, None);
        let arg = self
            .emit_args(m.arg_list(), &[])
            .into_iter()
            .next()
            .unwrap_or_else(|| Frag::text("undefined"));
        let mut f = Frag::mapped(self.off(m.syntax()), "κσ.$entrySlot(");
        f.append(rf)
            .push(format!(", \"{mode}\", "))
            .append(arg)
            .push(")");
        Some(f)
    }

    fn entry_slot(&mut self, m: &ast::MethodCallExpr) -> Option<Frag> {
        if let Some(f) = self.prelude_entry_slot(m) {
            return Some(f);
        }
        let method = m.name_ref()?.text().to_string();
        if !matches!(
            method.as_str(),
            "or_insert" | "or_default" | "or_insert_with" | "or_insert_with_key"
        ) {
            return None;
        }
        let ast::Expr::MethodCallExpr(mut entry) = m.receiver()? else {
            return None;
        };
        // `.and_modify(f)` between `entry` and `or_insert`: runs only when the key exists.
        let mut modify = None;
        if entry.name_ref()?.text() == "and_modify" {
            let f = entry.arg_list()?.args().next()?;
            let ast::Expr::MethodCallExpr(inner) = entry.receiver()? else {
                return None;
            };
            modify = Some(f);
            entry = inner;
        }
        if entry.name_ref()?.text() != "entry" {
            return None;
        }
        let Some(Ty::Entry(_, v)) = self.ty_of(entry.syntax()) else {
            return None;
        };
        let (mf, _) = self.receiver(&entry.receiver()?);
        let key = self
            .emit_args(entry.arg_list(), &[])
            .into_iter()
            .next()
            .unwrap_or_else(|| Frag::text("undefined"));
        let mut f = Frag::mapped(self.off(m.syntax()), "κσ.$slot(");
        f.append(mf).push(", ").append(key).push(", ");
        match method.as_str() {
            "or_insert" => {
                let value = self
                    .emit_args(m.arg_list(), &[])
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| Frag::text("undefined"));
                f.append(value);
            }
            // `or_insert_with(|| ..)`: the closure is the lazy default.
            // `or_insert_with_key(|k| ..)` gets the key too. Function values are called the same way.
            "or_insert_with" | "or_insert_with_key"
                if m.arg_list().and_then(|a| a.args().next()).is_some_and(|a| {
                    matches!(a, ast::Expr::ClosureExpr(_))
                        || matches!(self.ty_of(a.syntax()), Some(Ty::Closure { .. }))
                }) =>
            {
                let arg = m.arg_list().and_then(|a| a.args().next()).unwrap();
                let thunk = self.emit_expr(&arg, None).0;
                f.append(thunk).push(", true");
            }
            // The default is only built when the key is missing.
            _ => {
                let made = default_value(&v).unwrap_or_else(|| "undefined".into());
                f.push(format!("() => {made}, true"));
            }
        }
        if method == "or_insert" {
            f.push(", false");
        }
        if let Some(modify) = modify {
            let mf = self.slot_modifier(modify);
            f.push(", ").append(mf);
        }
        f.push(")");
        Some(f)
    }

    /// The `and_modify` closure, called with the map slot; its `v` aliases `slot[1]`.
    fn slot_modifier(&mut self, f: ast::Expr) -> Frag {
        match f {
            ast::Expr::ClosureExpr(c) => {
                let s = self.temp();
                let subjects = vec![vec![format!("{s}[1]")]];
                self.emit_closure_aliased(&c, vec![s], subjects)
            }
            other => {
                // A function value gets a copy; it can't write back.
                let (ff, _) = self.emit_expr(&other, None);
                let s = self.temp();
                Frag::text(format!("({s}) => "))
                    .append(ff)
                    .push(format!("({s}[1])"))
                    .clone()
            }
        }
    }

    /// A bare `map.entry(k).and_modify(f);` with no `or_*` after it.
    fn entry_and_modify(&mut self, m: &ast::MethodCallExpr) -> Option<Frag> {
        if m.name_ref()?.text() != "and_modify" {
            return None;
        }
        let ast::Expr::MethodCallExpr(entry) = m.receiver()? else {
            return None;
        };
        if entry.name_ref()?.text() != "entry" {
            return None;
        }
        let (mf, _) = self.receiver(&entry.receiver()?);
        let key = self
            .emit_args(entry.arg_list(), &[])
            .into_iter()
            .next()
            .unwrap_or_else(|| Frag::text("undefined"));
        let modifier = self.slot_modifier(m.arg_list()?.args().next()?);
        let mut f = Frag::mapped(self.off(m.syntax()), "κσ.$andModify(");
        f.append(mf)
            .push(", ")
            .append(key)
            .push(", ")
            .append(modifier)
            .push(")");
        Some(f)
    }

    /// `κσ.$optSlot(..)` over the `Option` in the place `e`: a field, an element, or a local
    /// (in a cell, or a loop's alias of an element) (see `typeck`'s `mark_as_mut_slot`).
    fn opt_slot(&mut self, e: &ast::Expr) -> Option<Frag> {
        match strip_parens_expr(e) {
            ast::Expr::FieldExpr(fe) => {
                let (b, n) = (fe.expr()?, fe.name_ref()?);
                let (bf, _) = self.emit_expr(&b, None);
                let key = js_string(&adt_gen::prop(n.text()));
                let mut f = Frag::text("κσ.$optSlot(");
                f.append(bf).push(format!(", {key}, null)"));
                Some(f)
            }
            ast::Expr::IndexExpr(ix) => {
                let (b, i) = (ix.base()?, ix.index()?);
                let (bf, _) = self.emit_expr(&b, None);
                let (xf, _) = self.emit_expr(&i, None);
                let loc = js_string(&self.location(ix.syntax()));
                let mut f = Frag::text("κσ.$optSlot(");
                f.append(bf).push(", ").append(xf).push(format!(", {loc})"));
                Some(f)
            }
            ast::Expr::PathExpr(p) => {
                let name = p.path()?.as_single_name_ref()?;
                let js = self.lookup(name.text())?.js.clone();
                let cell = match js.strip_suffix("[0]") {
                    Some(cell) => cell.to_string(),
                    None if js.ends_with(']') || js.contains('.') => {
                        format!("κσ.$cell(() => {js}, ($v) => {{ {js} = $v; }})")
                    }
                    None => return None,
                };
                Some(Frag::text(format!("κσ.$optSlot({cell}, 0, null)")))
            }
            _ => None,
        }
    }

    /// `entry_slot` for `*map.entry(..).or_insert(..)` (the deref is erased).
    fn entry_slot_of(&mut self, e: &ast::Expr) -> Option<Frag> {
        match e {
            ast::Expr::PrefixExpr(p) if p.op_kind() == Some(UnaryOp::Deref) => {
                self.entry_slot_of(&p.expr()?)
            }
            ast::Expr::ParenExpr(p) => self.entry_slot_of(&p.expr()?),
            ast::Expr::MethodCallExpr(m) => self.entry_slot(m),
            _ => None,
        }
    }

    fn emit_map_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: Frag,
        k: &Ty,
        v: &Ty,
        method: &str,
    ) -> (Frag, Ty) {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        let args = self.emit_args(m.arg_list(), &[]);
        let a = |i: usize| {
            args.get(i)
                .cloned()
                .unwrap_or_else(|| Frag::text("undefined"))
        };
        let call = |js: &str, rest: Vec<Frag>| {
            let mut f = rf.clone();
            f.push(".")
                .append(Frag::mapped(name_off, format!("${js}(")));
            f.append(Frag::join(rest, ", ")).push(")");
            f
        };
        let opt = Ty::Option(Box::new(v.clone()));
        let map_ty = Ty::Map(Box::new(k.clone()), Box::new(v.clone()));
        match method {
            "insert" => (call("insert", vec![a(0), a(1)]), opt),
            "get_mut" if self.types.slot_calls.contains(&self.range_of(m.syntax())) => {
                let mut f = Frag::mapped(name_off, "κσ.$slotOf(");
                f.append(rf.clone()).push(", ").append(a(0)).push(")");
                (f, opt)
            }
            "get" | "get_mut" => (call("get", vec![a(0)]), opt),
            "remove" => (call("remove", vec![a(0)]), opt),
            "contains_key" => (call("containsKey", vec![a(0)]), Ty::Bool),
            "len" => (call("len", vec![]), Ty::Int(IntKind::Usize)),
            "is_empty" => (call("len", vec![]).wrap("(", " === 0)"), Ty::Bool),
            "clear" => (call("clear", vec![]), Ty::Unit),
            "clone" | "to_owned" => (call("clone", vec![]), map_ty),
            // The prelude's `Entry`, over this map.
            "entry" if matches!(self.ty_of(m.syntax()), Some(Ty::Adt(ref n, _)) if n == "StdMapEntry") =>
            {
                let mut f = Frag::mapped(name_off, "__std_map_entry(");
                f.append(rf.clone()).push(", ").append(a(0)).push(")");
                (f, self.ty_of(m.syntax()).unwrap_or(Ty::Unknown))
            }
            "entry" | "keys" | "values" | "values_mut" | "iter" | "iter_mut" | "into_iter"
            | "into_keys" | "into_values" => {
                let what = if method == "entry" {
                    "`.entry(..)` without `.or_insert(..)` / `.or_default()`".to_string()
                } else {
                    format!("`.{method}()` outside a `for` loop")
                };
                // (Values of a type defined elsewhere: the checker said so already.)
                if !(*v == Ty::Unknown && self.type_errors && method != "entry") {
                    self.unsupported(m.syntax(), &what);
                }
                (Frag::text("undefined"), Ty::Unknown)
            }
            _ => self.no_method(m.syntax(), method, &map_ty),
        }
    }

    /// `for .. in &map / map.iter() / .keys() / .values() / .iter_mut() / .values_mut()`: the map
    /// expression, which part of each entry the loop binds, and whether bindings write through.
    fn map_iteration(&self, iterable: &ast::Expr) -> Option<(ast::Expr, MapPart, bool)> {
        let is_map = |e: &ast::Expr| matches!(self.ty_of(e.syntax()), Some(Ty::Map(..)));
        match iterable {
            ast::Expr::RefExpr(r) => {
                let inner = r.expr()?;
                is_map(&inner).then(|| (inner, MapPart::Entries, r.mut_token().is_some()))
            }
            ast::Expr::MethodCallExpr(m)
                if m.arg_list().is_some_and(|a| a.args().next().is_none()) =>
            {
                let recv = m.receiver()?;
                if !is_map(&recv) {
                    return None;
                }
                let (part, mutable) = match m.name_ref()?.text() {
                    "iter" | "into_iter" => (MapPart::Entries, false),
                    "iter_mut" => (MapPart::Entries, true),
                    "keys" | "into_keys" => (MapPart::Keys, false),
                    "values" | "into_values" => (MapPart::Values, false),
                    "values_mut" => (MapPart::Values, true),
                    _ => return None,
                };
                Some((recv, part, mutable))
            }
            e if is_map(e) => Some((e.clone(), MapPart::Entries, false)),
            _ => None,
        }
    }

    fn emit_for_map(
        &mut self,
        f: &ast::ForExpr,
        map: &ast::Expr,
        part: MapPart,
        mutable: bool,
        label: &str,
    ) {
        let start = self.off(f.syntax());
        let (mf, _) = self.receiver(map);
        let Some(pat) = f.pat() else { return };
        let slot = self.temp();
        let mut head = Frag::mapped(start, format!("{label}for (const {slot} of "));
        head.append(mf).push(".$slots()) {");
        self.line(head);
        self.scopes.push(HashMap::new());
        self.out.indent += 1;
        let subject = match part {
            MapPart::Entries => slot.clone(),
            MapPart::Keys => format!("{slot}[0]"),
            MapPart::Values => format!("{slot}[1]"),
        };
        let mut pc = PatCode::default();
        self.lower_pat(&pat, &subject, &mut pc);
        if mutable {
            // Bindings alias the slot, so `*v += 1` updates the map.
            self.alias_binds(pc.binds);
        } else {
            self.emit_binds(pc.binds);
        }
        self.out.indent -= 1;
        self.emit_body(f.loop_body());
        self.scopes.pop();
    }

    // ---------------------------------------------------------------- HashSet

    fn emit_set_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: Frag,
        elem: &Ty,
        method: &str,
    ) -> (Frag, Ty) {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        let args = self.emit_args(m.arg_list(), &[]);
        let a = |i: usize| {
            args.get(i)
                .cloned()
                .unwrap_or_else(|| Frag::text("undefined"))
        };
        // Runtime methods are `$`-prefixed so Bitburner's RAM calculator never mistakes them for
        // NS functions (`ns.clear` exists, for one).
        let call = |js: &str, rest: Vec<Frag>| {
            let mut f = rf.clone();
            f.push(".")
                .append(Frag::mapped(name_off, format!("${js}(")));
            f.append(Frag::join(rest, ", ")).push(")");
            f
        };
        let set_ty = Ty::Set(Box::new(elem.clone()));
        match method {
            "insert" => (call("insert", vec![a(0)]), Ty::Bool),
            "contains" => (call("contains", vec![a(0)]), Ty::Bool),
            "remove" => (call("remove", vec![a(0)]), Ty::Bool),
            "len" => (call("len", vec![]), Ty::Int(IntKind::Usize)),
            "is_empty" => (call("len", vec![]).wrap("(", " === 0)"), Ty::Bool),
            "clear" => (call("clear", vec![]), Ty::Unit),
            "clone" | "to_owned" => (call("clone", vec![]), set_ty),
            "is_subset" => (call("isSubset", vec![a(0)]), Ty::Bool),
            "is_superset" => (call("isSuperset", vec![a(0)]), Ty::Bool),
            "is_disjoint" => (call("isDisjoint", vec![a(0)]), Ty::Bool),
            "iter" | "into_iter" => {
                self.unsupported(
                    m.syntax(),
                    &format!("`.{method}()` outside a `for` loop"),
                );
                (Frag::text("undefined"), Ty::Unknown)
            }
            _ => self.no_method(m.syntax(), method, &set_ty),
        }
    }

    // ---------------------------------------------------------------- Vec

    /// Element type of the `Vec` a node evaluates to, per the type checker.
    fn vec_elem(&self, n: &SyntaxNode) -> Ty {
        match self.ty_of(n) {
            Some(Ty::Vec(e)) => *e,
            _ => Ty::Unknown,
        }
    }

    /// `vec![value; count]` / `[value; count]`. Non-primitive values (e.g. inner Vecs) are cloned
    /// per element, as Rust does, instead of sharing one JS array.
    fn vec_repeat(&self, value: Frag, count: Frag, elem: &Ty) -> Frag {
        let mut f = Frag::text("κσ.$vecRepeat(");
        f.append(value).push(", ").append(count);
        if !elem.is_js_primitive() {
            f.push(", true");
        }
        f.push(")");
        f
    }

    fn emit_array(&mut self, a: &ast::ArrayExpr) -> (Frag, Ty) {
        let elem = self.vec_elem(a.syntax());
        let f = match a.kind() {
            ast::ArrayExprKind::ElementList(items) => {
                let items: Vec<Frag> = items
                    .map(|i| {
                        let (f, t) = self.emit_expr(&i, None);
                        self.by_value(&i, f, &t)
                    })
                    .collect();
                Frag::text("[")
                    .append(Frag::join(items, ", "))
                    .push("]")
                    .clone()
            }
            ast::ArrayExprKind::Repeat {
                initializer,
                repeat,
            } => {
                let vf = initializer
                    .map(|e| self.emit_expr(&e, None).0)
                    .unwrap_or_else(|| Frag::text("undefined"));
                let cf = repeat
                    .map(|e| self.emit_expr(&e, None).0)
                    .unwrap_or_else(|| Frag::text("0"));
                self.vec_repeat(vf, cf, &elem)
            }
        };
        (f, Ty::Vec(Box::new(elem)))
    }

    /// `x.as_mut()` / `x.as_mut_slice()` of a `Vec` `x` that's safe to evaluate twice: `x`, the
    /// same array (so `b.as_mut()[0] = 7` for `B: AsMut<[u8]>` stays assignable).
    fn same_vec_call(&self, e: &ast::Expr) -> Option<ast::Expr> {
        let ast::Expr::MethodCallExpr(m) = strip_parens_expr(e) else {
            return None;
        };
        let name = m.name_ref()?.text().to_string();
        let r = m.receiver()?;
        (matches!(name.as_str(), "as_mut" | "as_mut_slice")
            && m.arg_list().is_some_and(|l| l.args().next().is_none())
            && is_pure(&r)
            && matches!(self.ty_of(r.syntax()), Some(Ty::Vec(_))))
        .then_some(r)
    }

    /// `v[i]` with Rust's bounds check: `v[κσ.$ix(v, i, loc)]`, which stays assignable. When `v`
    /// isn't safe to evaluate twice, reads go through `κσ.$get` instead.
    fn emit_index(&mut self, ix: &ast::IndexExpr) -> (Frag, Ty) {
        let (Some(base), Some(index)) = (ix.base(), ix.index()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let base = self.same_vec_call(&base).unwrap_or(base);
        // `grid[i]` with an `impl Index`: `grid.index$0(i)`.
        if let Some(crate::adt::Conv::Method { key }) = self.conv_of(ix.syntax()) {
            let (bf, _) = self.receiver(&base);
            let (f, t) = self.emit_expr(&index, None);
            let f = self.by_value(&index, f, &t);
            let ty = self.ty_of(ix.syntax()).unwrap_or(Ty::Unknown);
            let mut out = bf.wrap("(", ")");
            out.push(format!(".{key}(")).append(f).push(")");
            // Assigned through `IndexMut`: the place is the cell's `[0]`.
            if self
                .types
                .index_places
                .contains(&self.range_of(ix.syntax()))
            {
                out.push("[0]");
            }
            return (out.at(self.off(ix.syntax())), ty);
        }
        // `v[r]` / `&s[r]` with a range value.
        if matches!(self.raw_ty_of(index.syntax()), Some(Ty::Range(..))) {
            let (bf, bt) = self.receiver(&base);
            let (rf, _) = self.emit_expr(&index, None);
            let loc = js_string(&self.location(ix.syntax()));
            let name = if bt == Ty::Str { "$strSlice" } else { "$slice" };
            let mut f = Frag::mapped(self.off(ix.syntax()), format!("κσ.$sliceBy(\"{name}\", "));
            f.append(bf).push(", ").append(rf).push(format!(", {loc})"));
            let ty = self.ty_of(ix.syntax()).unwrap_or(bt);
            return (f, ty);
        }
        if let ast::Expr::RangeExpr(r) = &index {
            use ra_ap_syntax::ast::RangeItem;
            let (bf, bt) = self.receiver(&base);
            let lo = r
                .start()
                .map(|e| self.emit_expr(&e, None).0)
                .unwrap_or_else(|| Frag::text("0"));
            let hi = r
                .end()
                .map(|e| self.emit_expr(&e, None).0)
                .unwrap_or_else(|| Frag::text("null"));
            let inclusive = r.op_kind() == Some(RangeOp::Inclusive);
            // Like indexing, rustc reports slicing panics at the `[`.
            let bracket = ix
                .l_brack_token()
                .map(|t| self.base + usize::from(t.text_range().start()));
            let loc = js_string(
                &bracket
                    .map(|o| self.location_at(o))
                    .unwrap_or_else(|| self.location(ix.syntax())),
            );
            let name = if bt == Ty::Str {
                "strSlice"
            } else if std::mem::take(&mut self.slice_view)
                || self.types.slice_views.contains(&self.range_of(ix.syntax()))
            {
                "sliceMut"
            } else {
                "slice"
            };
            let mut f = Frag::mapped(self.off(ix.syntax()), format!("κσ.${name}("));
            f.append(bf)
                .push(", ")
                .append(lo)
                .push(", ")
                .append(hi)
                .push(format!(", {inclusive}, {loc})"));
            let ty = self.ty_of(ix.syntax()).unwrap_or(bt);
            return (f, ty);
        }
        // rustc reports index panics at the `[`.
        let bracket = ix
            .l_brack_token()
            .map(|t| self.base + usize::from(t.text_range().start()));
        let loc = js_string(
            &bracket
                .map(|o| self.location_at(o))
                .unwrap_or_else(|| self.location(ix.syntax())),
        );
        // `s.as_bytes()[i]`: the byte at that UTF-8 offset (`κσ.$byteAt`), without building the
        // bytes of an ASCII string. (A slice's index panic is reported at the expression.)
        if let ast::Expr::MethodCallExpr(mc) = strip_parens_expr(&base)
            && mc.name_ref().is_some_and(|n| n.text() == "as_bytes")
            && mc.arg_list().is_none_or(|l| l.args().next().is_none())
            && let Some(recv) = mc.receiver()
            && matches!(self.ty_of(recv.syntax()), Some(Ty::Str))
            && self.types.adts.method("String", "as_bytes").is_none()
            && self.recv_override.is_none()
        {
            let (sf, st) = self.receiver(&recv);
            if st == Ty::Str {
                let (xf, _) = self.emit_expr(&index, None);
                let at = js_string(&self.location(ix.syntax()));
                let mut f = Frag::mapped(self.off(ix.syntax()), "κσ.$byteAt(");
                f.append(sf).push(", ").append(xf).push(format!(", {at})"));
                return (f, Ty::Int(IntKind::U8));
            }
        }
        let (bf, bt) = self.receiver(&base);
        let (xf, _) = self.emit_expr(&index, None);
        if let Ty::Map(_, v) = &bt {
            let mut f = Frag::text("κσ.$mapIndex(");
            f.append(bf).push(", ").append(xf).push(format!(", {loc})"));
            return (f, (**v).clone());
        }
        let elem = match bt {
            Ty::Vec(e) => *e,
            _ => {
                // Untyped values (NS results): plain JS indexing.
                let f = bf.clone().push("[").append(xf).push("]").clone();
                return (f, Ty::Unknown);
            }
        };
        // A `VecDeque` (a ring buffer): `q.b[q.$i(i, loc)]`, its backing array at the checked
        // ring index, which is a place like a Vec's; `κσ.$dqGet` when `q` isn't safe to evaluate
        // twice.
        if matches!(
            self.raw_ty_of(strip_refs(&base).syntax()),
            Some(Ty::Coll(crate::ty::Coll::VecDeque, _))
        ) {
            let f = if is_pure(&base) {
                let mut f = bf.clone();
                f.push(".b[")
                    .append(bf)
                    .push(".$i(")
                    .append(xf)
                    .push(format!(", {loc})]"));
                f
            } else {
                let mut f = Frag::text("κσ.$dqGet(");
                f.append(bf).push(", ").append(xf).push(format!(", {loc})"));
                f
            };
            return (f, elem);
        }
        // (A view `v[a..b]` is fine to build twice: both write to `v`.)
        let view = self
            .types
            .slice_views
            .contains(&self.range_of(strip_parens_expr(&base).syntax()));
        // A read of `grid[i][j]`: the row once (`κσ.$get`), where the inline check would read
        // it three times. (A hoisted row, `recv_override`, is a plain name.)
        // (A call hoisted into a temp by `emit_assign_place` reads as a plain name.)
        let pure = is_pure(&base)
            || self
                .recv_override
                .as_ref()
                .is_some_and(|(r, ..)| *r == self.range_of(base.syntax()));
        let nested_read = matches!(strip_parens_expr(&base), ast::Expr::IndexExpr(_))
            && !index_is_place(ix)
            && !self
                .recv_override
                .as_ref()
                .is_some_and(|(r, ..)| *r == self.range_of(base.syntax()));
        let f = if nested_read {
            let mut f = Frag::text("κσ.$get(");
            f.append(bf).push(", ").append(xf).push(format!(", {loc})"));
            f
        } else if pure && !view && cheap_index(&index) {
            // The bounds check inline (about twice as fast as calling `κσ.$ix`):
            // `v[(i >= 0 && i < v.length ? i : κσ.$oob(v, i, loc))]`.
            let x = if matches!(
                strip_parens_expr(&index),
                ast::Expr::PathExpr(_) | ast::Expr::Literal(_)
            ) {
                xf
            } else {
                xf.wrap("(", ")")
            };
            let mut f = bf.clone();
            f.push("[(")
                .append(x.clone())
                .push(" >= 0 && ")
                .append(x.clone())
                .push(" < ")
                .append(bf.clone())
                .push(".length ? ")
                .append(x.clone())
                .push(" : κσ.$oob(")
                .append(bf)
                .push(", ")
                .append(x)
                .push(format!(", {loc}))]"));
            f
        } else if pure || view {
            let mut f = bf.clone();
            f.push("[κσ.$ix(")
                .append(bf)
                .push(", ")
                .append(xf)
                .push(format!(", {loc})]"));
            f
        } else {
            let mut f = Frag::text("κσ.$get(");
            f.append(bf).push(", ").append(xf).push(format!(", {loc})"));
            f
        };
        (f, elem)
    }

    fn emit_vec_method(
        &mut self,
        m: &ast::MethodCallExpr,
        rf: Frag,
        elem: &Ty,
        method: &str,
    ) -> (Frag, Ty) {
        let name_off = m.name_ref().map(|n| self.off(n.syntax())).unwrap_or(0);
        // rustc's `#[track_caller]` location for a method call is the method name.
        let loc = Frag::text(js_string(&self.location_at(name_off)));
        let args = self.emit_args(m.arg_list(), &[]);
        let a = |i: usize| {
            args.get(i)
                .cloned()
                .unwrap_or_else(|| Frag::text("undefined"))
        };
        // `κσ.$name(v, args...)`
        let helper = |name: &str, rest: Vec<Frag>| {
            let mut f = Frag::mapped(name_off, format!("κσ.${name}("));
            let mut all = vec![rf.clone()];
            all.extend(rest);
            f.append(Frag::join(all, ", ")).push(")");
            f
        };
        // `v.name(args...)`
        let mcall = |name: &str, rest: Vec<Frag>| {
            let mut f = rf.clone();
            f.push(".")
                .append(Frag::mapped(name_off, format!("{name}(")));
            f.append(Frag::join(rest, ", ")).push(")");
            f
        };
        let vec_ty = Ty::Vec(Box::new(elem.clone()));
        let usize = Ty::Int(IntKind::Usize);
        if self.types.slot_calls.contains(&self.range_of(m.syntax())) {
            // A slot over the element (`{ get 1(), set 1() }`), or null when there's none.
            let f = match method {
                // Of a `Vec<&mut i32>`: the stored slot itself.
                "first_mut" | "last_mut" | "get_mut"
                    if self.types.slot_item_calls.contains(&self.range_of(m.syntax())) =>
                {
                    let i = if method == "get_mut" { a(0) } else { Frag::text(if method == "first_mut" { "0" } else { "-1" }) };
                    helper("vecItem", vec![i])
                }
                // `iter_mut()` outside a loop by index: an iterator of slots.
                "iter_mut" => helper("iterMutSlots", vec![]),
                "first_mut" => helper("vecSlot", vec![Frag::text("0")]),
                "last_mut" => helper("vecSlot", vec![Frag::text("-1")]),
                _ => helper("vecSlot", vec![a(0)]),
            };
            return (f, Ty::Option(Box::new(elem.clone())));
        }
        // `v.get(1..3)` / `dest.get_mut(n..)`: `Some` of the slice (a view for `get_mut`, which
        // writes reach), `None` past the end.
        if matches!(method, "get" | "get_mut")
            && let Some(ast::Expr::RangeExpr(r)) = m
                .arg_list()
                .and_then(|l| l.args().next())
                .map(|e| crate::typeck::strip_parens(&e).clone())
        {
            use ra_ap_syntax::ast::RangeItem;
            let mut bound = |e: Option<ast::Expr>, none: &str| match e {
                Some(e) => self.emit_expr(&e, Some(&usize)).0,
                None => Frag::text(none),
            };
            let lo = bound(r.start(), "0");
            let hi = bound(r.end(), "null");
            let incl = r.op_kind() == Some(ast::RangeOp::Inclusive);
            let mut f = Frag::mapped(name_off, "κσ.$getRange(");
            f.append(rf.clone())
                .push(", ")
                .append(lo)
                .push(", ")
                .append(hi)
                .push(format!(", {incl}, {})", method == "get_mut"));
            return (f, Ty::Option(Box::new(vec_ty)));
        }
        match method {
            "len" => (rf.clone().push(".length").clone(), usize),
            "pop" => (helper("pop", vec![]), Ty::Option(Box::new(elem.clone()))),
            "first" | "first_mut" => (helper("first", vec![]), Ty::Option(Box::new(elem.clone()))),
            "last" | "last_mut" => (helper("last", vec![]), Ty::Option(Box::new(elem.clone()))),
            "get" | "get_mut" => (
                helper("vget", vec![a(0)]),
                Ty::Option(Box::new(elem.clone())),
            ),
            "is_empty" => (rf.clone().wrap("(", ".length === 0)"), Ty::Bool),
            "push" => (mcall("push", vec![a(0)]), Ty::Unit),
            "insert" => (helper("vecInsert", vec![a(0), a(1), loc]), Ty::Unit),
            "remove" => (helper("vecRemove", vec![a(0), loc]), elem.clone()),
            "swap" => (helper("vecSwap", vec![a(0), a(1), loc]), Ty::Unit),
            "truncate" => (helper("vecTruncate", vec![a(0)]), Ty::Unit),
            "clear" => (helper("vecClear", vec![]), Ty::Unit),
            "reverse" => (mcall("reverse", vec![]), Ty::Unit),
            // JS's default sort compares as strings; κσ.$cmp is Rust's `Ord` for numbers,
            // strings, bools and nested Vecs (`$cmpNum` / `$cmpStr`: the same, without the
            // checks, for integers that are JS numbers and for strings). Both sorts are stable.
            "sort" | "sort_unstable" => {
                let cmp = match elem {
                    Ty::Int(k) if !k.is_big() => "κσ.$cmpNum",
                    Ty::IntLit => "κσ.$cmpNum",
                    Ty::Str | Ty::Char => "κσ.$cmpStr",
                    _ => "κσ.$cmp",
                };
                (mcall("sort", vec![Frag::text(cmp)]), Ty::Unit)
            }
            // (`includes` is `===` on numbers, strings and bools; not for floats, whose NaN
            // never equals itself in Rust.)
            "contains" if elem.is_js_primitive() && !matches!(elem, Ty::Float { .. }) => {
                (mcall("includes", vec![a(0)]), Ty::Bool)
            }
            "contains" => (helper("contains", vec![a(0)]), Ty::Bool),
            "extend_from_slice" => (helper("extend", vec![a(0)]), Ty::Unit),
            "join" => (mcall("join", vec![a(0)]), Ty::Str),
            "concat" if matches!(elem, Ty::Vec(_)) => {
                let flat = mcall("flat", vec![]);
                let f = match elem {
                    Ty::Vec(inner) if !inner.is_js_primitive() => flat.wrap("", ".map(κσ.$clone)"),
                    _ => flat,
                };
                (f, elem.clone())
            }
            "concat" => (mcall("join", vec![Frag::text("\"\"")]), Ty::Str),
            "clone" | "to_owned" | "to_vec" => {
                // Elements that are immutable in JS only need a shallow copy.
                let f = if elem.is_js_primitive() {
                    mcall("slice", vec![])
                } else {
                    helper("clone", vec![])
                };
                (f, vec_ty)
            }
            "iter" | "iter_mut" | "into_iter" => {
                // (Elements of a type defined elsewhere: the checker said so already.)
                if !(*elem == Ty::Unknown && self.type_errors) {
                    self.unsupported(
                        m.syntax(),
                        &format!("`.{method}()` outside a `for` loop"),
                    );
                }
                (Frag::text("undefined"), Ty::Unknown)
            }
            _ => self.no_method(m.syntax(), method, &vec_ty),
        }
    }

    fn emit_ternary(&mut self, i: &ast::IfExpr, expected: Option<&Ty>) -> (Frag, Ty) {
        let cond = i.condition().unwrap();
        let (cf, _) = self.operand(&cond, Some(&Ty::Bool));
        let (tf, tt) = self.emit_expr(&simple_tail(&i.then_branch().unwrap()).unwrap(), expected);
        let (ef, et) = match i.else_branch().unwrap() {
            ElseBranch::Block(b) => self.emit_expr(&simple_tail(&b).unwrap(), Some(&tt)),
            ElseBranch::IfExpr(ei) => self.emit_ternary(&ei, Some(&tt)),
        };
        let mut f = cf;
        f.push(" ? ").append(tf).push(" : ").append(ef);
        (f.wrap("(", ")"), Ty::unify(&tt, &et))
    }

    // ---------------------------------------------------------------- macros

    fn macro_args(&mut self, mc: &ast::MacroCall) -> Vec<MacroArg> {
        macros::split_args(self.src, self.base, mc)
    }

    /// Parse a macro argument as an expression and lower it.
    fn emit_macro_arg(&mut self, arg: &MacroArg, expected: Option<&Ty>) -> (Frag, Ty) {
        let (expr, errors) = macros::parse_arg(arg);
        for (start, end, message) in errors {
            self.diag_range(start, end, message, "error");
        }
        let Some(expr) = expr else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let saved = std::mem::replace(&mut self.base, arg.start);
        let res = self.emit_expr(&expr, expected);
        self.base = saved;
        res
    }

    /// A macro argument in a by-value position (a `vec!` element): copies tuples and `Copy`
    /// structs read from places.
    fn emit_macro_arg_by_value(&mut self, arg: &MacroArg) -> Frag {
        let (expr, _) = macros::parse_arg(arg);
        let (f, ty) = self.emit_macro_arg(arg, None);
        match expr {
            Some(e) => self.by_value(&e, f, &ty),
            None => f,
        }
    }

    fn emit_macro(&mut self, mc: &ast::MacroCall) -> (Frag, Ty) {
        let name = mc
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string());
        let Some(name) = name else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let start = self.off(mc.syntax());
        let loc = js_string(&self.location(mc.syntax()));
        // `debug_assert!` and friends: checked only in debug builds (overflow checks on), like
        // rustc without `-O`.
        let name = match name.strip_prefix("debug_") {
            Some(n @ ("assert" | "assert_eq" | "assert_ne")) if overflow_checks() => n.to_string(),
            Some("assert" | "assert_eq" | "assert_ne") => {
                return (Frag::mapped(start, "undefined"), Ty::Unit);
            }
            _ => name,
        };
        match name.as_str() {
            "stringify" => {
                return (
                    Frag::mapped(start, js_string(&macros::stringify(mc))),
                    Ty::Str,
                );
            }
            "cfg" => {
                let v = macros::eval_cfg(mc, overflow_checks()).unwrap_or(false);
                return (Frag::mapped(start, v.to_string()), Ty::Bool);
            }
            _ => {}
        }
        let args = self.macro_args(mc);
        let rt = |f: &str| format!("κσ.${f}");
        match name.as_str() {
            "quote" | "quote_spanned" => self.emit_quote(mc),
            // (An error from the type checker.)
            "compile_error" => (Frag::text("undefined"), Ty::Never),
            // `format_ident!(..)`: an `Ident` (in a proc-macro crate).
            "format_ident" if self.types.adts.defs.contains_key("Ident") => {
                let args: Vec<_> = args
                    .into_iter()
                    .filter(|a| a.name.as_deref() != Some("span"))
                    .collect();
                let tpl = if args.is_empty() {
                    Frag::text("\"\"")
                } else {
                    self.format_args(mc, &args)
                };
                (
                    self.construct("Ident", 0, vec![tpl]),
                    Ty::Adt("Ident".into(), vec![]),
                )
            }
            // `dbg!(a, b)`: `((t1, t2) => (κσ.$eprintln(`[file:1:2] a = ${..}`), .., [t1, t2]))(a, b)`.
            "dbg" => {
                let head = format!("[{}]", self.location(mc.syntax()));
                if args.is_empty() {
                    let mut f = Frag::mapped(start, format!("{}(", rt("eprintln")));
                    f.push(js_string(&head)).push(")");
                    return (f, Ty::Unit);
                }
                let pretty = Spec {
                    debug: true,
                    alt: true,
                    ..Spec::default()
                };
                let mut temps = Vec::new();
                let mut values = Vec::new();
                let mut tys = Vec::new();
                let mut prints = Vec::new();
                for a in &args {
                    let (expr, _) = macros::parse_arg(a);
                    let (f, t) = self.emit_macro_arg(a, None);
                    let f = match expr {
                        Some(e) => self.by_value(&e, f, &t),
                        None => f,
                    };
                    let t = t.default_lit();
                    let tmp = self.temp();
                    let shown = template_escape(&format!("{head} {} = ", a.text.trim()));
                    let mut p = Frag::text(format!("{}(`{shown}${{", rt("eprintln")));
                    p.append(format_value(Frag::text(tmp.clone()), &t, &pretty, None, None))
                        .push("}`)");
                    prints.push(p);
                    temps.push(tmp);
                    values.push(f);
                    tys.push(t);
                }
                let result = if temps.len() == 1 {
                    temps[0].clone()
                } else {
                    format!("[{}]", temps.join(", "))
                };
                let mut f = Frag::mapped(start, format!("(({}) => (", temps.join(", ")));
                f.append(Frag::join(prints, ", "))
                    .push(format!(", {result}))("))
                    .append(Frag::join(values, ", "))
                    .push(")");
                let ty = if tys.len() == 1 { tys.pop().unwrap() } else { Ty::Tuple(tys) };
                (f, ty)
            }
            "println" | "print" | "eprintln" | "eprint" | "format" => {
                let tpl = if args.is_empty() {
                    Frag::text("\"\"")
                } else {
                    self.format_args(mc, &args)
                };
                if name == "format" {
                    return (tpl, Ty::Str);
                }
                let mut f = Frag::mapped(start, format!("{}(", rt(&name)));
                f.append(tpl).push(")");
                (f, Ty::Unit)
            }
            "panic" | "unreachable" | "todo" | "unimplemented" => {
                let default = match name.as_str() {
                    "panic" => "explicit panic",
                    "unreachable" => "internal error: entered unreachable code",
                    "todo" => "not yet implemented",
                    _ => "not implemented",
                };
                let msg = if args.is_empty() {
                    Frag::text(js_string(default))
                } else {
                    let tpl = self.format_args(mc, &args);
                    if name == "panic" {
                        tpl
                    } else {
                        Frag::text(format!("{} + ", js_string(&format!("{default}: "))))
                            .append(tpl)
                            .clone()
                    }
                };
                let mut f = Frag::mapped(start, format!("{}(", rt("panic")));
                f.append(msg).push(format!(", {loc})"));
                (f, Ty::Never)
            }
            "assert" => {
                let Some(cond) = args.first() else {
                    self.error(mc.syntax(), "`assert!` requires a boolean argument");
                    return (Frag::text("undefined"), Ty::Unit);
                };
                let (cf, _) = self.emit_macro_arg(cond, Some(&Ty::Bool));
                let msg = if args.len() > 1 {
                    self.format_args(mc, &args[1..])
                } else {
                    Frag::text(js_string(&format!("assertion failed: {}", cond.text)))
                };
                let mut f = cf.wrap("(", ")");
                f.push(format!(" || {}(", rt("panic")))
                    .append(msg)
                    .push(format!(", {loc})"));
                (f, Ty::Unit)
            }
            "assert_eq" | "assert_ne" => {
                if args.len() < 2 {
                    self.error(mc.syntax(), format!("`{name}!` requires two arguments"));
                    return (Frag::text("undefined"), Ty::Unit);
                }
                let (lf, lt) = self.emit_macro_arg(&args[0], None);
                let (rf, rt_) = self.emit_macro_arg(&args[1], Some(&lt));
                let ty = Ty::unify(&lt, &rt_).default_lit();
                let helper = if name == "assert_eq" {
                    "assertEq"
                } else {
                    "assertNe"
                };
                // The runtime formats operands with `{:?}` itself, so they're evaluated once.
                let dbg = debug_fn(&ty).unwrap_or_else(|| "undefined".into());
                let mut f = Frag::mapped(start, format!("{}(", rt(helper)));
                f.append(lf)
                    .push(", ")
                    .append(rf)
                    .push(format!(", {loc}, {dbg}"));
                if args.len() > 2 {
                    // Custom messages are only formatted on failure, like Rust.
                    let msg = self.format_args(mc, &args[2..]);
                    f.push(", () => ").append(msg);
                }
                f.push(")");
                (f, Ty::Unit)
            }
            "vec" => {
                let elem = self.vec_elem(mc.syntax());
                if let Some((value, count)) = macros::split_repeat(self.src, self.base, mc) {
                    let (vf, _) = self.emit_macro_arg(&value, None);
                    let (cf, _) = self.emit_macro_arg(&count, None);
                    return (
                        self.vec_repeat(vf, cf, &elem).at(start),
                        Ty::Vec(Box::new(elem)),
                    );
                }
                let items: Vec<Frag> = args
                    .iter()
                    .map(|a| self.emit_macro_arg_by_value(a))
                    .collect();
                let mut f = Frag::mapped(start, "[");
                f.append(Frag::join(items, ", ")).push("]");
                (f, Ty::Vec(Box::new(elem)))
            }
            "matches" => {
                let Some(arg) = macros::matches_as_match(&args) else {
                    return (Frag::text("false"), Ty::Bool);
                };
                let (expr, _) = macros::parse_arg(&arg);
                let Some(expr) = expr else {
                    return (Frag::text("false"), Ty::Bool);
                };
                // An arrow function called in place, so the `match` stays an expression.
                // (Its arms return a `bool` whatever the enclosing function returns.)
                let saved = std::mem::replace(&mut self.base, arg.start);
                let saved_ret = std::mem::replace(&mut self.ret_ty, Ty::Bool);
                let body = self.capture(|cg| {
                    cg.emit_into(&expr, &Dest::Return, Some(&Ty::Bool));
                });
                self.base = saved;
                self.ret_ty = saved_ret;
                let mut f = Frag::mapped(
                    start,
                    "(() => {
",
                );
                f.append(body)
                    .push(format!("{}}})()", "  ".repeat(self.out.indent)));
                (f, Ty::Bool)
            }
            "write" | "writeln" => {
                let fmt_result = Ty::Result(
                    Box::new(Ty::Unit),
                    Box::new(Ty::Extern("fmt::Error".into())),
                );
                let Some(target) = args.first() else {
                    return (Frag::text("undefined"), fmt_result);
                };
                let (tf, tt) = self.emit_macro_arg(target, None);
                let tpl = if args.len() > 1 {
                    self.format_args(mc, &args[1..])
                } else {
                    Frag::text("\"\"")
                };
                let nl = name == "writeln";
                // Into a `fmt::Write`: `w.__write_str(text)`, or through the cell of a
                // `W: fmt::Write` parameter (a `String` or `Formatter` has no such method).
                if self.types.fmt_writes.contains(&target.start) {
                    let mut text = tpl;
                    if nl {
                        text.push(" + \"\\n\"");
                    }
                    let mut f = Frag::mapped(start, "");
                    match crate::macros::parse_arg(target)
                        .0
                        .and_then(|e| self.writer_cell(&e))
                    {
                        Some(cell) => {
                            f.push(format!(
                                "κσ.$fmtWriteCall({cell}, {}, ",
                                js_string(&adt_gen::prop("__write_str"))
                            ))
                            .append(text)
                            .push(")");
                        }
                        None => {
                            f.append(tf).push(".__write_str(").append(text).push(")");
                        }
                    }
                    return (f, fmt_result);
                }
                // Into an `io::Write`: `w.__write_str(text)` (through `κσ.$dcall` for a
                // `Vec<u8>` or a type parameter, which may be one).
                if self.types.io_writes.contains(&target.start) {
                    let mut f = Frag::mapped(start, "");
                    // (Also a `&mut dyn Write` field or local: the value decides at run time.)
                    if crate::adt::ext_key_of(&tt).is_some() || matches!(tt, Ty::Param(_) | Ty::Dyn(_)) {
                        f.push("κσ.$dcall($extRegs, ")
                            .append(tf)
                            .push(format!(", {}, ", js_string(&adt_gen::prop("__write_str"))))
                            .append(tpl);
                    } else {
                        f.append(tf).push(".__write_str(").append(tpl);
                    }
                    if nl {
                        f.push(" + \"\\n\"");
                    }
                    f.push(")");
                    let io = Ty::Result(
                        Box::new(Ty::Unit),
                        Box::new(Ty::Adt("IoError".into(), vec![])),
                    );
                    return (f, io);
                }
                if tt == Ty::Str {
                    // `write!(s, ..)` appends to the string.
                    let mut f = Frag::mapped(start, "(");
                    f.append(tf).push(" += ").append(tpl);
                    if nl {
                        f.push(" + \"\\n\"");
                    }
                    f.push(", κσ.Ok(undefined))");
                    return (f, fmt_result);
                }
                let mut f = Frag::mapped(start, "κσ.$write(");
                f.append(tf).push(", ").append(tpl).push(format!(", {nl})"));
                (f, fmt_result)
            }
            _ => {
                self.unsupported(mc.syntax(), &format!("macro `{name}!` (`macro_rules!`)"));
                (Frag::text("undefined"), Ty::Unknown)
            }
        }
    }

    /// Lower `"fmt", args...` to a JS template literal (or plain string when there are no holes).
    fn format_args(&mut self, mc: &ast::MacroCall, args: &[MacroArg]) -> Frag {
        let fmt_arg = &args[0];
        let fmt_str = fmt_arg
            .single_token
            .as_ref()
            .and_then(|t| ast::String::cast(t.clone()))
            .and_then(|s| s.value().ok().map(|v| v.into_owned()));
        let Some(fmt_str) = fmt_str else {
            self.diag_range(
                fmt_arg.start,
                fmt_arg.start + fmt_arg.text.len(),
                "format argument must be a string literal".into(),
                "error",
            );
            return Frag::text("\"\"");
        };
        let pieces = match fmt::parse(&fmt_str) {
            Ok(p) => p,
            Err(msg) => {
                self.diag_range(
                    fmt_arg.start,
                    fmt_arg.start + fmt_arg.text.len(),
                    msg,
                    "error",
                );
                return Frag::text("\"\"");
            }
        };
        let positional: Vec<&MacroArg> = args[1..].iter().filter(|a| a.name.is_none()).collect();
        let named: HashMap<&str, &MacroArg> = args[1..]
            .iter()
            .filter_map(|a| a.name.as_deref().map(|n| (n, a)))
            .collect();

        // Lower every explicit argument exactly once, in order (Rust evaluates them all up front).
        let mut uses: HashMap<usize, usize> = HashMap::new();
        let mut next = 0;
        // Each placeholder's value, width and precision: an argument index, or a captured local.
        type Ref = (Option<usize>, Option<String>);
        let mut resolved: Vec<(Ref, Option<Ref>, Option<Ref>, Spec)> = Vec::new();
        let resolve = |arg: &ArgRef, next: &mut usize, uses: &mut HashMap<usize, usize>| -> Ref {
            let idx = match arg {
                ArgRef::Next => {
                    *next += 1;
                    Some(*next - 1)
                }
                ArgRef::Index(n) => Some(*n),
                ArgRef::Name(n) => named.get(n.as_str()).map(|a| {
                    positional.len()
                        + args[1..]
                            .iter()
                            .filter(|x| x.name.is_some())
                            .position(|x| std::ptr::eq(x, *a))
                            .unwrap()
                }),
            };
            let captured = match (arg, idx) {
                (ArgRef::Name(n), None) => Some(n.clone()),
                _ => None,
            };
            if let Some(i) = idx {
                *uses.entry(i).or_default() += 1;
            }
            (idx, captured)
        };
        for p in &pieces {
            if let Piece::Arg { arg, spec } = p {
                // `{:.*}` takes its precision before the value.
                let prec = spec
                    .precision_arg
                    .as_ref()
                    .map(|a| resolve(a, &mut next, &mut uses));
                let width = spec
                    .width_arg
                    .as_ref()
                    .map(|a| resolve(a, &mut next, &mut uses));
                let value = resolve(arg, &mut next, &mut uses);
                resolved.push((value, width, prec, spec.clone()));
            }
        }
        let all: Vec<&MacroArg> = positional
            .iter()
            .copied()
            .chain(args[1..].iter().filter(|a| a.name.is_some()))
            .collect();
        let mut lowered: Vec<(Frag, Ty)> = Vec::new();
        for (i, a) in all.iter().enumerate() {
            let (f, t) = self.emit_macro_arg(a, None);
            let t = t.default_lit();
            let trivial = matches!(a.text.trim(), t if t.starts_with('"') || t.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.'));
            if uses.get(&i).copied().unwrap_or(0) > 1 && !trivial {
                let tmp = self.temp();
                let mut decl = Frag::text(format!("const {tmp} = "));
                decl.append(f).push(";");
                self.line(decl);
                lowered.push((Frag::text(tmp), t));
            } else {
                lowered.push((f, t));
            }
        }
        for (i, a) in all.iter().enumerate() {
            if !uses.contains_key(&i) {
                self.diag_range(
                    a.start,
                    a.start + a.text.len(),
                    "argument never used in format string".into(),
                    "error",
                );
            }
        }

        let mut out = Frag::text("`");
        let mut holes = 0;
        let mut res_iter = resolved.into_iter();
        for p in &pieces {
            match p {
                Piece::Lit(s) => {
                    out.push(template_escape(s));
                }
                Piece::Arg { .. } => {
                    let ((idx, captured), width, prec, spec) = res_iter.next().unwrap();
                    // A width or precision taken from an argument.
                    let count = |cg: &mut Self, r: Option<Ref>| -> Option<Frag> {
                        match r? {
                            (Some(i), _) => lowered.get(i).map(|(f, _)| f.clone()),
                            (None, Some(name)) => cg
                                .lookup(&name)
                                .map(|l| Frag::text(l.js.clone()))
                                .or_else(|| cg.consts.get(&name).map(|js| Frag::text(js.clone()))),
                            (None, None) => None,
                        }
                    };
                    let width = count(self, width);
                    let prec = count(self, prec);
                    let (f, t) = match (idx, captured) {
                        (Some(i), _) if i < lowered.len() => lowered[i].clone(),
                        (Some(i), _) => {
                            self.error(
                                mc.syntax(),
                                format!("invalid reference to positional argument {i}"),
                            );
                            continue;
                        }
                        (None, Some(name)) => match self.lookup(&name) {
                            // `{refs:?}` of a Vec of slots: a copy with the slots read.
                            Some(l) => match self
                                .types
                                .slot_read_captures
                                .get(&(fmt_arg.start, name.clone()))
                            {
                                Some(shape) => (
                                    Frag::text(format!("κσ.$readSlots({}, {shape})", l.js)),
                                    l.ty.clone().default_lit(),
                                ),
                                None => (Frag::text(l.js.clone()), l.ty.clone().default_lit()),
                            },
                            // `{CYAN}`: a `const` item.
                            None if self.consts.contains_key(&name) => {
                                match self.captured_const(&name) {
                                    Some(r) => r,
                                    None => continue,
                                }
                            }
                            None => {
                                self.error(
                                    mc.syntax(),
                                    format!("cannot find value `{name}` in this scope"),
                                );
                                continue;
                            }
                        },
                        (None, None) => continue,
                    };
                    holes += 1;
                    out.push("${")
                        .append(format_value(f, &t, &spec, width, prec))
                        .push("}");
                }
            }
        }
        out.push("`");
        if holes == 0 {
            let lit: String = pieces
                .iter()
                .map(|p| {
                    if let Piece::Lit(s) = p {
                        s.as_str()
                    } else {
                        ""
                    }
                })
                .collect();
            return Frag::text(js_string(&lit));
        }
        out
    }
}

/// Runtime helper that renders `{:?}` for a type, or None when Debug equals Display.
/// A JS expression building `T::default()`.
fn default_value(ty: &Ty) -> Option<String> {
    Some(match ty {
        Ty::Coll(k, _) => coll_gen::new_coll(*k).into(),
        Ty::Set(_) => "new κσ.HashSet()".into(),
        Ty::Map(..) => "new κσ.HashMap()".into(),
        Ty::Tuple(es) => format!(
            "[{}]",
            es.iter()
                .map(default_value)
                .collect::<Option<Vec<_>>>()?
                .join(", ")
        ),
        Ty::Unit => "undefined".into(),
        Ty::Iter { .. } => "κσ.$iterEmpty()".into(),
        Ty::RefCell(inner) => format!("new κσ.RefCell({}, \"RefCell\")", default_value(inner)?),
        // (An NS interface whose fields are all optional: the type checker made sure.)
        Ty::Extern(n) if n == "JsValue" => "undefined".into(),
        Ty::Extern(_) => "{}".into(),
        Ty::Adt(n, args) => format!("{}.default()", class_view(escape_ident(n), args, &[])),
        other => other.default_js()?.into(),
    })
}

/// `[fmt0, fmt1, ...]`: one `{:?}` formatter per tuple element.
fn tuple_formatters(es: &[Ty]) -> String {
    tuple_formatters_in(es, &HashMap::new())
}

fn tuple_formatters_in(es: &[Ty], params: &HashMap<String, String>) -> String {
    let fs: Vec<String> = es
        .iter()
        .map(|e| debug_fn_in(e, params).unwrap_or_else(|| "String".into()))
        .collect();
    format!("[{}]", fs.join(", "))
}

thread_local! {
    /// The type parameters whose `{:?}` formatter the function being emitted takes (`$dbg$T`).
    static DBG_SCOPE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The NS types whose `{:?}` formatter is being written (a recursive one names itself).
    static DBG_EXTERN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The const generic parameters in scope, with their JS (`$N`, or `this.$N` in a method).
    static CONST_SCOPE: std::cell::RefCell<HashMap<String, String>> = std::cell::RefCell::new(HashMap::new());
}

/// Replace the const parameter scope (None: a copy of the current one stays); returns the
/// previous one.
fn set_const_scope(m: Option<HashMap<String, String>>) -> HashMap<String, String> {
    CONST_SCOPE.with(|s| match m {
        Some(m) => std::mem::replace(&mut *s.borrow_mut(), m),
        None => s.borrow().clone(),
    })
}

fn const_js_of(p: &str) -> Option<String> {
    CONST_SCOPE.with(|s| s.borrow().get(p).cloned())
}

/// A const argument as written in Rust (`3`, `true`, `'a'`, `SIZE`) in JS.
fn const_value_js(v: &str) -> String {
    if let Some(inner) = v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        let c = match inner.strip_prefix('\\') {
            Some("n") => '\n',
            Some("t") => '\t',
            Some("r") => '\r',
            Some("0") => '\0',
            Some(e) if e.starts_with("u{") && e.ends_with('}') => {
                u32::from_str_radix(&e[2..e.len() - 1], 16)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or('?')
            }
            Some(e) => e.chars().next().unwrap_or('?'),
            None => inner.chars().next().unwrap_or('?'),
        };
        return js_string(&c.to_string());
    }
    // (Past 2^64 only a 128-bit parameter's value, exact as a BigInt; a 128-bit parameter's body
    // converts a number argument with `BigInt(..)`.)
    match v.parse::<i128>() {
        Ok(n) if n.unsigned_abs() > u64::MAX as u128 => format!("{n}n"),
        Err(_) if v.parse::<u128>().is_ok() => format!("{v}n"),
        _ => v.to_string(),
    }
}

/// A struct's class with its const arguments (`Buf<4>` → `κσ.$cv(Buf, [4])`), for calling its
/// associated functions as a value (`T::default()`, `Default::default()`), which read them as
/// `this.$cargs`. Just the class when it has none.
pub(super) fn class_view(cls: String, args: &[Ty], stored: &[(usize, String)]) -> String {
    let vals: Vec<Option<String>> = args
        .iter()
        .enumerate()
        .map(|(i, a)| match a {
            _ if stored.iter().any(|s| s.0 == i) => {
                stored.iter().find(|s| s.0 == i).map(|s| s.1.clone())
            }
            Ty::ConstVal(v) => Some(const_value_js(v)),
            Ty::Param(p) => const_js_of(p),
            _ => None,
        })
        .collect();
    if vals.iter().all(Option::is_none) {
        return cls;
    }
    let vals: Vec<String> = vals
        .into_iter()
        .map(|v| v.unwrap_or_else(|| "undefined".into()))
        .collect();
    format!("κσ.$cv({cls}, [{}])", vals.join(", "))
}

/// Set the formatter scope for a function's body; returns the previous one.
fn set_dbg_scope(names: Vec<String>) -> Vec<String> {
    DBG_SCOPE.with(|s| std::mem::replace(&mut *s.borrow_mut(), names))
}

/// A JS function that renders `{:?}` for a type, or None when `String(x)` already matches Rust.
fn debug_fn(ty: &Ty) -> Option<String> {
    debug_fn_in(ty, &HashMap::new())
}

thread_local! {
    /// Inside a `{:x?}` / `{:X?}`: the spec each integer is printed with (`{:x}` / `{:X}`, with
    /// the `#`, `0` and width given).
    static HEX_DBG: std::cell::RefCell<Option<Spec>> = const { std::cell::RefCell::new(None) };
}

/// The bits `{:x}` / `{:b}` show for an integer type (its two's complement).
fn radix_bits(ty: &Ty) -> u32 {
    match ty {
        Ty::Int(k) => match k {
            IntKind::I8 | IntKind::U8 => 8,
            IntKind::I16 | IntKind::U16 => 16,
            IntKind::I32 | IntKind::U32 => 32,
            IntKind::I128 | IntKind::U128 => 128,
            _ => 64,
        },
        _ => 32,
    }
}

/// `debug_fn` inside a generic type's `$dbg`, where `params` names the formatter for each type
/// parameter.
fn debug_fn_in(ty: &Ty, params: &HashMap<String, String>) -> Option<String> {
    let debug_fn = |t: &Ty| debug_fn_in(t, params);
    if matches!(ty, Ty::Int(_) | Ty::IntLit)
        && let Some(spec) = HEX_DBG.with(|h| h.borrow().clone())
    {
        let f = format_value(Frag::text("x"), ty, &spec, None, None);
        return Some(format!("((x) => {})", f.plain()));
    }
    match ty {
        Ty::Str => Some("κσ.$dbgStr".into()),
        Ty::Char => Some("κσ.$dbgChar".into()),
        // (An `f32` prints its own shortest digits, not the double's.)
        Ty::Float { f32: true } => Some("κσ.$dbgF32".into()),
        Ty::Float { .. } => Some("κσ.$dbgF".into()),
        // (`()` is `undefined`, which `$dbg` prints as `()`: `Ok(())`.)
        // (`()` prints as such whatever stands for it: `Some(())` is a marker.)
        Ty::Unit => Some("(() => \"()\")".into()),
        // A bindings enum: `Str("n00dles")` / `Money(MoneyRequirement { .. })` by the variant the
        // value fits, its payload printed by the payload's own formatter. A named function, so
        // a recursive type (`NotRequirement.condition: PlayerRequirement`) refers back to it.
        Ty::Extern(n) if bindings::current().is_enum(n) => {
            let fn_name = format!("$dbg${}", n.replace("::", "$"));
            if DBG_EXTERN.with(|s| s.borrow().contains(n)) {
                return Some(fn_name);
            }
            DBG_EXTERN.with(|s| s.borrow_mut().push(n.clone()));
            let b = bindings::current();
            let et = b.enum_of(n)?;
            let variants =
                et.variants_js(|t| debug_fn(t).unwrap_or_else(|| "κσ.$dbg".into()));
            DBG_EXTERN.with(|s| s.borrow_mut().pop());
            Some(format!(
                "(function {fn_name}(v) {{ return κσ.$dbgJsEnum(v, {variants}, {}); }})",
                et.tag_js()
            ))
        }
        // An NS interface (`Server`, `MoneyRequirement`): its declared fields, as a derived
        // `Debug` would print a struct (`Server { hostname: "home", .. }`), the JS value's own
        // text otherwise.
        Ty::Extern(n)
            if bindings::current()
                .types
                .get(n)
                .is_some_and(|t| t.is_struct && !t.fields.is_empty()) =>
        {
            let fn_name = format!("$dbg${}", n.replace("::", "$"));
            if DBG_EXTERN.with(|s| s.borrow().contains(n)) {
                return Some(fn_name);
            }
            DBG_EXTERN.with(|s| s.borrow_mut().push(n.clone()));
            let b = bindings::current();
            let et = b.types.get(n)?;
            let mut fields: Vec<(&String, &bindings::Field)> = et.fields.iter().collect();
            fields.sort_by(|a, b| a.0.cmp(b.0));
            let items: Vec<String> = fields
                .iter()
                .map(|(name, f)| {
                    format!(
                        "[{}, {}, {}]",
                        js_string(&f.js),
                        js_string(name),
                        debug_fn(&f.ty).unwrap_or_else(|| "κσ.$dbg".into())
                    )
                })
                .collect();
            DBG_EXTERN.with(|s| s.borrow_mut().pop());
            Some(format!(
                "(function {fn_name}(o) {{ return κσ.$dbgIface({}, o, [{}], {}); }})",
                js_string(n),
                items.join(", "),
                et.debug_keys
            ))
        }
        Ty::Unknown | Ty::Extern(_) => Some("κσ.$dbg".into()),
        // (A `Box<dyn Error>` may hold a string, whose `{:?}` is quoted.)
        Ty::Dyn(_) => Some("κσ.$dbgAny".into()),
        Ty::Ordering => Some("κσ.$dbgOrdering".into()),
        // (A `fn(..)` pointer field: an address, as rustc prints it; see `{:p}`.)
        Ty::Closure { .. } => Some("((f) => κσ.$fmtPtr(f))".into()),
        // (Only a range value is `Debug`: `0..5`.)
        Ty::Iter { elem, .. } | Ty::Range(elem, _) => {
            let inner = debug_fn(elem).unwrap_or_else(|| "String".into());
            Some(format!("((r) => r.$dbg({inner}))"))
        }
        Ty::Coll(_, v) => debug_fn(v),
        // A generic type gets its arguments' formatters: `Tree<char>` prints chars as chars.
        Ty::Adt(_, args) if !args.is_empty() => {
            let fs: Vec<String> = args
                .iter()
                .map(|a| debug_fn(a).unwrap_or_else(|| "String".into()))
                .collect();
            Some(format!("((v) => v.$dbg({}))", fs.join(", ")))
        }
        Ty::Adt(..) => Some("κσ.$dbg".into()),
        // A type parameter: the formatter the caller passed (`$dbg$T`), else a guess from the
        // run-time value.
        Ty::Param(n) => Some(params.get(n).cloned().unwrap_or_else(|| {
            if DBG_SCOPE.with(|s| s.borrow().contains(n)) {
                format!("($dbg${} ?? κσ.$dbgAny)", n.replace("::", "$"))
            } else if let Some(cls) = const_js_of(n)
                && cls.starts_with("this.$")
            {
                // (An `impl<T>`'s `T` in a method with `self`: the class the value carries.)
                format!("κσ.$dbgOf({cls})")
            } else {
                "κσ.$dbgAny".into()
            }
        })),
        Ty::RefCell(t) => {
            let inner = debug_fn(t).unwrap_or_else(|| "String".into());
            Some(format!(
                "((c) => `${{c.kind}} {{ value: ${{({inner})(c.v)}} }}`)"
            ))
        }
        Ty::Result(t, e) => {
            let (tf, ef) = (
                debug_fn(t).unwrap_or_else(|| "String".into()),
                debug_fn(e).unwrap_or_else(|| "String".into()),
            );
            Some(format!("((r) => κσ.$dbgRes(r, {tf}, {ef}))"))
        }
        Ty::Vec(e) => {
            let inner = debug_fn(e).unwrap_or_else(|| "String".into());
            Some(format!("((v) => κσ.$dbgVec(v, {inner}))"))
        }
        Ty::Option(e) => {
            let inner = debug_fn(e).unwrap_or_else(|| "String".into());
            Some(format!("((v) => κσ.$dbgOpt(v, {inner}))"))
        }
        Ty::Set(e) => {
            let inner = debug_fn(e).unwrap_or_else(|| "String".into());
            Some(format!("((v) => κσ.$dbgSet(v, {inner}))"))
        }
        Ty::Map(k, v) => {
            let (kf, vf) = (
                debug_fn(k).unwrap_or_else(|| "String".into()),
                debug_fn(v).unwrap_or_else(|| "String".into()),
            );
            Some(format!("((m) => κσ.$dbgMap(m, {kf}, {vf}))"))
        }
        Ty::Tuple(es) => Some(format!(
            "((v) => κσ.$dbgTuple(v, {}))",
            tuple_formatters_in(es, params)
        )),
        _ => None,
    }
}

/// `name(f, extra..)`.
fn call_with_spec(name: &str, f: Frag, extra: Vec<Frag>) -> Frag {
    let mut g = Frag::text(format!("{name}("));
    g.append(f);
    for e in extra {
        g.push(", ").append(e);
    }
    g.push(")");
    g
}

fn format_value(
    f: Frag,
    ty: &Ty,
    spec: &Spec,
    width_arg: Option<Frag>,
    prec_arg: Option<Frag>,
) -> Frag {
    let ty = &ty.view();
    // `{:x?}` / `{:#X?}`: an integer as `{:x}` / `{:#X}`; elsewhere each integer inside the
    // `Debug` text that way (`[0a, ff]`), with the width per integer.
    if spec.debug && matches!(spec.kind, Some('x' | 'X')) {
        let hex = Spec {
            debug: false,
            ..spec.clone()
        };
        if matches!(ty, Ty::Int(_) | Ty::IntLit) {
            return format_value(f, ty, &hex, width_arg, None);
        }
        let outer = Spec {
            kind: None,
            width: None,
            zero: false,
            fill: None,
            align: None,
            ..spec.clone()
        };
        let prev = HEX_DBG.with(|h| h.replace(Some(hex)));
        let s = format_value(f, ty, &outer, None, None);
        HEX_DBG.with(|h| *h.borrow_mut() = prev);
        return s;
    }
    // `{:p}`: a made-up address, the same for the same object; padded like a string.
    if spec.kind == Some('p') {
        let mut g = Frag::text("κσ.$fmtPtr(");
        g.append(f).push(")");
        let plain = Spec {
            kind: None,
            ..spec.clone()
        };
        return format_value(g, &Ty::Str, &plain, width_arg, None);
    }
    let width = width_arg.or_else(|| spec.width.map(|w| Frag::text(w.to_string())));
    let precision = prec_arg.or_else(|| spec.precision.map(|p| Frag::text(p.to_string())));
    let call = |name: &str, f: Frag, extra: &str| {
        let mut g = Frag::text(format!("{name}("));
        g.append(f).push(extra).push(")");
        g
    };
    let numeric = ty.is_numeric() || *ty == Ty::Unknown;
    // A struct or enum with a spec (`{:>8}`, `{:.2?}`): its own `fmt` gets the spec through the
    // Formatter (`f.width()`, `f.pad(..)`); a derived `{:?}` is padded as before.
    if matches!(ty, Ty::Adt(..))
        && spec.kind.is_none()
        && (width.is_some() || precision.is_some() || spec.plus || spec.alt || spec.zero)
    {
        let opt = |v: Option<Frag>| v.unwrap_or_else(|| Frag::text("null"));
        let mut o = Frag::text("{ w: ");
        o.append(opt(width)).push(", p: ").append(opt(precision));
        let align = spec
            .align
            .map(|c| js_string(&c.to_string()))
            .unwrap_or_else(|| "null".into());
        let fill = js_string(&spec.fill.unwrap_or(' ').to_string());
        o.push(format!(
            ", align: {align}, fill: {fill}, plus: {}, alt: {}, zero: {} }}",
            spec.plus, spec.alt, spec.zero
        ));
        if !spec.debug {
            return call_with_spec("κσ.$displaySpec", f, vec![o]);
        }
        let d = debug_fn(ty).unwrap_or_else(|| "κσ.$dbg".into());
        return call_with_spec("κσ.$dbgSpec", f, vec![o, Frag::text(d)]);
    }
    // A user `Display` impl formats itself; like `write!`-based impls in Rust, it ignores width.
    if !spec.debug && matches!(ty, Ty::Adt(..)) {
        return call("κσ.$display", f, "");
    }
    // A generic value or trait object: decided at run time.
    let f = if !spec.debug && matches!(ty, Ty::Param(_) | Ty::Dyn(_)) {
        call("κσ.$disp", f, "")
    } else {
        f
    };
    let call_with = |name: &str, f: Frag, extra: Vec<Frag>| {
        let mut g = Frag::text(format!("{name}("));
        g.append(f);
        for e in extra {
            g.push(", ").append(e);
        }
        g.push(")");
        g
    };
    // `{:x}`, `{:#b}`, `{:o}`: the digits of the type's two's complement; `{:e}`: scientific.
    if let Some(k) = spec.kind {
        let mut s = match k {
            'e' | 'E' => call_with(
                "κσ.$exp",
                f,
                vec![
                    precision.unwrap_or_else(|| Frag::text("null")),
                    Frag::text((k == 'E').to_string()),
                ],
            ),
            _ => {
                let radix = match k {
                    'x' | 'X' => 16,
                    'o' => 8,
                    _ => 2,
                };
                let bits = radix_bits(ty);
                call_with(
                    "κσ.$radix",
                    f,
                    vec![Frag::text(format!(
                        "{radix}, {}, {}, {bits}",
                        k == 'X',
                        spec.alt
                    ))],
                )
            }
        };
        if spec.plus {
            s = call("κσ.$plus", s, "");
        }
        if let Some(w) = width {
            let align = spec
                .align
                .map(|c| js_string(&c.to_string()))
                .unwrap_or_else(|| "null".into());
            let fill = js_string(&spec.fill.unwrap_or(' ').to_string());
            s = call_with(
                "κσ.$pad",
                s,
                vec![
                    w,
                    Frag::text(format!("{align}, {fill}, {}, true", spec.zero)),
                ],
            );
        }
        return s;
    }
    let mut s = if spec.debug {
        match ty {
            Ty::Tuple(es) => call("κσ.$dbgTuple", f, &format!(", {}", tuple_formatters(es))),
            Ty::Map(k, v) => call(
                "κσ.$dbgMap",
                f,
                &format!(
                    ", {}, {}",
                    debug_fn(k).unwrap_or_else(|| "String".into()),
                    debug_fn(v).unwrap_or_else(|| "String".into())
                ),
            ),
            Ty::Set(e) => call(
                "κσ.$dbgSet",
                f,
                &format!(", {}", debug_fn(e).unwrap_or_else(|| "String".into())),
            ),
            Ty::Option(e) => call(
                "κσ.$dbgOpt",
                f,
                &format!(", {}", debug_fn(e).unwrap_or_else(|| "String".into())),
            ),
            Ty::Vec(e) => call(
                "κσ.$dbgVec",
                f,
                &format!(", {}", debug_fn(e).unwrap_or_else(|| "String".into())),
            ),
            _ => match debug_fn(ty) {
                Some(d) => call(&d, f, ""),
                None => f,
            },
        }
    } else if let Some(p) = precision {
        match ty {
            Ty::Str => call_with("κσ.$truncChars", f, vec![p]),
            Ty::Int(_) | Ty::IntLit => f,
            _ => call_with("κσ.$fixed", f, vec![p]),
        }
    } else {
        match ty {
            Ty::Float { f32: true } => call("κσ.$fmtF32", f, ""),
            Ty::Float { .. } => call("κσ.$fmtF", f, ""),
            Ty::Unknown => call("κσ.$disp", f, ""),
            _ => f,
        }
    };
    // `{:#?}`: pretty, one field or element per line.
    if spec.debug && spec.alt {
        s = call("κσ.$pretty", s, "");
    }
    if spec.plus && numeric {
        s = call("κσ.$plus", s, "");
    }
    if let Some(w) = width {
        let align = spec
            .align
            .map(|c| js_string(&c.to_string()))
            .unwrap_or_else(|| "null".into());
        let fill = js_string(&spec.fill.unwrap_or(' ').to_string());
        s = call_with(
            "κσ.$pad",
            s,
            vec![
                w,
                Frag::text(format!("{align}, {fill}, {}, {numeric}", spec.zero)),
            ],
        );
    }
    s
}

fn args_len(m: &ast::MethodCallExpr) -> usize {
    m.arg_list().map(|l| l.args().count()).unwrap_or(0)
}

/// A top-level `|` pattern (through parentheses).
fn strip_or(p: &ast::Pat) -> Option<ast::OrPat> {
    match p {
        ast::Pat::OrPat(op) => Some(op.clone()),
        ast::Pat::ParenPat(pp) => strip_or(&pp.pat()?),
        _ => None,
    }
}

/// The names a macro call's arguments may use under `node`: identifiers that aren't a field or
/// method (after `.`) or a path segment (before `::`), and `{name}` / `{name:?}` in strings.
fn macro_arg_names(node: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    for tt in node.descendants().filter_map(ast::TokenTree::cast) {
        let toks: Vec<_> = tt
            .syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        for (i, t) in toks.iter().enumerate() {
            match t.kind() {
                SyntaxKind::IDENT => {
                    let after_dot = i > 0 && toks[i - 1].kind() == SyntaxKind::DOT;
                    let path = toks
                        .get(i + 1)
                        .is_some_and(|n| n.kind() == SyntaxKind::COLON2)
                        || (i > 0 && toks[i - 1].kind() == SyntaxKind::COLON2);
                    if !after_dot && !path {
                        out.push(t.text().to_string());
                    }
                }
                SyntaxKind::STRING => {
                    let s = t.text();
                    let mut rest = s;
                    while let Some(open) = rest.find('{') {
                        rest = &rest[open + 1..];
                        if rest.starts_with('{') {
                            rest = &rest[1..];
                            continue;
                        }
                        let name: String = rest
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect();
                        if name.chars().next().is_some_and(|c| !c.is_ascii_digit()) {
                            out.push(name);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Whether a `Some` of this type needs the marker representation (`κσ.$some` / `κσ.$unsome`):
/// an `Option` or `()` payload, or a type parameter, which may be one (`impl<T> Tr for
/// Option<T>` reached with an `Option<Option<i32>>`; the helpers leave other values alone).
fn nullable_payload(t: &Ty) -> bool {
    matches!(t, Ty::Option(_) | Ty::Unit | Ty::Param(_))
}

/// Whether parameter type `t` of `f` is an `R: Read` / `R: BufRead` (`f`'s or its impl's),
/// `impl Read` or `Box<dyn Read>` taken by value: what `&[u8]` may be passed as.
fn byte_reader(f: &ast::Fn, t: &ast::Type) -> bool {
    use ast::HasTypeBounds;
    use ra_ap_syntax::ast::HasGenericArgs;
    let reads = |l: Option<ast::TypeBoundList>| {
        l.into_iter().flat_map(|l| l.bounds()).any(|b| {
            matches!(b.ty(), Some(ast::Type::PathType(p)) if p.path().and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_some_and(|n| n.text() == "IoRead" || n.text() == "IoBufRead"))
        })
    };
    let name = match t {
        ast::Type::ImplTraitType(it) => return reads(it.type_bound_list()),
        ast::Type::PathType(p) => match p.path().and_then(|p| p.segment()) {
            // `Box<dyn Read>`
            Some(seg) if seg.name_ref().is_some_and(|n| n.text() == "Box") => {
                return seg
                    .generic_arg_list()
                    .and_then(|l| l.generic_args().next())
                    .is_some_and(|g| {
                        matches!(g, ast::GenericArg::TypeArg(ta)
                            if matches!(ta.ty(), Some(ast::Type::DynTraitType(d)) if reads(d.type_bound_list())))
                    });
            }
            _ => match p.path().and_then(|p| p.as_single_name_ref()) {
                Some(n) => n.text().to_string(),
                None => return false,
            },
        },
        _ => return false,
    };
    let imp = f.syntax().ancestors().find_map(ast::Impl::cast);
    let scopes = [
        (f.generic_param_list(), f.where_clause()),
        (
            imp.as_ref().and_then(|i| i.generic_param_list()),
            imp.as_ref().and_then(|i| i.where_clause()),
        ),
    ];
    scopes.into_iter().any(|(params, wc)| {
        params.into_iter().flat_map(|l| l.generic_params()).any(|g| {
            matches!(g, ast::GenericParam::TypeParam(tp)
                if tp.name().is_some_and(|n| n.text() == name.as_str()) && reads(tp.type_bound_list()))
        }) || wc.into_iter().flat_map(|w| w.predicates()).any(|pred| {
            pred.ty().is_some_and(|t| t.syntax().text() == name.as_str())
                && reads(pred.type_bound_list())
        })
    })
}
