//! Spike code generator: walks the ra_ap_syntax AST directly and emits JS with source mappings.
//! Phase 1 inserts a HIR + type checker between parsing and this stage.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, AstToken, SourceFile, SyntaxKind, SyntaxNode,
    ast::{
        self, ArithOp, BinaryOp, CmpOp, ElseBranch, HasArgList, HasGenericParams, HasLoopBody,
        HasName, LiteralKind, LogicOp, RangeItem, RangeOp, UnaryOp,
    },
};
use serde::Serialize;

use crate::bindings::{self, bare};

#[path = "codegen_adt.rs"]
mod adt_gen;
#[path = "codegen_ext.rs"]
mod ext;
#[path = "codegen_res.rs"]
mod res_gen;
#[path = "codegen_std.rs"]
mod std_gen;
use crate::fmt::{self, ArgRef, Piece, Spec};
use crate::lineindex::{LineIndex, utf16_len};
use crate::macros::{self, EDITION, MacroArg};
use crate::sourcemap::{self, Mapping};
use crate::ty::{IntKind, Ty, is_float_method, std_const, std_tail};
use crate::typeck::{self, Types, is_none_binding, path_is};

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub message: String,
    pub severity: &'static str,
    /// Byte offsets into the source.
    pub start: usize,
    pub end: usize,
    /// 1-based line, 1-based UTF-16 column (Monaco's convention).
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CompileOutput {
    pub code: Option<String>,
    pub source_map: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
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
}

/// Which part of each `[key, value]` slot a `for` loop over a map binds.
#[derive(Clone, Copy)]
enum MapPart {
    Entries,
    Keys,
    Values,
}

/// A lowered pattern: conditions that must all hold, and `(name, mutable, js)` bindings.
#[derive(Default)]
struct PatCode {
    conds: Vec<Frag>,
    binds: Vec<(ast::Name, bool, String)>,
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
    if list.statements().next().is_some() || block.label().is_some() || block.modifier().is_some() {
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

/// Compile `src`; `resolver` reads the files of its `mod` declarations (paths relative to the
/// project root, like `lib/util.rs`).
pub fn compile_with(src: &str, filename: &str, resolver: typeck::Resolver) -> CompileOutput {
    let parse = SourceFile::parse(src, EDITION);
    let mut cg = Codegen {
        src,
        filename,
        idx: LineIndex::new(src),
        out: Out {
            code: String::new(),
            line: 0,
            col: 0,
            indent: 0,
            mappings: Vec::new(),
            marks: Vec::new(),
        },
        diags: Vec::new(),
        fns: HashMap::new(),
        scopes: Vec::new(),
        used_names: HashMap::new(),
        tmp: 0,
        base: 0,
        ret_ty: Ty::Unit,
        in_async: false,
        types: Types::default(),
        self_adt: None,
        consts: HashMap::new(),
        local_adts: Default::default(),
        loop_dests: Vec::new(),
    };
    for e in parse.errors() {
        let r = e.range();
        cg.diag_range(
            usize::from(r.start()),
            usize::from(r.end()),
            e.to_string(),
            "error",
        );
    }
    if cg.diags.is_empty() {
        let file = parse.tree();
        let (types, type_diags) = typeck::check(&file, src, filename, resolver);
        cg.types = types;
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
                cg.diag_range(d.start, d.end, d.message, d.severity);
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
            Some(sourcemap::to_json(filename, src, &mut mappings)),
        )
    };
    CompileOutput {
        code,
        source_map,
        diagnostics: cg.diags,
    }
}

struct Codegen<'a> {
    src: &'a str,
    filename: &'a str,
    idx: LineIndex<'a>,
    out: Out,
    diags: Vec<Diagnostic>,
    fns: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, Local>>,
    used_names: HashMap<String, u32>,
    tmp: u32,
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
    /// Enclosing loops, innermost last: (label, where a `loop`'s `break value` goes; None for
    /// `while`/`for`).
    loop_dests: Vec<(Option<String>, Option<Dest>, bool)>,
}

impl<'a> Codegen<'a> {
    // ---------------------------------------------------------------- diagnostics & positions

    fn off(&self, n: &SyntaxNode) -> usize {
        self.base + usize::from(n.text_range().start())
    }

