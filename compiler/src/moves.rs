//! Move checking: a use of a binding after its value was moved is an error, as in rustc. Also
//! definite initialization: a `let x;` must be assigned on every path before it's used, and only
//! once unless it's `mut`.
//!
//! In JS a "moved" object is still reachable through the old name, so without this check code that
//! rustc rejects would run with shared-reference semantics. The checker is deliberately
//! conservative (types are partly erased, so references and owned values can look alike): it
//! tracks only bindings that certainly own a non-`Copy` value, and only the moves listed in
//! `Walker::expr`. Control flow is followed: branches are checked separately and joined, loop
//! bodies run twice (to catch a move in a previous iteration), and assigning a new value to a
//! moved binding makes it usable again.

use std::collections::{HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, SyntaxNode,
    ast::{self, HasArgList, HasLoopBody, HasName},
};

use crate::adt::Adts;
use crate::lineindex::LineIndex;
use crate::macros;
use crate::ty::Ty;
use crate::typeck::RawDiag;

/// A binding, by its name's absolute range.
type Decl = (usize, usize);

#[derive(Clone, Default)]
struct State {
    /// Moved bindings: where, and how (`UNINIT` for a `let x;` not yet assigned).
    moved: HashMap<Decl, (usize, &'static str)>,
    /// `let x;` bindings that may have been assigned already.
    assigned: HashSet<Decl>,
    /// `let x;` bindings assigned on no path yet.
    unassigned: HashSet<Decl>,
    /// After `return`/`break`/`continue`: nothing below runs.
    unreachable: bool,
}

/// How a `let x;` binding without a value is "moved": it has no value yet.
const UNINIT: &str = "declared without a value";

impl State {
    fn join(a: State, b: State) -> State {
        if a.unreachable {
            return b;
        }
        if b.unreachable {
            return a;
        }
        let mut moved = a.moved;
        for (k, v) in b.moved {
            moved.entry(k).or_insert(v);
        }
        let mut assigned = a.assigned;
        assigned.extend(b.assigned);
        let unassigned = a.unassigned.intersection(&b.unassigned).copied().collect();
        State {
            moved,
            assigned,
            unassigned,
            unreachable: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Use,
    Move(&'static str),
}

struct Walker<'a> {
    src: &'a str,
    types: &'a HashMap<(usize, usize), Ty>,
    adts: &'a Adts,
    /// Functions declared in the file, by impl type (`None` for free functions) and name: whether
    /// they certainly return an owned value (no references anywhere in the return type).
    fns: HashMap<(Option<String>, String), bool>,
    /// The type of the `impl` being checked, for `Self::new()`.
    cur_impl: Option<String>,
    base: usize,
    scopes: Vec<HashMap<String, Decl>>,
    /// Bindings that own a non-`Copy` value.
    tracked: HashSet<Decl>,
    /// `let x;` bindings (declared without a value), and whether they're `mut`.
    deferred: HashMap<Decl, bool>,
    names: HashMap<Decl, String>,
    reported: HashSet<(Decl, usize)>,
    diags: Vec<RawDiag>,
    /// Enclosing loops and labeled blocks, innermost last.
    targets: Vec<Target>,
}

/// Where `break`/`continue` go: the states they leave with.
struct Target {
    label: Option<String>,
    is_loop: bool,
    breaks: Vec<State>,
    continues: Vec<State>,
}

/// Check every function and method body in `file`.
pub fn check(
    file: &ast::SourceFile,
    src: &str,
    types: &HashMap<(usize, usize), Ty>,
    adts: &Adts,
) -> Vec<RawDiag> {
    let mut fns = HashMap::new();
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(n) = f.name() else { continue };
        let owned = f.ret_type().and_then(|r| r.ty()).is_some_and(|t| {
            !t.syntax().text().contains_char('&') && !matches!(t, ast::Type::ImplTraitType(_))
        });
        // Same-named functions that disagree (in different modules) aren't trusted.
        fns.entry((impl_of(&f), n.text().to_string()))
            .and_modify(|o| *o &= owned)
            .or_insert(owned);
    }
    let mut w = Walker {
        src,
        types,
        adts,
        fns,
        cur_impl: None,
        base: 0,
        scopes: Vec::new(),
        tracked: HashSet::new(),
        deferred: HashMap::new(),
        names: HashMap::new(),
        reported: HashSet::new(),
        diags: Vec::new(),
        targets: Vec::new(),
    };
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        if !crate::adt::test_only(f.syntax()) {
            w.function(&f);
        }
    }
    w.diags
}

impl Walker<'_> {
    fn range(&self, n: &SyntaxNode) -> (usize, usize) {
        let r = n.text_range();
        (
            self.base + usize::from(r.start()),
            self.base + usize::from(r.end()),
        )
    }

