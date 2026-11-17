//! Borrow-conflict warnings ("NLL-lite"): the conflicts rustc's borrow checker reports most often,
//! found from what the type checker records. A borrow (`let r = &v;`, `let r = v.first();`,
//! `for x in &v`) lasts until its holder's last use; changing `v` while it lasts (or, for a `&mut`
//! borrow, using `v` at all) is reported with rustc's message. In JS such a program still runs, so
//! these are warnings: rustc rejects the program.
//!
//! The check is conservative: only borrows bound by `let` (and borrowing `for` loops) on a local,
//! uses in textual order within one function, and no report when the change and the later use
//! are in different branches of an `if` or `match`.
//!
//! Borrows through calls follow the callee's signature (`ref_sigs`): a function of this file
//! returning a reference (`fn first(v: &Vec<i32>) -> &i32`, `fn name(&self) -> &str`, or a
//! lifetime it names) keeps the arguments the result's lifetime comes from borrowed. Places are
//! tracked down to fields: `&s.a` conflicts with changing `s.a` or `s`, not `s.b`.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, TextRange, TextSize, ast, ast::HasName,
};

use crate::typeck::RawDiag;

static STRICT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Report borrow conflicts as errors (the program doesn't compile, as with rustc) instead of
/// warnings.
pub fn set_strict(on: bool) {
    STRICT.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn strict() -> bool {
    STRICT.load(std::sync::atomic::Ordering::Relaxed)
}

/// A binding, by its name's absolute range.
pub type Decl = (usize, usize);

/// What a borrow borrows.
#[derive(Debug)]
pub struct Place {
    /// The local.
    pub of: Decl,
    pub mutable: bool,
    /// The fields borrowed: `["a"]` for `&v.a`.
    pub path: Vec<String>,
    /// `&v` / `&mut v` of the variable itself. Other borrows (`&v[0]`, `v.first()`) may borrow
    /// what `v` points at when it holds a reference, so assigning `v` doesn't conflict with them
    /// unless `v` is known to own its value.
    pub direct: bool,
}

/// What the type checker saw of borrows, uses and changes of locals (absolute ranges).
#[derive(Default, Debug)]
pub struct Events {
    /// `let holder = &v;` and the like: (holder, what it borrows, where the borrow starts).
    pub borrows: Vec<(Decl, Place, usize)>,
    /// `for x in &v { .. }`: (what the loop borrows, its body).
    pub loops: Vec<(Place, (usize, usize))>,
    /// Locals known to own their value (`let v = vec![..];`), not hold a reference.
    pub owned: std::collections::HashSet<Decl>,
    /// A read of a local: (local, the path).
    pub uses: Vec<(Decl, (usize, usize))>,
    /// A change of (something in) a local: (local, the place, whether it's an assignment).
    pub changes: Vec<(Decl, (usize, usize), bool)>,
    /// The borrows this file's functions return (`ref_sigs`).
    pub sigs: HashMap<String, Option<RefSig>>,
}

/// Which arguments the reference a function returns borrows from.
#[derive(Clone, Debug, PartialEq)]
pub struct RefSig {
    /// The receiver (`&self` / `&mut self`): whether mutably.
    pub from_self: Option<bool>,
    /// Parameters (not counting `self`), by position.
    pub from_params: Vec<usize>,
}

/// Lifetimes a type mentions: elided ones (`&T`, `'_`) and named ones (not `'static`). Types
/// with their own elision scope (`fn(&T) -> &U`, `Fn(&T) -> &U`, `for<'b> ..`) don't count.
fn lifetimes(t: &SyntaxNode) -> (usize, Vec<String>) {
    let (mut elided, mut named) = (0, Vec::new());
    let mut walk = t.preorder();
    while let Some(ev) = walk.next() {
        let ra_ap_syntax::WalkEvent::Enter(n) = ev else {
            continue;
        };
        match n.kind() {
            SyntaxKind::FN_PTR_TYPE
            | SyntaxKind::PARENTHESIZED_ARG_LIST
            | SyntaxKind::PARAM_LIST
            | SyntaxKind::RET_TYPE
            | SyntaxKind::FOR_TYPE
            | SyntaxKind::FOR_BINDER => walk.skip_subtree(),
            SyntaxKind::REF_TYPE => {
                if ast::RefType::cast(n).is_some_and(|r| r.lifetime().is_none()) {
                    elided += 1;
                }
            }
            SyntaxKind::LIFETIME => match n.text().to_string().as_str() {
                "'_" => elided += 1,
                "'static" => {}
                l => named.push(l.to_string()),
            },
            _ => {}
        }
    }
    (elided, named)
}

/// The lifetime of a type's outer reference: `Some(None)` for an elided one (`&T`, `&'_ T`),
/// `Some(Some('a))` for `&'a T`, None when it isn't a reference.
fn outer_lifetime(t: &ast::Type) -> Option<Option<String>> {
    match t {
        ast::Type::RefType(r) => Some(
            r.lifetime()
                .map(|l| l.text().to_string())
                .filter(|l| l != "'_"),
        ),
        ast::Type::ParenType(p) => outer_lifetime(&p.ty()?),
        _ => None,
    }
}

/// The borrow a function's result carries, by rustc's elision rules (None: it borrows nothing
/// this check can follow). Only a parameter's outer reference counts: the result of
/// `fn f<'a>(s: &mut &'a str) -> &'a str` borrows what `s` points at, not the argument.
fn ref_sig(f: &ast::Fn) -> Option<RefSig> {
    if f.async_token().is_some() {
        return None;
    }
    let ret = f.ret_type()?.ty()?;
    let (ret_elided, ret_named) = lifetimes(ret.syntax());
    if ret_elided == 0 && ret_named.is_empty() {
        return None;
    }
    let list = f.param_list()?;
    // The receiver: (its outer reference's lifetime, whether `&mut`, how many lifetimes).
    let recv = list.self_param().map(|s| match s.ty() {
        Some(t) => {
            let mutable = matches!(&t, ast::Type::RefType(r) if r.mut_token().is_some());
            let (e, n) = lifetimes(t.syntax());
            (outer_lifetime(&t), mutable, e + n.len())
        }
        None => (
            s.amp_token().map(|_| {
                s.lifetime()
                    .map(|l| l.text().to_string())
                    .filter(|l| l != "'_")
            }),
            s.amp_token().is_some() && s.mut_token().is_some(),
            usize::from(s.amp_token().is_some()),
        ),
    });
    // Each parameter: (its outer reference's lifetime, how many lifetimes it has).
    let params: Vec<(Option<Option<String>>, usize)> = list
        .params()
        .map(|p| match p.ty() {
            Some(t) => {
                let (e, n) = lifetimes(t.syntax());
                (outer_lifetime(&t), e + n.len())
            }
            None => (None, 0),
        })
        .collect();
    let mut sig = RefSig {
        from_self: None,
        from_params: Vec::new(),
    };
    if ret_elided > 0 {
        match &recv {
            Some((Some(_), mutable, _)) => sig.from_self = Some(*mutable),
            _ => {
                // The one lifetime among the parameters, on a parameter's outer reference.
                let total: usize =
                    params.iter().map(|p| p.1).sum::<usize>() + recv.as_ref().map_or(0, |r| r.2);
                let one = params.iter().position(|p| p.1 == 1 && p.0 == Some(None));
                match one {
                    Some(i) if total == 1 => sig.from_params.push(i),
                    _ => return None,
                }
            }
        }
    }
    for l in &ret_named {
        let named = |o: &Option<Option<String>>| o.as_ref().is_some_and(|x| x.as_ref() == Some(l));
        if let Some((o, mutable, _)) = &recv
            && named(o)
        {
            sig.from_self = Some(*mutable || sig.from_self == Some(true));
        }
        for (i, p) in params.iter().enumerate() {
            if named(&p.0) && !sig.from_params.contains(&i) {
                sig.from_params.push(i);
            }
        }
    }
    (sig.from_self.is_some() || !sig.from_params.is_empty()).then_some(sig)
}

/// The borrows of this file's functions, by name (`first`, or `Type::method` in an `impl`). A
/// name defined more than once gets the shared signature, or nothing when they differ.
pub fn ref_sigs(file: &SourceFile) -> HashMap<String, Option<RefSig>> {
    let mut out: HashMap<String, Option<RefSig>> = HashMap::new();
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(name) = f.name() else { continue };
        let owner = f.syntax().parent().and_then(|p| p.parent());
        let key = match owner {
            Some(o) if ast::Impl::can_cast(o.kind()) => {
                let Some(ast::Type::PathType(t)) = ast::Impl::cast(o).and_then(|i| i.self_ty())
                else {
                    continue;
                };
                let Some(ty) = t
                    .path()
                    .and_then(|p| p.segment())
                    .and_then(|s| s.name_ref())
                else {
                    continue;
                };
                format!("{}::{}", ty.text(), name.text())
            }
            // (A trait's default methods: which type's is unknown.)
            Some(o) if ast::Trait::can_cast(o.kind()) => continue,
            _ => name.text().to_string(),
        };
        let sig = ref_sig(&f);
        match out.get(&key) {
            Some(prev) if *prev != sig => {
                out.insert(key, None);
            }
            _ => {
                out.insert(key, sig);
            }
        }
    }
    out
}

/// The fields a place names from its local: `["a", "b"]` for `s.a.b`, `["a"]` for `s.a[i].b`
/// (an index borrows the whole collection).
pub fn field_path(e: &ast::Expr) -> Vec<String> {
    match e {
        ast::Expr::ParenExpr(p) => p.expr().map(|x| field_path(&x)).unwrap_or_default(),
        ast::Expr::RefExpr(r) => r.expr().map(|x| field_path(&x)).unwrap_or_default(),
        ast::Expr::FieldExpr(f) => {
            let Some(base) = f.expr() else {
                return Vec::new();
            };
            let mut p = field_path(&base);
            if fields_only(&base)
                && let Some(n) = f.name_ref()
            {
                p.push(n.text().to_string());
            }
            p
        }
        ast::Expr::IndexExpr(ix) => ix.base().map(|x| field_path(&x)).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Whether `e` is a local or fields of one (`s`, `s.a.b`).
fn fields_only(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::PathExpr(_) => true,
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|x| fields_only(&x)),
        ast::Expr::FieldExpr(f) => f.expr().is_some_and(|x| fields_only(&x)),
        _ => false,
    }
}

/// Whether two places overlap (one contains the other).
fn overlaps(a: &[String], b: &[String]) -> bool {
    a.iter().zip(b).all(|(x, y)| x == y)
}

/// A use of a local, widened to the fields read (`s.a.b`, not the method in `s.a.len()`): (the
/// range of the place, the fields). Read off the text, so it works inside macro arguments too.
fn use_place(src: &str, base: usize, r: (usize, usize)) -> ((usize, usize), Vec<String>) {
    let text = src.as_bytes();
    let (mut end, mut path) = (r.1, Vec::new());
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    loop {
        let mut i = end.saturating_sub(base);
        while text.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        if text.get(i) != Some(&b'.') || text.get(i + 1) == Some(&b'.') {
            break;
        }
        i += 1;
        while text.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        let start = i;
        while text.get(i).is_some_and(|c| ident(*c)) {
            i += 1;
        }
        let mut j = i;
        while text.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        // A method call (`.len()`, `.iter::<T>()`), `.await`, or not a name.
        let field = &src[start..i];
        if i == start || matches!(text.get(j), Some(b'(') | Some(b':')) || field == "await" {
            break;
        }
        path.push(field.to_string());
        end = i + base;
    }
    ((r.0, end), path)
}

/// Whether the use at `r` is an argument of a formatting macro (`println!("{}", v)`), which
/// borrows it.
fn in_format_macro(root: &SyntaxNode, base: usize, r: (usize, usize)) -> bool {
    let Some(start) = u32::try_from(r.0 - base).ok().map(TextSize::from) else {
        return false;
    };
    let Some(tok) = root.token_at_offset(start).right_biased() else {
        return false;
    };
    tok.parent_ancestors()
        .find_map(ast::MacroCall::cast)
        .and_then(|m| m.path()?.segment()?.name_ref())
        .is_some_and(|n| {
            matches!(
                n.text().to_string().as_str(),
                "print"
                    | "println"
                    | "eprint"
                    | "eprintln"
                    | "format"
                    | "write"
                    | "writeln"
                    | "panic"
                    | "assert_eq"
                    | "assert_ne"
            )
        })
}

/// The place a change names: its fields, and whether it is a plain assignment to them
/// (`s.a = 1`; `s.v.push(1)` and `s.v[0] = 1` borrow `s.v` mutably instead).
fn change_place(root: &SyntaxNode, base: usize, r: (usize, usize)) -> (Vec<String>, bool) {
    let Some(e) = node_at(root, base, r).and_then(ast::Expr::cast) else {
        return (Vec::new(), false);
    };
    let assigned = fields_only(&e)
        && e.syntax()
            .parent()
            .and_then(ast::BinExpr::cast)
            .is_some_and(|b| {
                matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                    && b.lhs().is_some_and(|l| l.syntax() == e.syntax())
            });
    (field_path(&e), assigned)
}

/// Where control goes after the code at `r` (when it can't fall through to what follows).
#[derive(PartialEq)]
enum Exit {
    /// Leaves the function (`return`, `panic!`).
    Return,
    /// Leaves this loop or labeled block (by its range): what follows it still runs.
    Break((usize, usize)),
}

/// The label a loop or block carries (`'outer`).
fn label_of(n: &SyntaxNode) -> Option<String> {
    let l = n.children().find(|c| c.kind() == SyntaxKind::LABEL)?;
    Some(ast::Label::cast(l)?.lifetime()?.text().to_string())
}

/// Whether `n` is a loop (or, with a label, a labeled block).
fn is_loop(n: &SyntaxNode) -> bool {
    matches!(
        n.kind(),
        SyntaxKind::LOOP_EXPR | SyntaxKind::WHILE_EXPR | SyntaxKind::FOR_EXPR
    )
}

/// How `e` leaves, when it always does (`return x`, `break`, `break 'outer`, `panic!(..)`).
fn exit_of(base: usize, e: &SyntaxNode) -> Option<Exit> {
    let range = |n: &SyntaxNode| {
        let r = n.text_range();
        (usize::from(r.start()) + base, usize::from(r.end()) + base)
    };
    match e.kind() {
        SyntaxKind::RETURN_EXPR => Some(Exit::Return),
        SyntaxKind::BREAK_EXPR => {
            let label = ast::BreakExpr::cast(e.clone())
                .and_then(|b| b.lifetime())
                .map(|l| l.text().to_string());
            let target = e
                .ancestors()
                .skip(1)
                .take_while(|a| a.kind() != SyntaxKind::CLOSURE_EXPR && a.kind() != SyntaxKind::FN)
                .find(|a| match &label {
                    Some(l) => label_of(a).as_ref() == Some(l),
                    None => is_loop(a),
                })?;
            Some(Exit::Break(range(&target)))
        }
        SyntaxKind::MACRO_EXPR => {
            let name = ast::MacroExpr::cast(e.clone())?
                .macro_call()?
                .path()?
                .segment()?
                .name_ref()?
                .text()
                .to_string();
            matches!(
                name.as_str(),
                "panic" | "unreachable" | "todo" | "unimplemented"
            )
            .then_some(Exit::Return)
        }
        SyntaxKind::EXPR_STMT => exit_of(base, ast::ExprStmt::cast(e.clone())?.expr()?.syntax()),
        _ => None,
    }
}

/// Where control goes after the code at `r` when it can't reach the code after it in its
/// function: it sits in a `return`/`break`, or a statement after it in the same block (or an
/// enclosing one, within the same loop) always leaves.
fn exit_after(root: &SyntaxNode, base: usize, r: (usize, usize)) -> Option<Exit> {
    let start = u32::try_from(r.0 - base).ok().map(TextSize::from)?;
    let tok = root.token_at_offset(start).right_biased()?;
    for a in tok.parent_ancestors() {
        if matches!(a.kind(), SyntaxKind::RETURN_EXPR | SyntaxKind::BREAK_EXPR) {
            return exit_of(base, &a);
        }
        if is_loop(&a) || matches!(a.kind(), SyntaxKind::CLOSURE_EXPR | SyntaxKind::FN) {
            return None;
        }
        if a.parent()
            .is_some_and(|p| p.kind() == SyntaxKind::STMT_LIST)
        {
            for s in a.siblings(ra_ap_syntax::Direction::Next).skip(1) {
                if let Some(x) = exit_of(base, &s) {
                    return Some(x);
                }
            }
        }
    }
    None
}

/// Whether a change conflicts with a borrow as far as what the local holds goes: assigning the
/// whole local (`v = ..`) only ends a borrow of `v` itself or of a value `v` owns (when `v` holds
/// a reference, `&v[0]` borrows what it points at, which the assignment doesn't touch).
fn replaces(place: &Place, ev: &Events, assign: bool, cpath: &[String]) -> bool {
    !(assign && cpath.is_empty()) || place.direct || ev.owned.contains(&place.of)
}

/// Whether a place goes through a `*` (`(*p).x`, `*r`): it borrows what `p` points at.
pub fn has_deref(e: &ast::Expr) -> bool {
    match e {
        ast::Expr::PrefixExpr(p) if p.op_kind() == Some(ast::UnaryOp::Deref) => true,
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|x| has_deref(&x)),
        ast::Expr::FieldExpr(f) => f.expr().is_some_and(|x| has_deref(&x)),
        ast::Expr::IndexExpr(ix) => ix.base().is_some_and(|x| has_deref(&x)),
        _ => false,
    }
}

