//! Borrow-conflict warnings ("NLL-lite"): the conflicts rustc's borrow checker reports most often,
//! found from what the type checker records. A borrow (`let r = &v;`, `let r = v.first();`,
//! `for x in &v`) lasts until its holder's last use; changing `v` while it lasts (or, for a `&mut`
//! borrow, using `v` at all) is reported with rustc's message. In JS such a program still runs, so
//! these are warnings: rustc rejects the program.
//!
//! The check is conservative: only borrows bound by `let` (and borrowing `for` loops) on a local,
//! uses in textual order within one function, and no report when the change and the later use
//! are in different branches of an `if` or `match`.

use ra_ap_syntax::{AstNode, SourceFile, SyntaxNode, TextRange, TextSize, ast};

use crate::typeck::RawDiag;

static STRICT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Report borrow conflicts as errors (the program doesn't compile, as with rustc) instead of
/// warnings.
pub fn set_strict(on: bool) {
    STRICT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// A binding, by its name's absolute range.
pub type Decl = (usize, usize);

/// What the type checker saw of borrows, uses and changes of locals (absolute ranges).
#[derive(Default, Debug)]
pub struct Events {
    /// `let holder = &v;` and the like: (holder, borrowed, mutable, where the borrow starts).
    pub borrows: Vec<(Decl, Decl, bool, usize)>,
    /// `for x in &v { .. }`: (borrowed, mutable, the loop body).
    pub loops: Vec<(Decl, bool, (usize, usize))>,
    /// A read of a local: (local, the path).
    pub uses: Vec<(Decl, (usize, usize))>,
    /// A change of (something in) a local: (local, the place, whether it's an assignment).
    pub changes: Vec<(Decl, (usize, usize), bool)>,
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
    for &(holder, of, mutable, from) in &ev.borrows {
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
            at.0 >= from
                && at.0 < last
                && holder_uses
                    .iter()
                    .any(|u| u.0 > at.0 && !apart(root, base, at, *u))
        };
        for &(d, at, assign) in &ev.changes {
            if d != of || !live(at) {
                continue;
            }
            let msg = match (mutable, assign) {
                (false, true) => format!("cannot assign to `{v}` because it is borrowed"),
                (false, false) => format!(
                    "cannot borrow `{v}` as mutable because it is also borrowed as immutable"
                ),
                (true, _) => format!("cannot borrow `{v}` as mutable more than once at a time"),
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
                    let msg = if is_receiver(root, base, at) {
                        format!(
                            "cannot borrow `{v}` as immutable because it is also borrowed as mutable"
                        )
                    } else {
                        format!("cannot use `{v}` because it was mutably borrowed")
                    };
                    warn(at, msg);
                }
            }
        }
    }
    for &(of, mutable, body) in &ev.loops {
        let v = name(src, base, of);
        let inside = |at: (usize, usize)| body.0 <= at.0 && at.1 <= body.1;
        for &(d, at, assign) in &ev.changes {
            if d == of && inside(at) {
                let msg = match (mutable, assign) {
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