    fn ty(&self, n: &SyntaxNode) -> Option<&Ty> {
        self.types.get(&self.range(n))
    }

    /// Whether values of `t` are moved (not copied) by assignment.
    fn moves(&self, t: &Ty) -> bool {
        match t {
            Ty::Vec(_)
            | Ty::Set(_)
            | Ty::Map(..)
            | Ty::Coll(..)
            | Ty::RefCell(_)
            | Ty::Dyn(_)
            | Ty::Iter { .. } => true,
            Ty::Adt(n, _) => self.adts.defs.get(n).is_some_and(|d| !d.derives("Copy")),
            Ty::Option(x) => self.moves(x),
            Ty::Result(a, b) => self.moves(a) || self.moves(b),
            Ty::Tuple(es) => es.iter().any(|e| self.moves(e)),
            _ => false,
        }
    }

    fn function(&mut self, f: &ast::Fn) {
        let Some(body) = f.body() else { return };
        self.cur_impl = impl_of(f);
        self.scopes = vec![HashMap::new()];
        if let Some(pl) = f.param_list() {
            for p in pl.params() {
                let (Some(ast::Pat::IdentPat(ip)), Some(t)) = (p.pat(), p.ty()) else {
                    continue;
                };
                let Some(n) = ip.name() else { continue };
                let decl = self.range(n.syntax());
                // By-value parameters own their argument; `&T` ones don't.
                let text = t.syntax().text().to_string();
                let owned = !text.contains('&')
                    && !matches!(t, ast::Type::ImplTraitType(_))
                    && (self.ty(n.syntax()).is_some_and(|ty| self.moves(ty)) || text == "String");
                self.declare(&n, decl, owned);
            }
        }
        let mut st = State::default();
        self.block(&body, &mut st, Mode::Use);
    }

    fn declare(&mut self, n: &ast::Name, decl: Decl, owned: bool) {
        self.scopes
            .last_mut()
            .unwrap()
            .insert(n.text().to_string(), decl);
        self.names.insert(decl, n.text().to_string());
        if owned {
            self.tracked.insert(decl);
        }
    }

    fn lookup(&self, name: &str) -> Option<Decl> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    /// Bind every name in a pattern (untracked: pattern bindings may be references).
    fn bind_pattern(&mut self, p: &ast::Pat) {
        for ip in p.syntax().descendants().filter_map(ast::IdentPat::cast) {
            if let Some(n) = ip.name() {
                let decl = self.range(n.syntax());
                self.declare(&n, decl, false);
            }
        }
    }