/// Whether a `let` initializer makes a new owned value (`vec![..]`, `S { .. }`, `X::new()`,
/// `s.to_string()`), so the local doesn't hold a reference.
pub fn makes_owned(e: &ast::Expr) -> bool {
    let last = |p: Option<ast::Path>| {
        p.and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .unwrap_or_default()
    };
    match e {
        ast::Expr::RecordExpr(_) | ast::Expr::ArrayExpr(_) => true,
        ast::Expr::ParenExpr(p) => p.expr().is_some_and(|x| makes_owned(&x)),
        ast::Expr::MacroExpr(m) => matches!(
            last(m.macro_call().and_then(|c| c.path())).as_str(),
            "vec" | "format"
        ),
        ast::Expr::CallExpr(c) => match c.expr() {
            Some(ast::Expr::PathExpr(p)) => matches!(
                last(p.path()).as_str(),
                "new" | "from" | "with_capacity" | "default" | "from_iter"
            ),
            _ => false,
        },
        ast::Expr::MethodCallExpr(m) => matches!(
            m.name_ref()
                .map(|n| n.text().to_string())
                .unwrap_or_default()
                .as_str(),
            "to_string" | "to_owned" | "to_vec" | "collect"
        ),
        _ => false,
    }
}

/// A place for messages: `s.a`.
fn place_name(v: &str, path: &[String]) -> String {
    std::iter::once(v.to_string())
        .chain(path.iter().cloned())
        .collect::<Vec<_>>()
        .join(".")
}