    fn diag_range(&mut self, start: usize, end: usize, message: String, severity: &'static str) {
        let (l0, c0) = self.idx.line_col(start);
        let (l1, c1) = self.idx.line_col(end);
        self.diags.push(Diagnostic {
            message,
            severity,
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

    fn unsupported(&mut self, n: &SyntaxNode, what: &str, phase: u8) {
        self.error(
            n,
            format!("{what} is not supported yet (planned for phase {phase})"),
        );
    }

    /// `file:line:col` for panic messages, 1-based like rustc.
    fn location(&self, n: &SyntaxNode) -> String {
        self.location_at(self.off(n))
    }

    fn location_at(&self, offset: usize) -> String {
        let (l, c) = self.idx.line_col(offset);
        format!("{}:{}:{}", self.filename, l + 1, c + 1)
    }

    fn line(&mut self, frag: Frag) {
        self.out.line(&frag, &self.idx);
    }

    // ---------------------------------------------------------------- scopes & names

    fn fresh(&mut self, rust_name: &str) -> String {
        let base = escape_ident(rust_name);
        let n = self.used_names.entry(base.clone()).or_insert(0);
        let name = if *n == 0 {
            base.clone()
        } else {
            format!("{base}${n}")
        };
        *n += 1;
        name
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
        self.scopes
            .last_mut()
            .unwrap()
            .insert(rust_name.to_string(), local);
    }

    fn lookup(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// The type checker's resolved type for a node (an expression or a binding name).
    fn ty_of(&self, n: &SyntaxNode) -> Option<Ty> {
        let r = n.text_range();
        self.types
            .get(
                self.base + usize::from(r.start()),
                self.base + usize::from(r.end()),
            )
            .cloned()
    }

    // ---------------------------------------------------------------- types

    /// Lower a type annotation. The type checker reports unsupported types; this only needs the
    /// shape (for casts and function signatures).
    fn lower_type(&mut self, t: &ast::Type) -> Ty {
        match t {
            ast::Type::RefType(r) => r.ty().map(|t| self.lower_type(&t)).unwrap_or(Ty::Unknown),
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
                        Ty::Adt(n.text().to_string())
                    }
                    Some(n) if n.text() == "Self" && self.self_adt.is_some() => {
                        Ty::Adt(self.self_adt.clone().unwrap())
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
        // Pass 1: signatures, so calls can be lowered before the callee is emitted.
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
                let sig = FnSig {
                    js,
                    params,
                    ret,
                    is_async: f.async_token().is_some(),
                };
                self.fns.insert(name.text().to_string(), sig);
            }
        }
        // Functions and constants from `mod` files live on their module's namespace.
        for module in self.types.modules.clone() {
            self.used_names.insert(escape_ident(&module.name), 1);
        }
        for imp in self.types.imported.clone() {
            let js = format!("{}.{}", escape_ident(&imp.module), escape_ident(&imp.js));
            match imp.sig {
                Some(sig) => {
                    let sig = FnSig {
                        js,
                        params: sig.params,
                        ret: sig.ret,
                        is_async: sig.is_async,
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
        }
        // `impl` blocks are emitted inside their type's class.
        let mut impls: HashMap<String, Vec<ast::Impl>> = HashMap::new();
        for item in crate::adt::all_items(file) {
            if let ast::Item::Impl(imp) = &item
                && let (_, Some(t)) = crate::adt::impl_parts(imp)
            {
                impls.entry(t).or_default().push(imp.clone());
            }
        }
        // Pass 2: code.
        let globals = self.used_names.clone();
        for item in crate::adt::all_items(file) {
            match &item {
                ast::Item::Fn(f) => {
                    self.used_names = globals.clone();
                    self.tmp = 0;
                    self.emit_fn(f);
                }
                ast::Item::Struct(s) => {
                    if let Some(n) = s.name() {
                        self.used_names = globals.clone();
                        let imps = impls
                            .get(n.text().to_string().as_str())
                            .cloned()
                            .unwrap_or_default();
                        self.emit_adt(s.syntax(), &n, &imps);
                    }
                }
                ast::Item::Enum(e) => {
                    if let Some(n) = e.name() {
                        self.used_names = globals.clone();
                        let imps = impls
                            .get(n.text().to_string().as_str())
                            .cloned()
                            .unwrap_or_default();
                        self.emit_adt(e.syntax(), &n, &imps);
                    }
                }
                // Checked by the type checker; emitted with their type.
                ast::Item::Impl(_) => {}
                ast::Item::Trait(t) => {
                    self.used_names = globals.clone();
                    self.emit_trait_defaults(t);
                }
                ast::Item::Const(_) | ast::Item::Static(_) => {}
                // `use` only brings names into scope; the type checker resolves them.
                ast::Item::Use(_) => {}
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
                        self.line(Frag::default());
                    }
                }
                other => {
                    let kind = match other {
                        ast::Item::MacroRules(_) => "`macro_rules!`",
                        _ => "this item",
                    };
                    let phase = if matches!(other, ast::Item::MacroRules(_)) {
                        4
                    } else {
                        1
                    };
                    self.unsupported(other.syntax(), kind, phase);
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
            let mut line = Frag::mapped(self.item_start(&node), "export const ");
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
        if let Some(gp) = f.generic_param_list() {
            // Type parameters are erased; const generics aren't supported.
            if gp
                .generic_params()
                .any(|p| matches!(p, ast::GenericParam::ConstParam(_)))
            {
                self.unsupported(gp.syntax(), "const generic parameters", 4);
            }
        }
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
        if let Some(pl) = f.param_list() {
            if let Some(sp) = pl.self_param() {
                self.unsupported(sp.syntax(), "methods (`self` parameters)", 1);
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
                        self.declare(n.text(), Some(n.syntax()), Local { js: access, ty });
                        params.push(Frag::mapped(self.off(n.syntax()), js));
                    }
                    Some(ast::Pat::WildcardPat(_)) => params.push(Frag::text(self.temp())),
                    Some(other) => self.unsupported(other.syntax(), "destructuring parameters", 1),
                    None => {}
                }
            }
        }

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
        if let Some(body) = f.body() {
            self.emit_block_body(&body, &Dest::Return, None);
        }
        self.out.indent -= 1;
        self.close_line("}", f.body().as_ref());
        self.line(Frag::default());
        self.scopes.pop();
    }

    // ---------------------------------------------------------------- statements

    fn emit_block_body(
        &mut self,
        block: &ast::BlockExpr,
        dest: &Dest,
        expected: Option<&Ty>,
    ) -> Ty {
        if let Some(m) = block.modifier()
            && !matches!(m, ast::BlockModifier::Label(_))
        {
            let what = match m {
                ast::BlockModifier::Async(_) => "`async` blocks",
                ast::BlockModifier::Unsafe(_) => "`unsafe` blocks",
                _ => "this block kind",
            };
            let phase = if matches!(m, ast::BlockModifier::Async(_)) {
                1
            } else {
                4
            };
            self.unsupported(block.syntax(), what, phase);
            return Ty::Unknown;
        }
        let Some(list) = block.stmt_list() else {
            return Ty::Unit;
        };
        self.scopes.push(HashMap::new());
        let mut diverged = false;
        for stmt in list.statements() {
            diverged |= self.emit_stmt(&stmt);
        }
        let ty = match list.tail_expr() {
            Some(tail) => self.emit_into(&tail, dest, expected),
            None if diverged => Ty::Never,
            None => Ty::Unit,
        };
        self.scopes.pop();
        ty
    }

    /// Returns true if the statement always diverges (return/break/continue/panic).
    fn emit_stmt(&mut self, stmt: &ast::Stmt) -> bool {
        match stmt {
            ast::Stmt::LetStmt(l) => {
                self.emit_let(l);
                false
            }
            ast::Stmt::ExprStmt(es) => match es.expr() {
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
                self.unsupported(other.syntax(), "destructuring `let` patterns", 1);
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
            // `let _ = expr;` evaluates and drops.
            self.emit_into(&init, &Dest::Discard, declared.as_ref());
            return;
        };
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
            let mut f = Frag::mapped(start, "const ");
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
            ast::Expr::BlockExpr(b) => {
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
            self.unsupported(range.syntax(), "open-ended ranges", 1);
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
                self.unsupported(other.syntax(), "destructuring `for` patterns", 1);
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

    /// `for x in v` / `&v` / `v.iter()` / `v.into_iter()` (Vec or HashSet) become `for...of`.
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
        let enumerate = is_call(iterable, &["enumerate"]);
        let inner = if enumerate {
            receiver(iterable)
        } else {
            Some(iterable.clone())
        };
        let Some(inner) = inner else { return };
        let (source, mut by_mut_ref) = match &inner {
            ast::Expr::RefExpr(r) => (r.expr(), r.mut_token().is_some()),
            e if is_call(e, &["iter", "iter_mut", "into_iter"]) => {
                let mutable = matches!(e, ast::Expr::MethodCallExpr(m) if m.name_ref().is_some_and(|n| n.text() == "iter_mut"));
                (receiver(e), mutable)
            }
            other => (Some(other.clone()), false),
        };
        let Some(source) = source else { return };
        // Iterators (and enumerating anything but a Vec) loop over the whole expression.
        let mut enumerate = enumerate;
        let source_ty = self.ty_of(source.syntax()).unwrap_or(Ty::Unknown);
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
        if !matches!(
            st,
            Ty::Vec(_) | Ty::Set(_) | Ty::Iter { mutable: false, .. } | Ty::Unknown
        ) {
            self.unsupported(iterable.syntax(), "`for` over this iterator", 1);
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
            let element = format!("{place}[{i}]");
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
        (frag.at(start), ty)
    }

    fn operand(&mut self, e: &ast::Expr, expected: Option<&Ty>) -> (Frag, Ty) {
        let (f, t) = self.emit_expr(e, expected);
        (if needs_parens(e) { f.wrap("(", ")") } else { f }, t)
    }

    fn receiver(&mut self, e: &ast::Expr) -> (Frag, Ty) {
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
            ast::Expr::PathExpr(p) => self.emit_path(p),
            ast::Expr::ParenExpr(p) => match p.expr() {
                Some(inner) => {
                    let (f, t) = self.emit_expr(&inner, expected);
                    (f.wrap("(", ")"), t)
                }
                None => (Frag::text("undefined"), Ty::Unit),
            },
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
            ast::Expr::CallExpr(c) if crate::typeck::is_cell_new(c) => {
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
                if crate::typeck::is_box_new(c) || crate::typeck::is_rc_call(c) =>
            {
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
                let js = default_value(&ty).unwrap_or_else(|| "undefined".into());
                (Frag::mapped(self.off(c.syntax()), js), ty)
            }
            ast::Expr::CallExpr(c) => self.emit_call(c),
            ast::Expr::MethodCallExpr(m) => self.emit_method(m),
            ast::Expr::FieldExpr(fe) => self.emit_field(fe),
            ast::Expr::AwaitExpr(a) => {
                let Some(inner) = a.expr() else {
                    return (Frag::text("undefined"), Ty::Unknown);
                };
                let (f, t) = self.receiver(&inner);
                let ty = match t {
                    Ty::Future(t) => *t,
                    _ => Ty::Unknown,
                };
                (Frag::text("await ").append(f).clone(), ty)
            }
            ast::Expr::MacroExpr(m) => match m.macro_call() {
                Some(mc) => self.emit_macro(&mc),
                None => (Frag::text("undefined"), Ty::Unknown),
            },
            ast::Expr::IfExpr(i) if if_is_ternary(i) => self.emit_ternary(i, expected),
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
            ast::Expr::ClosureExpr(c) => (self.emit_closure(c), Ty::Unknown),
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
                    let early = format!("if ({tmp}.$tag === 1) return {tmp};");
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
                let (what, phase) = match other {
                    ast::Expr::MatchExpr(_) => ("`match`", 1),
                    ast::Expr::ClosureExpr(_) => ("closures", 1),
                    ast::Expr::TupleExpr(_) => ("tuples", 1),
                    ast::Expr::RecordExpr(_) => ("struct literals", 1),
                    _ => ("this expression", 1),
                };
                self.unsupported(other.syntax(), what, phase);
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
                if value > (1u128 << 53) {
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
            _ => {
                self.unsupported(l.syntax(), "byte strings and C strings", 4);
                (Frag::text("undefined"), Ty::Unknown)
            }
        }
    }

    fn emit_path(&mut self, p: &ast::PathExpr) -> (Frag, Ty) {
        let Some(path) = p.path() else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        if let Some(name) = path.as_single_name_ref() {
            if let Some(local) = self.lookup(name.text()) {
                // `&mut n` / a `&mut` parameter passed on: the cell itself.
                if self.types.cell_args.contains(&self.range_of(p.syntax()))
                    && let Some(cell) = local.js.strip_suffix("[0]")
                {
                    return (Frag::text(cell.to_string()), local.ty.clone());
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
            if let Some(r) = self.emit_adt_path(&[name.text().to_string()]) {
                return r;
            }
            if name.text() == "None" {
                return (Frag::text("null"), Ty::Option(Box::new(Ty::Unknown)));
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
        if let Some(r) = self.emit_adt_path(&segs) {
            return r;
        }
        if matches!(
            segs.iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            [.., "String", "from"] | [.., "ToString", "to_string"]
        ) {
            return (Frag::text("String"), Ty::Unknown);
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
        self.error(
            p.syntax(),
            format!(
                "cannot find value `{}` (paths are resolved in phase 2)",
                path.syntax().text()
            ),
        );
        (Frag::text("undefined"), Ty::Unknown)
    }

    fn emit_prefix(&mut self, p: &ast::PrefixExpr, expected: Option<&Ty>) -> (Frag, Ty) {
        let Some(inner) = p.expr() else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        match p.op_kind() {
            Some(UnaryOp::Deref) => self.emit_expr(&inner, expected),
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
                        let not = Frag::text("~").append(f).clone();
                        match k {
                            IntKind::U32 | IntKind::U8 | IntKind::U16 => {
                                let (pre, post) = k.wrap_parts().unwrap();
                                (not.wrap(pre, post), t)
                            }
                            k if k.is_narrow() => (not, t),
                            _ => {
                                self.unsupported(
                                    p.syntax(),
                                    "bitwise operators on 64-bit integers",
                                    4,
                                );
                                (not, t)
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
        match op {
            BinaryOp::Assignment { op } => {
                let assignable = match strip_refs(&lhs) {
                    ast::Expr::PathExpr(_) | ast::Expr::FieldExpr(_) | ast::Expr::PrefixExpr(_) => {
                        true
                    }
                    // `v[i] = x` lowers to `v[κσ.$ix(v, i)] = x`, which evaluates `v` twice.
                    ast::Expr::IndexExpr(ix) => ix.base().is_some_and(|b| is_pure(&b)),
                    // `(a, b) = (b, a)` is JS destructuring: `[a, b] = [b, a]`.
                    ast::Expr::TupleExpr(_) => op.is_none(),
                    _ => false,
                };
                if !assignable {
                    self.unsupported(lhs.syntax(), "assignment to this place", 1);
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
                        None => self.emit_expr(&lhs, None),
                    },
                };
                let (rf, rt) = self.operand(&rhs, Some(&lt));
                let ty = Ty::unify(&lt, &rt).default_lit();
                let value = match op {
                    None => self.by_value(&rhs, rf, &rt),
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
                // `*r = v` / `*self = v` with `r` a `&mut` to a struct or enum: the JS variable
                // holds the object itself, so replace its contents in place.
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
                if op.is_none() && through_ref && matches!(lt, Ty::Adt(_)) {
                    let mut f = Frag::text("κσ.$replace(");
                    f.append(lf).push(", ").append(value).push(")");
                    return (f, Ty::Unit);
                }
                let mut f = lf;
                f.push(" = ").append(value);
                (f, Ty::Unit)
            }
            BinaryOp::LogicOp(op) => {
                if needs_hoist(rhs.syntax()) {
                    self.unsupported(
                        rhs.syntax(),
                        "block expressions on the right of `&&`/`||`",
                        1,
                    );
                }
                let (lf, _) = self.operand(&lhs, Some(&Ty::Bool));
                let (rf, _) = self.operand(&rhs, Some(&Ty::Bool));
                let mut f = lf;
                f.push(if op == LogicOp::And { " && " } else { " || " })
                    .append(rf);
                (f, Ty::Bool)
            }
            BinaryOp::CmpOp(op) => {
                let (lf, lt) = self.operand(&lhs, None);
                let (rf, _) = self.operand(&rhs, Some(&lt));
                // Tuples and Vecs order lexicographically, like Rust's derived `Ord`.
                if let (Ty::Vec(_) | Ty::Tuple(_) | Ty::Adt(_) | Ty::Param(_), CmpOp::Ord { .. }) =
                    (&lt, op)
                {
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
                    | Ty::Adt(_)
                    | Ty::Result(..)
                    | Ty::Param(_),
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
                    ArithOp::Div => {
                        let f = call("κσ.$idiv", l, r);
                        if narrow { wrap(f) } else { f }
                    }
                    ArithOp::Rem => call("κσ.$irem", l, r),
                    _ if !narrow => {
                        self.unsupported(b.syntax(), "bitwise operators on 64-bit integers", 4);
                        return plain(l, r);
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
        let to = self.lower_type(&target);
        let (f, from) = self.operand(&inner, None);
        let from = from.default_lit();
        let out = match (&from, &to) {
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
            (Ty::Adt(n), Ty::Int(_)) => self.enum_discriminant(f, n),
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
        let place = matches!(
            strip_refs(e),
            ast::Expr::PathExpr(_)
                | ast::Expr::FieldExpr(_)
                | ast::Expr::IndexExpr(_)
                | ast::Expr::PrefixExpr(_)
        );
        if place && self.copied_on_read(ty) {
            Frag::text("κσ.$clone(").append(f).push(")").clone()
        } else {
            f
        }
    }

    fn emit_args(&mut self, args: Option<ast::ArgList>, expected: &[Ty]) -> Vec<Frag> {
        let Some(args) = args else { return vec![] };
        args.args()
            .enumerate()
            .map(|(i, a)| {
                let exp = expected.get(i).cloned();
                let (f, ty) = self.emit_expr(&a, exp.as_ref());
                self.by_value(&a, f, &ty)
            })
            .collect()
    }

    /// A call of the function `name` (local or imported).
    fn emit_fn_call(&mut self, c: &ast::CallExpr, name: &str, callee_off: usize) -> (Frag, Ty) {
        let Some(sig) = self.fns.get(name) else {
            let callee = c
                .expr()
                .map(|e| e.syntax().clone())
                .unwrap_or_else(|| c.syntax().clone());
            self.error(
                &callee,
                format!("cannot find function `{name}` in this scope"),
            );
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let (js, params, ret, is_async) = (
            sig.js.clone(),
            sig.params.clone(),
            sig.ret.clone(),
            sig.is_async,
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
        let args = self.emit_args(c.arg_list(), &params);
        let mut f = Frag::mapped(callee_off, js);
        f.push("(").append(Frag::join(args, ", ")).push(")");
        let ty = if is_async {
            Ty::Future(Box::new(ret))
        } else {
            ret
        };
        (f, ty)
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
        segs[i..].to_vec()
    }

    fn emit_call(&mut self, c: &ast::CallExpr) -> (Frag, Ty) {
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
            // `Some(x)` is `x` itself.
            let arg = self
                .emit_args(c.arg_list(), &[])
                .into_iter()
                .next()
                .unwrap_or_else(|| Frag::text("undefined"));
            return (arg.wrap("(", ")"), Ty::Option(Box::new(Ty::Unknown)));
        }
        if let Some(name) = path.as_single_name_ref() {
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
            return self.emit_fn_call(c, name.text().to_string().as_str(), callee_off);
        }
        let segs: Vec<String> = path
            .segments()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect();
        if let [single] = self.strip_modules(&segs).as_slice()
            && self.fns.contains_key(single)
        {
            return self.emit_fn_call(c, single, callee_off);
        }
        if let Some(r) = self.emit_adt_call(c, &segs) {
            return r;
        }
        {
            let ss: Vec<&str> = segs.iter().map(String::as_str).collect();
            if let Some(r) = self.emit_std_call(c, &ss) {
                return r;
            }
        }
        let segs: Vec<&str> = segs.iter().map(String::as_str).collect();
        let tail = match segs.as_slice() {
            ["std" | "core", rest @ ..] => rest,
            all => all,
        };
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
            ["String", "from"] => {
                let a = self.emit_args(c.arg_list(), &[Ty::Str]);
                (
                    a.into_iter().next().unwrap_or_else(|| Frag::text("\"\"")),
                    Ty::Str,
                )
            }
            ["cmp", f @ ("max" | "min")] => {
                let a = self.emit_args(c.arg_list(), &[]);
                let mut fr = Frag::text(format!("Math.{f}("));
                fr.append(Frag::join(a, ", ")).push(")");
                (fr, Ty::Unknown)
            }
            [ty_name, "from"] if Ty::from_name(ty_name).is_numeric() => {
                let ty = Ty::from_name(ty_name);
                let a = self.emit_args(c.arg_list(), &[]);
                (a.into_iter().next().unwrap_or_else(|| Frag::text("0")), ty)
            }
            _ => {
                self.error(
                    callee.syntax(),
                    format!(
                        "cannot find function `{}` (paths are resolved in phase 2)",
                        path.syntax().text()
                    ),
                );
                (Frag::text("undefined"), Ty::Unknown)
            }
        }
    }

    fn emit_field(&mut self, fe: &ast::FieldExpr) -> (Frag, Ty) {
        let (Some(recv), Some(name)) = (fe.expr(), fe.name_ref()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let (rf, rt) = self.receiver(&recv);
        if let Ty::Adt(n) = &rt {
            return self.emit_adt_field(rf, n, &name);
        }
        if let Some(i) = name.as_tuple_field() {
            if let Ty::Tuple(es) = &rt {
                let ty = es.get(i).cloned().unwrap_or(Ty::Unknown);
                let mut f = rf;
                f.append(Frag::mapped(self.off(name.syntax()), format!("[{i}]")));
                return (f, ty);
            }
            self.unsupported(fe.syntax(), "fields of this type", 1);
        }
        match rt {
            Ty::Extern(type_name) => {
                let js = extern_js_name(&type_name, name.text(), false);
                let mut f = rf;
                f.append(Frag::mapped(self.off(name.syntax()), member(&js)));
                (f, Ty::Unknown)
            }
            Ty::Unknown => {
                let mut f = rf;
                f.push(format!(".{}", name.text()));
                (f, Ty::Unknown)
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

    fn emit_method(&mut self, m: &ast::MethodCallExpr) -> (Frag, Ty) {
        let (Some(recv), Some(name)) = (m.receiver(), m.name_ref()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
        let name_off = self.off(name.syntax());
        let method = name.text().to_string();
        let node = m.syntax().clone();
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
        let (rf, rt) = self.receiver(&recv);

        if let Ty::Extern(type_name) = &rt {
            // NS API: snake_case Rust names map to the camelCase JS API, kept as a literal
            // `ns.method(...)` member call so Bitburner's RAM calculator sees it.
            let args = self.emit_args(m.arg_list(), &[]);
            let mut f = rf;
            let js = extern_js_name(type_name, &method, true);
            f.append(Frag::mapped(name_off, member(&js)));
            f.push("(").append(Frag::join(args, ", ")).push(")");
            return (f, Ty::Unknown);
        }

        if let Ty::Adt(n) = &rt
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
                // A method from a trait bound: dynamic dispatch.
                _ => {
                    let args = self.emit_args(m.arg_list(), &[]);
                    let mut f = rf.clone();
                    f.push(".")
                        .append(Frag::mapped(name_off, adt_gen::prop(&method)))
                        .push("(")
                        .append(Frag::join(args, ", "))
                        .push(")");
                    f
                }
            };
            return (f, ret);
        }
        if let Ty::Dyn(_) = &rt {
            // `dyn Trait`: dynamic dispatch.
            let name_off = self.off(name.syntax());
            let args = self.emit_args(m.arg_list(), &[]);
            let mut f = rf.clone();
            f.push(".")
                .append(Frag::mapped(name_off, adt_gen::prop(&method)))
                .push("(")
                .append(Frag::join(args, ", "))
                .push(")");
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

        // NS values are untyped until phase 2 bindings; NS numbers are f64, so assume that for
        // float-only method names.
        let rt = match rt {
            Ty::Unknown if is_float_method(&method) => Ty::Float { f32: false },
            Ty::IntLit => Ty::Int(IntKind::I32),
            t => t,
        };
        if matches!(method.as_str(), "clone" | "to_owned" | "as_str" | "into") && rt != Ty::Unknown
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
                    "to_string" => (fcall("κσ.$fmtF", vec![rf.clone()]), Ty::Str),
                    _ => unreachable!(),
                }
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
                "push_str" | "push" => {
                    // Lowered as `s += x`, so the receiver must be an assignable JS place.
                    if !is_pure(&strip_refs(&recv)) {
                        self.unsupported(&node, "`push_str` on this expression", 1);
                    }
                    let mut f = rf.clone();
                    f.push(" += ").append(a(0));
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
        let (f, _) = self.emit_expr(e, None);
        if is_pure(e) {
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
            ast::Pat::PathPat(pp) if crate::typeck::ordering_variant(pp.path()).is_some() => {
                let n = crate::typeck::ordering_variant(pp.path()).unwrap();
                out.conds.push(Frag::text(format!("{subject} === {n}")));
            }
            ast::Pat::PathPat(pp) if path_is(pp.path(), "None") => {
                out.conds.push(Frag::text(format!("{subject} == null")))
            }
            ast::Pat::TupleStructPat(tp) if crate::typeck::result_variant(tp.path()).is_some() => {
                let v = crate::typeck::result_variant(tp.path()).unwrap();
                self.lower_result_pat(tp, v, subject, out);
            }
            ast::Pat::TupleStructPat(tp) if path_is(tp.path(), "Some") => {
                // `Some(x)` is represented by `x` itself.
                out.conds.push(Frag::text(format!("{subject} != null")));
                if let Some(f) = tp.fields().next() {
                    self.lower_pat(&f, subject, out);
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
                    let ast::Pat::LiteralPat(lp) = b? else {
                        return None;
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
                let mut alts = Vec::new();
                for alt in op.pats() {
                    let mut pc = PatCode::default();
                    self.lower_pat(&alt, subject, &mut pc);
                    match pc.condition() {
                        Some(c) => alts.push(c),
                        None => return, // an irrefutable alternative: the whole pattern always matches
                    }
                }
                out.conds.push(Frag::join(alts, " || ").wrap("(", ")"));
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
                self.unsupported(other.syntax(), what, 1);
            }
        }
    }

    fn pat_literal(&mut self, lp: &ast::LiteralPat) -> Frag {
        let Some(l) = lp.literal() else {
            return Frag::text("undefined");
        };
        let (f, _) = self.emit_literal(&l, None);
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
        let mut first = true;
        for arm in &arms {
            let Some(pat) = arm.pat() else { continue };
            let arm_off = self.off(arm.syntax());
            let mut pc = PatCode::default();
            self.lower_pat(&pat, &subject, &mut pc);
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
                    1,
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
        match method {
            "is_some" => (rf.clone().wrap("(", " != null)"), Ty::Bool),
            "is_none" => (rf.clone().wrap("(", " == null)"), Ty::Bool),
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
            "as_ref" | "as_mut" | "as_deref" | "cloned" | "copied" => (rf, opt),
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

    // ---------------------------------------------------------------- HashMap

    /// `map.entry(k).or_insert(d)` / `.or_default()` / `.or_insert_with(Vec::new)`: the map's
    /// `[key, value]` storage slot for `k`, inserting the default first if the key is missing.
    fn entry_slot(&mut self, m: &ast::MethodCallExpr) -> Option<Frag> {
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
            "get" | "get_mut" => (call("get", vec![a(0)]), opt),
            "remove" => (call("remove", vec![a(0)]), opt),
            "contains_key" => (call("containsKey", vec![a(0)]), Ty::Bool),
            "len" => (call("len", vec![]), Ty::Int(IntKind::Usize)),
            "is_empty" => (call("len", vec![]).wrap("(", " === 0)"), Ty::Bool),
            "clear" => (call("clear", vec![]), Ty::Unit),
            "clone" | "to_owned" => (call("clone", vec![]), map_ty),
            "entry" | "keys" | "values" | "values_mut" | "iter" | "iter_mut" | "into_iter"
            | "into_keys" | "into_values" => {
                let what = if method == "entry" {
                    "`.entry(..)` without `.or_insert(..)` / `.or_default()`".to_string()
                } else {
                    format!("`.{method}()` outside a `for` loop")
                };
                self.unsupported(m.syntax(), &what, 1);
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
                    1,
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

    /// `v[i]` with Rust's bounds check: `v[κσ.$ix(v, i, loc)]`, which stays assignable. When `v`
    /// isn't safe to evaluate twice, reads go through `κσ.$get` instead.
    fn emit_index(&mut self, ix: &ast::IndexExpr) -> (Frag, Ty) {
        let (Some(base), Some(index)) = (ix.base(), ix.index()) else {
            return (Frag::text("undefined"), Ty::Unknown);
        };
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
            let name = if bt == Ty::Str { "strSlice" } else { "slice" };
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
        let (bf, bt) = self.receiver(&base);
        let (xf, _) = self.emit_expr(&index, None);
        if let Ty::Map(_, v) = &bt {
            // rustc reports index panics at the `[`.
            let bracket = ix
                .l_brack_token()
                .map(|t| self.base + usize::from(t.text_range().start()));
            let loc = js_string(
                &bracket
                    .map(|o| self.location_at(o))
                    .unwrap_or_else(|| self.location(ix.syntax())),
            );
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
        // rustc reports index panics at the `[`.
        let bracket = ix
            .l_brack_token()
            .map(|t| self.base + usize::from(t.text_range().start()));
        let loc = js_string(
            &bracket
                .map(|o| self.location_at(o))
                .unwrap_or_else(|| self.location(ix.syntax())),
        );
        let f = if is_pure(&base) {
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
        match method {
            "len" => (rf.clone().push(".length").clone(), usize),
            "pop" => (helper("pop", vec![]), Ty::Option(Box::new(elem.clone()))),
            "first" => (helper("first", vec![]), Ty::Option(Box::new(elem.clone()))),
            "last" => (helper("last", vec![]), Ty::Option(Box::new(elem.clone()))),
            "get" => (
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
            // strings, bools and nested Vecs. Both sorts are stable.
            "sort" | "sort_unstable" => (mcall("sort", vec![Frag::text("κσ.$cmp")]), Ty::Unit),
            "contains" => (helper("contains", vec![a(0)]), Ty::Bool),
            "extend_from_slice" => (helper("extend", vec![a(0)]), Ty::Unit),
            "join" => (mcall("join", vec![a(0)]), Ty::Str),
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
                self.unsupported(
                    m.syntax(),
                    &format!("`.{method}()` outside a `for` loop"),
                    1,
                );
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
        let args = self.macro_args(mc);
        let rt = |f: &str| format!("κσ.${f}");
        match name.as_str() {
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
                let saved = std::mem::replace(&mut self.base, arg.start);
                let body = self.capture(|cg| {
                    cg.emit_into(&expr, &Dest::Return, Some(&Ty::Bool));
                });
                self.base = saved;
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
                self.unsupported(mc.syntax(), &format!("macro `{name}!` (`macro_rules!`)"), 4);
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
        let mut resolved: Vec<(Option<usize>, Option<String>, Spec)> = Vec::new();
        for p in &pieces {
            if let Piece::Arg { arg, spec } = p {
                let idx = match arg {
                    ArgRef::Next => {
                        next += 1;
                        Some(next - 1)
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
                resolved.push((idx, captured, spec.clone()));
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
                    let (idx, captured, spec) = res_iter.next().unwrap();
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
                            Some(l) => (Frag::text(l.js.clone()), l.ty.clone().default_lit()),
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
                    out.push("${").append(format_value(f, &t, &spec)).push("}");
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
        Ty::Adt(n) => format!("{}.default()", escape_ident(n)),
        other => other.default_js()?.into(),
    })
}

/// `[fmt0, fmt1, ...]`: one `{:?}` formatter per tuple element.
fn tuple_formatters(es: &[Ty]) -> String {
    let fs: Vec<String> = es
        .iter()
        .map(|e| debug_fn(e).unwrap_or_else(|| "String".into()))
        .collect();
    format!("[{}]", fs.join(", "))
}

/// A JS function that renders `{:?}` for a type, or None when `String(x)` already matches Rust.
fn debug_fn(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Str => Some("κσ.$dbgStr".into()),
        Ty::Char => Some("κσ.$dbgChar".into()),
        Ty::Float { .. } => Some("κσ.$dbgF".into()),
        Ty::Unknown | Ty::Extern(_) => Some("κσ.$dbg".into()),
        Ty::Ordering => Some("κσ.$dbgOrdering".into()),
        Ty::Adt(_) => Some("κσ.$dbg".into()),
        Ty::Param(_) => Some("κσ.$dbgAny".into()),
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
            tuple_formatters(es)
        )),
        _ => None,
    }
}

fn format_value(f: Frag, ty: &Ty, spec: &Spec) -> Frag {
    let call = |name: &str, f: Frag, extra: &str| {
        let mut g = Frag::text(format!("{name}("));
        g.append(f).push(extra).push(")");
        g
    };
    let numeric = ty.is_numeric() || *ty == Ty::Unknown;
    // A user `Display` impl formats itself; like `write!`-based impls in Rust, it ignores width.
    if !spec.debug && matches!(ty, Ty::Adt(_)) {
        return call("κσ.$display", f, "");
    }
    // A generic value: decided at run time.
    let f = if !spec.debug && matches!(ty, Ty::Param(_)) {
        call("κσ.$disp", f, "")
    } else {
        f
    };
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
    } else if let Some(p) = spec.precision {
        match ty {
            Ty::Str => call("κσ.$truncChars", f, &format!(", {p}")),
            Ty::Int(_) | Ty::IntLit => f,
            _ => call("κσ.$fixed", f, &format!(", {p}")),
        }
    } else {
        match ty {
            Ty::Float { .. } => call("κσ.$fmtF", f, ""),
            Ty::Unknown => call("κσ.$disp", f, ""),
            _ => f,
        }
    };
    if spec.plus && numeric {
        s = call("κσ.$plus", s, "");
    }
    if let Some(w) = spec.width {
        let align = spec
            .align
            .map(|c| js_string(&c.to_string()))
            .unwrap_or_else(|| "null".into());
        let fill = js_string(&spec.fill.unwrap_or(' ').to_string());
        s = call(
            "κσ.$pad",
            s,
            &format!(", {w}, {align}, {fill}, {}, {numeric}", spec.zero),
        );
    }
    s
}