    /// Whether `init` certainly produces an owned value (so the binding owns it).
    fn owned_init(&self, init: &ast::Expr) -> bool {
        match strip(init) {
            ast::Expr::RecordExpr(_) | ast::Expr::ArrayExpr(_) | ast::Expr::TupleExpr(_) => true,
            ast::Expr::MacroExpr(m) => m
                .macro_call()
                .and_then(|mc| macros::macro_name(&mc))
                .is_some_and(|n| matches!(n.as_str(), "vec" | "format")),
            ast::Expr::PathExpr(p) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .and_then(|n| self.lookup(n.text().to_string().as_str()))
                .is_some_and(|d| self.tracked.contains(&d)),
            ast::Expr::CallExpr(c) => match c.expr() {
                Some(ast::Expr::PathExpr(p)) => {
                    let segs: Vec<String> = p
                        .path()
                        .map(|p| {
                            p.segments()
                                .filter_map(|s| s.name_ref())
                                .map(|n| n.text().to_string())
                                .collect()
                        })
                        .unwrap_or_default();
                    match segs.as_slice() {
                        [.., t, f]
                            if matches!(
                                t.as_str(),
                                "Vec" | "HashMap" | "HashSet" | "String" | "RefCell" | "Box"
                            ) && matches!(f.as_str(), "new" | "from" | "with_capacity") =>
                        {
                            true
                        }
                        // A tuple struct.
                        [f] if self.adts.defs.contains_key(f.as_str()) => true,
                        // A user function that doesn't return a reference.
                        [f] => self.fns.get(&(None, f.clone())).copied().unwrap_or(false),
                        [.., t, f] => {
                            let t = if t == "Self" {
                                self.cur_impl.clone().unwrap_or_default()
                            } else {
                                t.clone()
                            };
                            match self.fns.get(&(Some(t.clone()), f.clone())) {
                                Some(&o) => o,
                                // An enum variant, or a derived `default()`.
                                None => self.adts.defs.get(&t).is_some_and(|d| {
                                    f == "default" || d.variants.iter().any(|v| v.name == *f)
                                }),
                            }
                        }
                        _ => false,
                    }
                }
                _ => false,
            },
            ast::Expr::MethodCallExpr(m) => m.name_ref().is_some_and(|n| {
                matches!(
                    n.text().to_string().as_str(),
                    "clone"
                        | "to_string"
                        | "to_owned"
                        | "to_vec"
                        | "collect"
                        | "into_iter"
                        | "to_uppercase"
                        | "to_lowercase"
                ) || match m.receiver().and_then(|r| self.ty(r.syntax()).cloned()) {
                    Some(Ty::Adt(t, _)) => self
                        .fns
                        .get(&(Some(t), n.text().to_string()))
                        .copied()
                        .unwrap_or(false),
                    _ => false,
                }
            }),
            _ => false,
        }
    }

    /// A read of a binding; a move when `mode` says so.
    fn path(&mut self, p: &ast::PathExpr, st: &mut State, mode: Mode) {
        let Some(name) = p.path().and_then(|p| p.as_single_name_ref()) else {
            return;
        };
        let at = self.range(p.syntax());
        self.touch(name.text(), at, st, mode);
    }

    /// A read of the binding `name` at `at`.
    fn touch(&mut self, name: &str, at: (usize, usize), st: &mut State, mode: Mode) {
        let Some(decl) = self.lookup(name) else {
            return;
        };
        if !self.tracked.contains(&decl) && !self.deferred.contains_key(&decl) {
            return;
        }
        if let Some(&(moved_at, how)) = st.moved.get(&decl)
            && !st.unreachable
            && self.reported.insert((decl, at.0))
        {
            let idx = LineIndex::new(self.src);
            let (l, c) = idx.line_col(moved_at);
            let n = &self.names[&decl];
            self.diags.push(RawDiag {
                start: at.0,
                end: at.1,
                message: if how == UNINIT && st.unassigned.contains(&decl) {
                    format!(
                        "used binding `{n}` isn't initialized (declared without a value on line {}; assign it before this use)",
                        l + 1
                    )
                } else if how == UNINIT {
                    format!(
                        "used binding `{n}` is possibly-uninitialized (declared without a value on line {}; assign it on every path before this use)",
                        l + 1
                    )
                } else if moved_at == at.0 {
                    format!("use of moved value: `{n}` (moved here {how} in the loop's previous iteration; `.clone()` it to keep a copy)")
                } else {
                    format!(
                        "use of moved value: `{n}` (moved on line {}:{} {how}; `.clone()` it there to keep a copy)",
                        l + 1,
                        c + 1
                    )
                },
                severity: "error",
            });
        }
        if let Mode::Move(how) = mode
            && !st.unreachable
            && self.tracked.contains(&decl)
        {
            st.moved.insert(decl, (at.0, how));
        }
    }