/// The name a declaration range spells, for messages.
fn name(src: &str, base: usize, d: Decl) -> String {
    src.get(d.0 - base..d.1 - base).unwrap_or("_").to_string()
}

/// Whether `a` and `b` sit in different branches of the same `if` / `match` (so one doesn't run
/// after the other).
fn apart(root: &SyntaxNode, base: usize, a: (usize, usize), b: (usize, usize)) -> bool {
    let node = |r: (usize, usize)| {
        let range = TextRange::new(
            TextSize::from((r.0 - base) as u32),
            TextSize::from((r.1 - base) as u32),
        );
        root.covering_element(range)
    };
    let (na, nb) = (node(a), node(b));
    let anc_a: Vec<SyntaxNode> = na.ancestors().collect();
    let Some(common) = nb.ancestors().find(|n| anc_a.contains(n)) else {
        return false;
    };
    // Different arms of a `match`, or the two sides of an `if`'s `else`.
    let branch_of = |r: (usize, usize)| {
        let n = node(r);
        n.ancestors()
            .take_while(|x| x != &common)
            .last()
            .map(|x| x.text_range())
    };
    let (ba, bb) = (branch_of(a), branch_of(b));
    (ast::MatchArmList::can_cast(common.kind()) || ast::IfExpr::can_cast(common.kind()))
        && ba.is_some()
        && bb.is_some()
        && ba != bb
}