    fn block(&mut self, b: &ast::BlockExpr, st: &mut State, mode: Mode) {
        let Some(list) = b.stmt_list() else { return };
        self.scopes.push(HashMap::new());
        for s in list.statements() {
            match s {
                ast::Stmt::LetStmt(l) => self.let_stmt(&l, st),
                ast::Stmt::ExprStmt(es) => {
                    if let Some(e) = es.expr() {
                        self.expr(&e, st, Mode::Use);
                    }
                }
                ast::Stmt::Item(_) => {}
            }
        }
        if let Some(t) = list.tail_expr() {
            self.expr(&t, st, mode);
        }
        self.scopes.pop();
    }

    fn let_stmt(&mut self, l: &ast::LetStmt, st: &mut State) {
        let init = l.initializer();
        if let Some(e) = &init {
            self.expr(e, st, Mode::Move("by `let`"));
        }
        if let Some(le) = l.let_else()
            && let Some(b) = le.block_expr()
        {
            let mut s2 = st.clone();
            self.block(&b, &mut s2, Mode::Use);
        }
        let Some(pat) = l.pat() else { return };
        match &pat {
            ast::Pat::IdentPat(ip) if ip.pat().is_none() && ip.ref_token().is_none() => {
                let Some(n) = ip.name() else { return };
                let decl = self.range(n.syntax());
                let typed_string = l.ty().is_some_and(|t| t.syntax().text() == "String");
                let owned = match &init {
                    Some(e) => {
                        self.owned_init(e)
                            && (self.ty(n.syntax()).is_some_and(|t| self.moves(t))
                                || (typed_string || self.owned_string(e)))
                    }
                    None => self.ty(n.syntax()).is_some_and(|t| self.moves(t)),
                };
                self.declare(&n, decl, owned);
                st.moved.remove(&decl);
                if init.is_none() {
                    self.deferred.insert(decl, ip.mut_token().is_some());
                    st.moved.insert(decl, (decl.0, UNINIT));
                    st.assigned.remove(&decl);
                    st.unassigned.insert(decl);
                }
            }
            other => self.bind_pattern(other),
        }
    }

    /// A `String` (not a `&str`) from its initializer.
    fn owned_string(&self, e: &ast::Expr) -> bool {
        match strip(e) {
            ast::Expr::MacroExpr(m) => m
                .macro_call()
                .and_then(|mc| macros::macro_name(&mc))
                .is_some_and(|n| n == "format"),
            ast::Expr::CallExpr(_) => {
                let t = e.syntax().text().to_string();
                t.starts_with("String::")
            }
            ast::Expr::MethodCallExpr(m) => m.name_ref().is_some_and(|n| {
                matches!(
                    n.text().to_string().as_str(),
                    "to_string" | "to_owned" | "to_uppercase" | "to_lowercase"
                )
            }),
            _ => false,
        }
    }