/// The node at an absolute range.
fn node_at(root: &SyntaxNode, base: usize, r: (usize, usize)) -> Option<SyntaxNode> {
    let range = TextRange::new(
        TextSize::from((r.0 - base) as u32),
        TextSize::from((r.1 - base) as u32),
    );
    // (The covering element is the innermost one: climb to the outermost node of that range.)
    let e = root.covering_element(range);
    let start = match e {
        ra_ap_syntax::NodeOrToken::Node(n) => n,
        ra_ap_syntax::NodeOrToken::Token(t) => t.parent()?,
    };
    start
        .ancestors()
        .take_while(|n| n.text_range() == range)
        .last()
}

/// Where rustc points: at `&mut v` rather than `v` for a borrow.
fn widen(root: &SyntaxNode, base: usize, r: (usize, usize)) -> (usize, usize) {
    match node_at(root, base, r)
        .and_then(|n| n.parent())
        .filter(|p| ast::RefExpr::can_cast(p.kind()))
    {
        Some(p) => {
            let pr = p.text_range();
            (usize::from(pr.start()) + base, usize::from(pr.end()) + base)
        }
        None => r,
    }
}

/// Whether the use at `r` is a method call's receiver (`v.len()`): a shared borrow in rustc.
fn is_receiver(root: &SyntaxNode, base: usize, r: (usize, usize)) -> bool {
    node_at(root, base, r)
        .and_then(|n| n.parent())
        .and_then(ast::MethodCallExpr::cast)
        .is_some_and(|m| {
            m.receiver()
                .is_some_and(|x| usize::from(x.syntax().text_range().start()) + base == r.0)
        })
}

pub fn check(file: &SourceFile, src: &str, base: usize, ev: &Events) -> Vec<RawDiag> {
    let root = file.syntax();
    let strict = STRICT.load(std::sync::atomic::Ordering::Relaxed);
    let mut out: Vec<RawDiag> = Vec::new();
    let mut warn = |at: (usize, usize), msg: String| {
        let at = widen(root, base, at);
        if !out.iter().any(|d| d.start == at.0 && d.end == at.1) {
            out.push(RawDiag {
                start: at.0,
                end: at.1,
                message: if strict {
                    msg.clone()
                } else {
                    format!("{msg} (rustc rejects this borrow; here the program still runs)")
                },
                severity: if strict { "error" } else { "warning" },
            });
        }
    };
    for (holder, place, from) in &ev.borrows {
        let (holder, of, mutable, from, bpath) =
            (*holder, place.of, place.mutable, *from, &place.path);
        let holder_uses: Vec<(usize, usize)> = ev
            .uses
            .iter()
            .filter(|(d, _)| *d == holder)
            .map(|(_, r)| *r)
            .collect();
        let Some(last) = holder_uses.iter().map(|r| r.0).max() else {
            continue;
        };
        let v = name(src, base, of);
        let live = |at: (usize, usize)| {
            // After a `break`, only uses after the loop it leaves are reached.
            let after = match exit_after(root, base, at) {
                Some(Exit::Return) => return false,
                Some(Exit::Break(l)) => l.1,
                None => 0,
            };
            at.0 >= from
                && at.0 < last
                && holder_uses
                    .iter()
                    .any(|u| u.0 > at.0 && u.0 >= after && !apart(root, base, at, *u))
        };
        for &(d, at, assign) in &ev.changes {
            if d != of || !live(at) {
                continue;
            }
            let (cpath, assigned) = change_place(root, base, at);
            if !overlaps(bpath, &cpath) || !replaces(place, ev, assign, &cpath) {
                continue;
            }
            let p = place_name(&v, &cpath);
            let msg = match (mutable, assign || assigned) {
                (false, true) => format!("cannot assign to `{p}` because it is borrowed"),
                (false, false) => format!(
                    "cannot borrow `{p}` as mutable because it is also borrowed as immutable"
                ),
                (true, _) => format!("cannot borrow `{p}` as mutable more than once at a time"),
            };
            warn(at, msg);
        }
        if mutable {
            for &(d, at) in &ev.uses {
                if d == of
                    && live(at)
                    && !ev
                        .changes
                        .iter()
                        .any(|c| c.0 == of && c.1.0 <= at.0 && at.1 <= c.1.1)
                {
                    let (outer, upath) = use_place(src, base, at);
                    if !overlaps(bpath, &upath) {
                        continue;
                    }
                    let p = place_name(&v, &upath);
                    let msg = if is_receiver(root, base, outer) || in_format_macro(root, base, at) {
                        format!(
                            "cannot borrow `{p}` as immutable because it is also borrowed as mutable"
                        )
                    } else {
                        format!("cannot use `{p}` because it was mutably borrowed")
                    };
                    warn(at, msg);
                }
            }
        }
    }
    for (place, body) in &ev.loops {
        let (of, mutable, body, bpath) = (place.of, place.mutable, *body, &place.path);
        let v = name(src, base, of);
        // (Leaving the loop right after ends its borrow.)
        let inside = |at: (usize, usize)| {
            body.0 <= at.0
                && at.1 <= body.1
                && match exit_after(root, base, at) {
                    Some(Exit::Return) => false,
                    Some(Exit::Break(l)) => !(l.0 <= body.0 && body.1 <= l.1),
                    None => true,
                }
        };
        for &(d, at, assign) in &ev.changes {
            if d == of && inside(at) {
                let (cpath, assigned) = change_place(root, base, at);
                if !overlaps(bpath, &cpath) || !replaces(place, ev, assign, &cpath) {
                    continue;
                }
                let v = place_name(&v, &cpath);
                let msg = match (mutable, assign || assigned) {
                    (false, true) => {
                        format!("cannot assign to `{v}` because it is borrowed by the `for` loop")
                    }
                    (false, false) => format!(
                        "cannot borrow `{v}` as mutable because it is also borrowed as immutable (by the `for` loop)"
                    ),
                    (true, _) => format!(
                        "cannot borrow `{v}` as mutable more than once at a time (the `for` loop borrows it)"
                    ),
                };
                warn(at, msg);
            }
        }
        if mutable {
            for &(d, at) in &ev.uses {
                if d == of
                    && inside(at)
                    && !ev
                        .changes
                        .iter()
                        .any(|c| c.0 == of && c.1.0 <= at.0 && at.1 <= c.1.1)
                {
                    let (_, upath) = use_place(src, base, at);
                    if !overlaps(bpath, &upath) {
                        continue;
                    }
                    let v = place_name(&v, &upath);
                    warn(
                        at,
                        format!(
                            "cannot use `{v}` because it was mutably borrowed (by the `for` loop)"
                        ),
                    );
                }
            }
        }
    }
    out
}