    /// Whether `m` consumes its receiver.
    fn consumes_receiver(&self, m: &ast::MethodCallExpr) -> bool {
        let Some(name) = m.name_ref().map(|n| n.text().to_string()) else {
            return false;
        };
        let rt = m.receiver().and_then(|r| self.ty(r.syntax()).cloned());
        // `x.into()` takes `self` whatever `x` is.
        if name == "into"
            && m.arg_list().is_some_and(|l| l.args().next().is_none())
            && !matches!(&rt, Some(Ty::Adt(n, _)) if self.adts.method(n, "into").is_some())
        {
            return true;
        }
        match &rt {
            Some(Ty::Adt(adt, _)) => self
                .adts
                .method(adt, &name)
                .is_some_and(|s| s.receiver == Some(crate::adt::Receiver::Value)),
            // Adapters and consumers; `next`, `by_ref`, `any`, `find`, .. take `&mut self`.
            Some(Ty::Iter { .. }) => matches!(
                name.as_str(),
                "map"
                    | "filter"
                    | "filter_map"
                    | "flat_map"
                    | "flatten"
                    | "enumerate"
                    | "zip"
                    | "chain"
                    | "rev"
                    | "skip"
                    | "take"
                    | "skip_while"
                    | "take_while"
                    | "step_by"
                    | "peekable"
                    | "cloned"
                    | "copied"
                    | "inspect"
                    | "scan"
                    | "fuse"
                    | "cycle"
                    | "collect"
                    | "sum"
                    | "product"
                    | "count"
                    | "fold"
                    | "reduce"
                    | "for_each"
                    | "last"
                    | "max"
                    | "min"
                    | "max_by"
                    | "min_by"
                    | "max_by_key"
                    | "min_by_key"
                    | "partition"
                    | "unzip"
            ),
            Some(Ty::Option(_) | Ty::Result(..)) => matches!(
                name.as_str(),
                "unwrap"
                    | "expect"
                    | "unwrap_or"
                    | "unwrap_or_default"
                    | "unwrap_or_else"
                    | "map"
                    | "and_then"
                    | "ok"
                    | "err"
            ),
            Some(Ty::Vec(_) | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..)) => {
                matches!(name.as_str(), "into_iter" | "into_keys" | "into_values")
            }
            Some(Ty::RefCell(_)) => name == "into_inner",
            _ => false,
        }
    }

    fn expr(&mut self, e: &ast::Expr, st: &mut State, mode: Mode) {
        match e {
            ast::Expr::PathExpr(p) => self.path(p, st, mode),
            ast::Expr::ParenExpr(p) => {
                if let Some(i) = p.expr() {
                    self.expr(&i, st, mode);
                }
            }
            ast::Expr::RefExpr(r) => {
                if let Some(i) = r.expr() {
                    self.expr(&i, st, Mode::Use);
                }
            }
            ast::Expr::BinExpr(b) => {
                let (lhs, rhs) = (b.lhs(), b.rhs());
                match b.op_kind() {
                    Some(ast::BinaryOp::Assignment { op: None }) => {
                        if let Some(r) = &rhs {
                            self.expr(r, st, Mode::Move("by assignment"));
                        }
                        // Assigning to a moved binding gives it a value again.
                        match lhs.as_ref().map(strip) {
                            Some(ast::Expr::PathExpr(p)) => {
                                if let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
                                    && let Some(d) = self.lookup(n.text().to_string().as_str())
                                {
                                    self.assign_deferred(d, self.range(p.syntax()), st);
                                    st.moved.remove(&d);
                                }
                            }
                            Some(other) => self.expr(&other, st, Mode::Use),
                            None => {}
                        }
                    }
                    // `s1 + &s2` moves `s1`.
                    Some(ast::BinaryOp::ArithOp(ast::ArithOp::Add))
                        if lhs
                            .as_ref()
                            .and_then(|l| self.ty(l.syntax()))
                            .is_some_and(|t| *t == Ty::Str) =>
                    {
                        if let Some(l) = &lhs {
                            self.expr(l, st, Mode::Move("by `+`"));
                        }
                        if let Some(r) = &rhs {
                            self.expr(r, st, Mode::Use);
                        }
                    }
                    Some(ast::BinaryOp::LogicOp(_)) => {
                        if let Some(l) = &lhs {
                            self.expr(l, st, Mode::Use);
                        }
                        // The right side may not run.
                        if let Some(r) = &rhs {
                            let mut s2 = st.clone();
                            self.expr(r, &mut s2, Mode::Use);
                            *st = State::join(st.clone(), s2);
                        }
                    }
                    _ => {
                        for x in [lhs, rhs].into_iter().flatten() {
                            self.expr(&x, st, Mode::Use);
                        }
                    }
                }
            }
            ast::Expr::CallExpr(c) => {
                if let Some(f) = c.expr() {
                    self.expr(&f, st, Mode::Use);
                }
                for a in c.arg_list().into_iter().flat_map(|l| l.args()) {
                    self.expr(&a, st, Mode::Move("as an argument"));
                }
            }
            ast::Expr::MethodCallExpr(m) => {
                if let Some(r) = m.receiver() {
                    let how = if self.consumes_receiver(m) {
                        Mode::Move("by a method that takes `self`")
                    } else {
                        Mode::Use
                    };
                    self.expr(&r, st, how);
                }
                for a in m.arg_list().into_iter().flat_map(|l| l.args()) {
                    self.expr(&a, st, Mode::Move("as an argument"));
                }
            }
            ast::Expr::FieldExpr(f) => {
                if let Some(x) = f.expr() {
                    self.expr(&x, st, Mode::Use);
                }
            }
            ast::Expr::IndexExpr(ix) => {
                for x in [ix.base(), ix.index()].into_iter().flatten() {
                    self.expr(&x, st, Mode::Use);
                }
            }
            ast::Expr::RecordExpr(r) => {
                if let Some(l) = r.record_expr_field_list() {
                    for f in l.fields() {
                        match f.expr() {
                            Some(x) => self.expr(&x, st, Mode::Move("into a struct")),
                            // Shorthand `Point { x }` moves `x`.
                            None => {
                                if let Some(n) = f.field_name()
                                    && let Some(d) = self.lookup(n.text().to_string().as_str())
                                    && self.tracked.contains(&d)
                                {
                                    let at = self.range(f.syntax());
                                    if !st.unreachable {
                                        st.moved.insert(d, (at.0, "into a struct"));
                                    }
                                }
                            }
                        }
                    }
                    if let Some(b) = l.spread() {
                        self.expr(&b, st, Mode::Use);
                    }
                }
            }
            ast::Expr::TupleExpr(t) => {
                for x in t.fields() {
                    self.expr(&x, st, Mode::Move("into a tuple"));
                }
            }
            ast::Expr::ArrayExpr(a) => {
                for x in a.syntax().children().filter_map(ast::Expr::cast) {
                    self.expr(&x, st, Mode::Move("into an array"));
                }
            }
            ast::Expr::MacroExpr(m) => {
                let Some(mc) = m.macro_call() else { return };
                let name = macros::macro_name(&mc).unwrap_or_default();
                let mode = if name == "vec" {
                    Mode::Move("into a `vec!`")
                } else {
                    Mode::Use
                };
                let args = macros::split_args(self.src, self.base, &mc);
                // `{name}` in a format string reads `name`.
                if name != "vec"
                    && let Some(fs) = args.iter().find_map(macros::format_string)
                    && let Ok(pieces) = crate::fmt::parse(&fs)
                {
                    let at = self.range(mc.syntax());
                    for piece in pieces {
                        if let crate::fmt::Piece::Arg {
                            arg: crate::fmt::ArgRef::Name(n),
                            ..
                        } = piece
                            && !args.iter().any(|a| a.name.as_deref() == Some(n.as_str()))
                        {
                            self.touch(&n, at, st, Mode::Use);
                        }
                    }
                }
                for a in args {
                    let (Some(x), _) = macros::parse_arg(&a) else {
                        continue;
                    };
                    let saved = std::mem::replace(&mut self.base, a.start);
                    self.expr(&x, st, mode);
                    self.base = saved;
                }
            }
            ast::Expr::BlockExpr(b) => match b.label() {
                // `'a: { .. break 'a .. }`: the block ends at its end or at a `break 'a`.
                Some(l) => {
                    let label = l.lifetime().map(|l| l.text().to_string());
                    self.targets.push(Target {
                        label,
                        is_loop: false,
                        breaks: vec![],
                        continues: vec![],
                    });
                    self.block(b, st, mode);
                    let t = self.targets.pop().unwrap();
                    for s2 in t.breaks {
                        *st = State::join(st.clone(), s2);
                    }
                }
                None => self.block(b, st, mode),
            },
            ast::Expr::IfExpr(i) => {
                if let Some(c) = i.condition() {
                    self.condition(&c, st);
                }
                let mut then_st = st.clone();
                self.scopes.push(HashMap::new());
                if let Some(c) = i.condition() {
                    for le in c.syntax().descendants().filter_map(ast::LetExpr::cast) {
                        if let Some(p) = le.pat() {
                            self.bind_pattern(&p);
                        }
                    }
                }
                if let Some(b) = i.then_branch() {
                    self.block(&b, &mut then_st, mode);
                }
                self.scopes.pop();
                let mut else_st = st.clone();
                match i.else_branch() {
                    Some(ast::ElseBranch::Block(b)) => self.block(&b, &mut else_st, mode),
                    Some(ast::ElseBranch::IfExpr(ei)) => {
                        self.expr(&ast::Expr::IfExpr(ei), &mut else_st, mode)
                    }
                    None => {}
                }
                *st = State::join(then_st, else_st);
            }
            ast::Expr::MatchExpr(mx) => {
                if let Some(s) = mx.expr() {
                    self.expr(&s, st, Mode::Use);
                }
                let mut out: Option<State> = None;
                for arm in mx.match_arm_list().into_iter().flat_map(|l| l.arms()) {
                    let mut s2 = st.clone();
                    self.scopes.push(HashMap::new());
                    if let Some(p) = arm.pat() {
                        self.bind_pattern(&p);
                    }
                    if let Some(g) = arm.guard().and_then(|g| g.condition()) {
                        self.expr(&g, &mut s2, Mode::Use);
                    }
                    if let Some(b) = arm.expr() {
                        self.expr(&b, &mut s2, mode);
                    }
                    self.scopes.pop();
                    out = Some(match out {
                        Some(o) => State::join(o, s2),
                        None => s2,
                    });
                }
                if let Some(o) = out {
                    *st = o;
                }
            }
            ast::Expr::WhileExpr(w) => {
                let body = w.loop_body();
                let cond = w.condition();
                let label = w
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.looped(st, label, false, |me, s| {
                    me.scopes.push(HashMap::new());
                    if let Some(c) = &cond {
                        me.condition(c, s);
                        for le in c.syntax().descendants().filter_map(ast::LetExpr::cast) {
                            if let Some(p) = le.pat() {
                                me.bind_pattern(&p);
                            }
                        }
                    }
                    if let Some(b) = &body {
                        me.block(b, s, Mode::Use);
                    }
                    me.scopes.pop();
                });
            }
            ast::Expr::LoopExpr(l) => {
                let body = l.loop_body();
                let label = l
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.looped(st, label, true, |me, s| {
                    if let Some(b) = &body {
                        me.block(b, s, Mode::Use);
                    }
                });
            }
            ast::Expr::ForExpr(f) => {
                // `for x in v` consumes `v`.
                if let Some(it) = f.iterable() {
                    self.expr(&it, st, Mode::Move("by a `for` loop"));
                }
                let body = f.loop_body();
                let pat = f.pat();
                let label = f
                    .label()
                    .and_then(|l| l.lifetime())
                    .map(|l| l.text().to_string());
                self.looped(st, label, false, |me, s| {
                    me.scopes.push(HashMap::new());
                    if let Some(p) = &pat {
                        me.bind_pattern(p);
                    }
                    if let Some(b) = &body {
                        me.block(b, s, Mode::Use);
                    }
                    me.scopes.pop();
                });
            }
            ast::Expr::ClosureExpr(c) => {
                let moves_captures = c.move_token().is_some();
                let Some(body) = c.body() else { return };
                // The body runs later: uses are checked now, its own moves don't leak out.
                let mut inner = st.clone();
                self.scopes.push(HashMap::new());
                if let Some(pl) = c.param_list() {
                    for p in pl.params() {
                        if let Some(pat) = p.pat() {
                            self.bind_pattern(&pat);
                        }
                    }
                }
                self.expr(&body, &mut inner, Mode::Use);
                let captured: Vec<Decl> = body
                    .syntax()
                    .descendants()
                    .filter_map(ast::PathExpr::cast)
                    .filter_map(|p| p.path()?.as_single_name_ref())
                    .filter_map(|n| self.lookup(n.text().to_string().as_str()))
                    .collect();
                self.scopes.pop();
                if moves_captures && !st.unreachable {
                    let at = self.range(c.syntax()).0;
                    for d in captured {
                        if self.tracked.contains(&d) && self.lookup_is_outer(d) {
                            st.moved.insert(d, (at, "into a `move` closure"));
                        }
                    }
                }
            }
            ast::Expr::ReturnExpr(r) => {
                if let Some(x) = r.expr() {
                    self.expr(&x, st, Mode::Move("by `return`"));
                }
                st.unreachable = true;
            }
            ast::Expr::BreakExpr(b) => {
                if let Some(x) = b.expr() {
                    self.expr(&x, st, Mode::Move("by `break`"));
                }
                let label = b.lifetime().map(|l| l.text().to_string());
                if let Some(t) = self.target(label.as_deref(), false) {
                    t.breaks.push(st.clone());
                }
                st.unreachable = true;
            }
            ast::Expr::ContinueExpr(c) => {
                let label = c.lifetime().map(|l| l.text().to_string());
                if let Some(t) = self.target(label.as_deref(), true) {
                    t.continues.push(st.clone());
                }
                st.unreachable = true;
            }
            ast::Expr::TryExpr(t) => {
                if let Some(x) = t.expr() {
                    self.expr(&x, st, Mode::Move("by `?`"));
                }
            }
            ast::Expr::AwaitExpr(a) => {
                if let Some(x) = a.expr() {
                    self.expr(&x, st, Mode::Use);
                }
            }
            ast::Expr::LetExpr(le) => {
                if let Some(x) = le.expr() {
                    self.expr(&x, st, Mode::Use);
                }
            }
            other => {
                for x in other.syntax().children().filter_map(ast::Expr::cast) {
                    self.expr(&x, st, Mode::Use);
                }
            }
        }
    }

    /// An assignment to `d`: a second one to an immutable `let x;` is an error.
    fn assign_deferred(&mut self, d: Decl, at: (usize, usize), st: &mut State) {
        let Some(&mutable) = self.deferred.get(&d) else {
            return;
        };
        if st.unreachable {
            return;
        }
        if !mutable && st.assigned.contains(&d) && self.reported.insert((d, at.0)) {
            let n = &self.names[&d];
            self.diags.push(RawDiag {
                start: at.0,
                end: at.1,
                message: format!(
                    "cannot assign twice to immutable variable `{n}` (make it `let mut {n};` to assign it again)"
                ),
                severity: "error",
            });
        }
        st.assigned.insert(d);
        st.unassigned.remove(&d);
    }

    /// An `if` / `while` condition: its parts are reads.
    fn condition(&mut self, c: &ast::Expr, st: &mut State) {
        self.expr(c, st, Mode::Use);
    }

    /// Whether a captured binding was declared outside the closure (still in scope after it).
    fn lookup_is_outer(&self, d: Decl) -> bool {
        self.scopes.iter().any(|s| s.values().any(|x| *x == d))
    }

    /// The loop or labeled block a `break`/`continue` leaves.
    fn target(&mut self, label: Option<&str>, is_continue: bool) -> Option<&mut Target> {
        self.targets.iter_mut().rev().find(|t| match label {
            Some(l) => t.label.as_deref() == Some(l),
            None => t.is_loop,
        } && (t.is_loop || !is_continue))
    }

    /// A loop body: run twice so a move in one iteration is caught by the next. After the loop,
    /// the state is any `break`'s, plus (unless it is a `loop`) the entry and every iteration's end.
    fn looped(
        &mut self,
        st: &mut State,
        label: Option<String>,
        infinite: bool,
        mut body: impl FnMut(&mut Self, &mut State),
    ) {
        let entry = st.clone();
        let mut exit = if infinite {
            State {
                unreachable: true,
                ..State::default()
            }
        } else {
            entry.clone()
        };
        let mut next = entry.clone();
        for _ in 0..2 {
            if next.unreachable {
                break;
            }
            self.targets.push(Target {
                label: label.clone(),
                is_loop: true,
                breaks: vec![],
                continues: vec![],
            });
            let mut s = next.clone();
            body(self, &mut s);
            let t = self.targets.pop().unwrap();
            for c in t.continues {
                s = State::join(s, c);
            }
            for b in t.breaks {
                exit = State::join(exit, b);
            }
            if !infinite {
                exit = State::join(exit, s.clone());
            }
            next = s;
        }
        *st = exit;
    }
}

/// The type name of the `impl` that `f` is a method of.
fn impl_of(f: &ast::Fn) -> Option<String> {
    let imp = f.syntax().ancestors().find_map(ast::Impl::cast)?;
    // A function nested in a method body isn't one of the impl's.
    if f.syntax().parent()?.parent().as_ref() != Some(imp.syntax()) {
        return None;
    }
    crate::adt::impl_parts(&imp).1
}

fn strip(e: &ast::Expr) -> ast::Expr {
    match e {
        ast::Expr::ParenExpr(p) => p.expr().map(|i| strip(&i)).unwrap_or_else(|| e.clone()),
        _ => e.clone(),
    }
}
