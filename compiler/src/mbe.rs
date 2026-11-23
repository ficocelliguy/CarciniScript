//! `macro_rules!`: macro-by-example expansion as a pre-pass over the source text.
//!
//! Each invocation of a macro defined in the file is matched against the macro's rules (token
//! trees with `$name:frag` captures and `$(..) sep op` repetitions), and the chosen rule's
//! transcriber, with the captures substituted, replaces the invocation's text. Expansion repeats
//! until no invocation is left, then the definitions are removed and the result is compiled like
//! any other source. An `OffsetMap` maps offsets in the expanded text back to the original, so
//! diagnostics, panic locations and source maps point at the code the user wrote (code produced
//! by an expansion maps to its invocation).
//!
//! Captured fragments keep the user's own text; `expr` captures are parenthesized, as Rust treats
//! them as one expression. Macro hygiene isn't modeled: a `let` inside an expansion is visible
//! to the code after the invocation.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, NodeOrToken, SourceFile, SyntaxKind, SyntaxNode,
    ast::{self, HasName},
};

/// A span of the expanded text and where it came from.
#[derive(Clone, Debug)]
struct Seg {
    cur: (usize, usize),
    orig: (usize, usize),
    /// Produced by an expansion: every offset in it maps to the invocation.
    synthetic: bool,
}

/// Offsets in the expanded text → offsets in the original source.
#[derive(Clone, Debug)]
pub struct OffsetMap {
    segs: Vec<Seg>,
}

impl OffsetMap {
    pub(crate) fn identity(len: usize) -> Self {
        OffsetMap {
            segs: vec![Seg {
                cur: (0, len),
                orig: (0, len),
                synthetic: false,
            }],
        }
    }

    /// The original offset of `off`; `end` picks the end of an expansion's invocation.
    pub fn orig(&self, off: usize, end: bool) -> usize {
        // (The segments are sorted by `cur` and don't overlap: the first one that reaches past
        // `off`, or is empty at it, holds it unless it starts later.)
        let i = self
            .segs
            .partition_point(|s| !(off < s.cur.1 || (off == s.cur.1 && s.cur.0 == s.cur.1)));
        if let Some(s) = self.segs.get(i)
            && off >= s.cur.0
        {
            return if s.synthetic {
                if end { s.orig.1 } else { s.orig.0 }
            } else {
                s.orig.0 + (off - s.cur.0)
            };
        }
        self.segs
            .last()
            .map(|s| s.orig.1 + off.saturating_sub(s.cur.1))
            .unwrap_or(off)
    }

    /// Whether `off` is in text the compiler wrote: an expansion, or a prelude after the
    /// program (`original_len` is the program's length).
    pub fn generated(&self, off: usize, original_len: usize) -> bool {
        if self
            .segs
            .iter()
            .any(|s| s.synthetic && s.cur.0 <= off && off < s.cur.1)
        {
            return true;
        }
        self.orig(off, false) >= original_len
    }

    /// The expanded offset of an original one, unless it's inside an invocation (or a removed
    /// definition).
    pub fn cur(&self, orig: usize) -> Option<usize> {
        self.segs
            .iter()
            .find(|s| !s.synthetic && s.orig.0 <= orig && orig <= s.orig.1)
            .map(|s| s.cur.0 + (orig - s.orig.0))
    }

    /// `cur`, or for an offset inside a short rewritten span (a flattened path `geo::Point` →
    /// `geo__Point`, an imported variant `Up` → `Dir::Up`), the last byte of what replaced it.
    pub fn cur_near(&self, orig: usize) -> Option<usize> {
        self.cur(orig).or_else(|| {
            self.segs
                .iter()
                .find(|s| {
                    s.synthetic && s.orig.0 <= orig && orig < s.orig.1 && s.cur.1 - s.cur.0 < 64
                })
                .filter(|s| s.cur.1 > s.cur.0)
                .map(|s| s.cur.1 - 1)
        })
    }

    /// Record that `cur[a..b)` was replaced by `len` bytes of new text.
    pub(crate) fn replace(&mut self, a: usize, b: usize, len: usize) {
        let (oa, ob) = (self.orig(a, false), self.orig(b, true));
        let mut out = Vec::new();
        let delta = len as isize - (b - a) as isize;
        let shift = |x: usize| (x as isize + delta) as usize;
        for s in &self.segs {
            if s.cur.1 <= a {
                out.push(s.clone());
            } else if s.cur.0 >= b {
                out.push(Seg {
                    cur: (shift(s.cur.0), shift(s.cur.1)),
                    ..s.clone()
                });
            } else {
                // Overlaps the replaced range: keep the parts outside it.
                if s.cur.0 < a {
                    out.push(Seg {
                        cur: (s.cur.0, a),
                        orig: if s.synthetic {
                            s.orig
                        } else {
                            (s.orig.0, s.orig.0 + (a - s.cur.0))
                        },
                        synthetic: s.synthetic,
                    });
                }
                if s.cur.1 > b {
                    out.push(Seg {
                        cur: (shift(b), shift(s.cur.1)),
                        orig: if s.synthetic {
                            s.orig
                        } else {
                            (s.orig.1 - (s.cur.1 - b), s.orig.1)
                        },
                        synthetic: s.synthetic,
                    });
                }
            }
        }
        out.push(Seg {
            cur: (a, a + len),
            orig: (oa, ob.max(oa)),
            synthetic: true,
        });
        out.sort_by_key(|s| (s.cur.0, s.cur.1));
        self.segs = out;
    }
}

/// The expanded source, or errors at original offsets.
pub struct Expansion {
    pub text: String,
    pub map: OffsetMap,
}

#[derive(Clone, Debug)]
enum Tok {
    Leaf {
        text: String,
        kind: SyntaxKind,
        range: (usize, usize),
        spaced: bool,
    },
    Group {
        open: String,
        close: String,
        toks: Vec<Tok>,
        range: (usize, usize),
        spaced: bool,
    },
}

impl Tok {
    fn range(&self) -> (usize, usize) {
        match self {
            Tok::Leaf { range, .. } | Tok::Group { range, .. } => *range,
        }
    }
    fn text(&self) -> Option<&str> {
        match self {
            Tok::Leaf { text, .. } => Some(text),
            Tok::Group { .. } => None,
        }
    }
}

/// The tokens inside a token tree (without its delimiters), and the delimiters.
fn token_tree(tt: &SyntaxNode) -> (String, String, Vec<Tok>) {
    let items: Vec<NodeOrToken<SyntaxNode, ra_ap_syntax::SyntaxToken>> =
        tt.children_with_tokens().collect();
    let mut toks = Vec::new();
    let mut open = String::new();
    let mut close = String::new();
    let n = items.len();
    for (i, it) in items.iter().enumerate() {
        let spaced = items.get(i + 1).is_some_and(|nx| {
            matches!(
                nx.kind(),
                SyntaxKind::WHITESPACE
                    | SyntaxKind::COMMENT
                    | SyntaxKind::OUTER_DOC_COMMENT
                    | SyntaxKind::INNER_DOC_COMMENT
            )
        });
        match it {
            NodeOrToken::Token(t) => {
                if matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT) {
                    continue;
                }
                // A doc comment is the attribute `#[doc = ".."]` (`#![doc = ".."]`), as in rustc.
                if matches!(
                    t.kind(),
                    SyntaxKind::OUTER_DOC_COMMENT | SyntaxKind::INNER_DOC_COMMENT
                ) {
                    toks.extend(doc_attr(t));
                    continue;
                }
                let is_delim = matches!(
                    t.kind(),
                    SyntaxKind::L_PAREN
                        | SyntaxKind::L_BRACK
                        | SyntaxKind::L_CURLY
                        | SyntaxKind::R_PAREN
                        | SyntaxKind::R_BRACK
                        | SyntaxKind::R_CURLY
                );
                if is_delim && i == 0 {
                    open = t.text().to_string();
                    continue;
                }
                if is_delim && i == n - 1 {
                    close = t.text().to_string();
                    continue;
                }
                let r = t.text_range();
                toks.push(Tok::Leaf {
                    text: t.text().to_string(),
                    kind: t.kind(),
                    range: (r.start().into(), r.end().into()),
                    spaced,
                });
            }
            NodeOrToken::Node(nd) => {
                let (o, c, inner) = token_tree(nd);
                let r = nd.text_range();
                toks.push(Tok::Group {
                    open: o,
                    close: c,
                    toks: inner,
                    range: (r.start().into(), r.end().into()),
                    spaced,
                });
            }
        }
    }
    (open, close, toks)
}

/// The tokens of `#[doc = ".."]` for a doc comment (`#![doc = ".."]` for an inner one). They
/// don't hold the comment's text, so `capture` renders them rather than copying the source.
fn doc_attr(t: &ra_ap_syntax::SyntaxToken) -> Vec<Tok> {
    let text = t.text();
    let body = if let Some(b) = text
        .strip_prefix("/**")
        .or_else(|| text.strip_prefix("/*!"))
    {
        b.strip_suffix("*/").unwrap_or(b)
    } else {
        text.get(3..).unwrap_or("")
    };
    let r = t.text_range();
    let (a, b): (usize, usize) = (r.start().into(), r.end().into());
    let leaf = |text: &str, kind| Tok::Leaf {
        text: text.to_string(),
        kind,
        range: (a, a),
        spaced: true,
    };
    let mut out = vec![Tok::Leaf {
        text: "#".into(),
        kind: SyntaxKind::POUND,
        range: (a, a),
        spaced: false,
    }];
    if t.kind() == SyntaxKind::INNER_DOC_COMMENT {
        out.push(Tok::Leaf {
            text: "!".into(),
            kind: SyntaxKind::BANG,
            range: (a, a),
            spaced: false,
        });
    }
    out.push(Tok::Group {
        open: "[".into(),
        close: "]".into(),
        toks: vec![
            leaf("doc", SyntaxKind::IDENT),
            leaf("=", SyntaxKind::EQ),
            leaf(&format!("{body:?}"), SyntaxKind::STRING),
        ],
        range: (a, b),
        spaced: true,
    });
    out
}

/// Whether `t` stands for source text other than its own (a doc comment's attribute).
fn synthetic(t: &Tok, src: &str) -> bool {
    match t {
        Tok::Leaf { text, range, .. } => src.get(range.0..range.1) != Some(text.as_str()),
        Tok::Group {
            open, range, toks, ..
        } => {
            !src.get(range.0..range.1)
                .is_some_and(|s| s.starts_with(open.as_str()))
                || toks.iter().any(|t| synthetic(t, src))
        }
    }
}

/// The text of `toks` (consecutive tokens of one tree): the source as written, with a doc
/// comment's attribute written out.
fn capture(toks: &[Tok], src: &str) -> String {
    let (Some(first), Some(last)) = (toks.first(), toks.last()) else {
        return String::new();
    };
    if !toks.iter().any(|t| synthetic(t, src)) {
        return src[first.range().0..last.range().1].to_string();
    }
    let mut out = String::new();
    let mut prev: Option<usize> = None;
    for t in toks {
        if let Some(p) = prev {
            match src.get(p..t.range().0) {
                Some(gap) => out.push_str(gap),
                None => out.push(' '),
            }
        }
        match t {
            _ if !synthetic(t, src) => out.push_str(&src[t.range().0..t.range().1]),
            Tok::Leaf { text, .. } => out.push_str(text),
            Tok::Group {
                open, close, toks, ..
            } => {
                out.push_str(open);
                out.push(' ');
                if toks.iter().all(|t| synthetic(t, src)) {
                    let parts: Vec<String> = toks
                        .iter()
                        .map(|t| capture(std::slice::from_ref(t), src))
                        .collect();
                    out.push_str(&parts.join(" "));
                } else {
                    out.push_str(&capture(toks, src));
                }
                out.push(' ');
                out.push_str(close);
            }
        }
        prev = Some(t.range().1);
    }
    out
}

#[derive(Clone, Debug)]
enum Pat {
    Tok(String),
    Group(String, Vec<Pat>),
    Frag(String, String),
    Rep(Vec<Pat>, Option<String>, char),
}

#[derive(Clone, Debug)]
enum Tpl {
    Tok(String, bool),
    Group(String, String, Vec<Tpl>, bool),
    Var(String, bool),
    Rep(Vec<Tpl>, Option<String>, bool),
}

/// `$( .. ) sep? op` after a `$` at `i`: the separator, the operator and the index after them.
fn rep_suffix(toks: &[Tok], i: usize) -> Option<(Option<String>, char, usize)> {
    let t1 = toks.get(i).and_then(Tok::text)?;
    if matches!(t1, "*" | "+" | "?") {
        return Some((None, t1.chars().next()?, i + 1));
    }
    let t2 = toks.get(i + 1).and_then(Tok::text)?;
    if matches!(t2, "*" | "+") {
        return Some((Some(t1.to_string()), t2.chars().next()?, i + 2));
    }
    // A separator of joined punctuation: `$($seg)::+`, `$(..)=>*`.
    let mut sep = t1.to_string();
    let mut j = i;
    while sep.len() < 3 {
        let Some(Tok::Leaf {
            kind,
            spaced: false,
            ..
        }) = toks.get(j)
        else {
            return None;
        };
        let next = toks.get(j + 1).and_then(Tok::text)?;
        if !kind.is_punct() || !next.chars().all(|c| c.is_ascii_punctuation()) {
            return None;
        }
        if matches!(toks.get(j + 2).and_then(Tok::text), Some("*" | "+")) {
            sep.push_str(next);
            return Some((Some(sep), toks[j + 2].text()?.chars().next()?, j + 3));
        }
        sep.push_str(next);
        j += 1;
    }
    None
}

/// The number of tokens at `toks[at..]` that spell the separator `sep` (`::` is two).
fn sep_at(toks: &[Tok], at: usize, sep: &str) -> Option<usize> {
    let mut s = String::new();
    let mut k = 0;
    while s.len() < sep.len() {
        s.push_str(toks.get(at + k).and_then(Tok::text)?);
        k += 1;
    }
    (s == sep).then_some(k)
}

/// A separator as the tokens that end a fragment before it (`::` → `:`, `:`).
fn sep_tokens(sep: &str) -> Vec<String> {
    if sep.len() > 1 && sep.chars().all(|c| c.is_ascii_punctuation()) {
        sep.chars().map(String::from).collect()
    } else {
        vec![sep.to_string()]
    }
}

fn parse_matcher(toks: &[Tok]) -> Result<Vec<Pat>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Leaf { text, .. } if text == "$" => match toks.get(i + 1) {
                Some(Tok::Group {
                    open, toks: inner, ..
                }) if open == "(" => {
                    let inner = parse_matcher(inner)?;
                    let (sep, op, next) = rep_suffix(toks, i + 2)
                        .ok_or("expected `*`, `+` or `?` after `$( .. )`")?;
                    out.push(Pat::Rep(inner, sep, op));
                    i = next;
                }
                Some(Tok::Leaf { text: name, .. }) => {
                    let colon = toks.get(i + 2).and_then(Tok::text);
                    let kind = toks.get(i + 3).and_then(Tok::text);
                    match (colon, kind) {
                        (Some(":"), Some(k)) => {
                            out.push(Pat::Frag(name.clone(), k.to_string()));
                            i += 4;
                        }
                        _ => {
                            return Err(format!(
                                "missing fragment specifier for `${name}` (e.g. `${name}:expr`)"
                            ));
                        }
                    }
                }
                // A `$` with nothing after it is itself (syn's `[$] => { .. }`).
                None => {
                    out.push(Pat::Tok("$".into()));
                    i += 1;
                }
                _ => return Err("expected a name or `(` after `$`".into()),
            },
            Tok::Leaf { text, .. } => {
                out.push(Pat::Tok(text.clone()));
                i += 1;
            }
            Tok::Group {
                open, toks: inner, ..
            } => {
                out.push(Pat::Group(open.clone(), parse_matcher(inner)?));
                i += 1;
            }
        }
    }
    Ok(out)
}

fn parse_transcriber(toks: &[Tok]) -> Result<Vec<Tpl>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Leaf { text, .. } if text == "$" => match toks.get(i + 1) {
                Some(Tok::Group {
                    open,
                    toks: inner,
                    spaced,
                    ..
                }) if open == "(" => {
                    let inner = parse_transcriber(inner)?;
                    let (sep, _, next) = rep_suffix(toks, i + 2)
                        .ok_or("expected `*`, `+` or `?` after `$( .. )`")?;
                    let spaced = toks.get(next - 1).map_or(*spaced, |t| match t {
                        Tok::Leaf { spaced, .. } | Tok::Group { spaced, .. } => *spaced,
                    });
                    out.push(Tpl::Rep(inner, sep, spaced));
                    i = next;
                }
                Some(Tok::Leaf {
                    text: name, spaced, ..
                }) if name == "crate" => {
                    out.push(Tpl::Tok("crate".into(), *spaced));
                    i += 2;
                }
                Some(Tok::Leaf {
                    text: name, spaced, ..
                }) => {
                    out.push(Tpl::Var(name.clone(), *spaced));
                    i += 2;
                }
                None => {
                    out.push(Tpl::Tok("$".into(), false));
                    i += 1;
                }
                _ => return Err("expected a name or `(` after `$`".into()),
            },
            Tok::Leaf { text, spaced, .. } => {
                out.push(Tpl::Tok(text.clone(), *spaced));
                i += 1;
            }
            Tok::Group {
                open,
                close,
                toks: inner,
                spaced,
                ..
            } => {
                out.push(Tpl::Group(
                    open.clone(),
                    close.clone(),
                    parse_transcriber(inner)?,
                    *spaced,
                ));
                i += 1;
            }
        }
    }
    Ok(out)
}

struct Rule {
    matcher: Vec<Pat>,
    transcriber: Vec<Tpl>,
}

/// A captured fragment: its text and kind, or one per repetition.
#[derive(Clone, Debug)]
enum Bound {
    One(String, Frag),
    Seq(Vec<Bound>),
}

/// What a capture needs around it to stay one piece of syntax once it's text again.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Frag {
    /// Parenthesized, as Rust treats it as one expression.
    Expr,
    /// Parenthesized before a `<` (`as $t << 3` isn't `$t<..>`).
    Ty,
    Other,
}

type Bindings = HashMap<String, Bound>;

fn pat_names(pats: &[Pat], out: &mut Vec<String>) {
    for p in pats {
        match p {
            Pat::Frag(n, _) => out.push(n.clone()),
            Pat::Group(_, inner) | Pat::Rep(inner, _, _) => pat_names(inner, out),
            Pat::Tok(_) => {}
        }
    }
}

struct Matcher<'a> {
    src: &'a str,
}

impl Matcher<'_> {
    /// Match `pats` against `toks[pos..]`; `follow` is the literal token(s) that end a greedy
    /// fragment at the end of `pats`. Returns the position after the match.
    fn seq(
        &self,
        pats: &[Pat],
        toks: &[Tok],
        mut pos: usize,
        follow: &[String],
        b: &mut Bindings,
    ) -> Option<usize> {
        for (i, p) in pats.iter().enumerate() {
            match p {
                Pat::Tok(t) => {
                    if toks.get(pos).and_then(Tok::text) != Some(t.as_str()) {
                        return None;
                    }
                    pos += 1;
                }
                Pat::Group(open, inner) => match toks.get(pos) {
                    Some(Tok::Group {
                        open: o, toks: it, ..
                    }) if o == open => {
                        if self.seq(inner, it, 0, &[], b)? != it.len() {
                            return None;
                        }
                        pos += 1;
                    }
                    _ => return None,
                },
                Pat::Frag(name, kind) => {
                    // What ends a greedy fragment: the literal tokens that follow it.
                    let mut stop: Vec<String> = pats[i + 1..]
                        .iter()
                        .map_while(|p| match p {
                            Pat::Tok(t) => Some(t.clone()),
                            _ => None,
                        })
                        .collect();
                    if stop.is_empty() && i + 1 == pats.len() {
                        stop = follow.to_vec();
                    }
                    let end = self.fragment(kind, toks, pos, &stop)?;
                    // (An empty `vis` binds nothing.)
                    let text = capture(&toks[pos..end], self.src);
                    // (A block needs no parentheses, and in statement position mustn't have
                    // them: `$body $i += 1;` with a `{ .. }` body.)
                    let block = end == pos + 1
                        && matches!(&toks[pos], Tok::Group { open, .. } if open == "{");
                    let frag = match kind.as_str() {
                        "expr" | "expr_2021" if !block => Frag::Expr,
                        "ty" => Frag::Ty,
                        _ => Frag::Other,
                    };
                    b.insert(name.clone(), Bound::One(text, frag));
                    pos = end;
                }
                Pat::Rep(inner, sep, op) => {
                    let mut names = Vec::new();
                    pat_names(inner, &mut names);
                    let mut iters: Vec<Bindings> = Vec::new();
                    // A greedy fragment at the end of the repetition stops at the separator.
                    let inner_follow: Vec<String> = match sep {
                        Some(s) => sep_tokens(s),
                        None => pats[i + 1..]
                            .iter()
                            .map_while(|p| match p {
                                Pat::Tok(t) => Some(t.clone()),
                                _ => None,
                            })
                            .collect(),
                    };
                    loop {
                        if pos >= toks.len() || (*op == '?' && !iters.is_empty()) {
                            break;
                        }
                        let mut ib = Bindings::new();
                        let Some(next) = self.seq(inner, toks, pos, &inner_follow, &mut ib) else {
                            break;
                        };
                        if next == pos {
                            break;
                        }
                        iters.push(ib);
                        pos = next;
                        match sep {
                            Some(s) if let Some(k) = sep_at(toks, pos, s) => {
                                // Only consume the separator if another repetition follows.
                                let mut probe = Bindings::new();
                                if self
                                    .seq(inner, toks, pos + k, &inner_follow, &mut probe)
                                    .is_some()
                                {
                                    pos += k;
                                } else {
                                    break;
                                }
                            }
                            Some(_) => break,
                            None => {}
                        }
                    }
                    if *op == '+' && iters.is_empty() {
                        return None;
                    }
                    for n in names {
                        let vals: Vec<Bound> =
                            iters.iter().filter_map(|ib| ib.get(&n).cloned()).collect();
                        b.insert(n, Bound::Seq(vals));
                    }
                }
            }
        }
        Some(pos)
    }

    /// The end of a fragment of `kind` starting at `pos`.
    fn fragment(&self, kind: &str, toks: &[Tok], pos: usize, stop: &[String]) -> Option<usize> {
        if kind == "vis" && pos >= toks.len() {
            return Some(pos);
        }
        let t = toks.get(pos)?;
        // One fragment as rustc parses it (`$($n:expr)*` takes `0 1 2` as three, and
        // `<R<(), ()> as Clone>::clone(&x)` is one `expr`). Falls back to the scan below when the
        // parser doesn't take the tokens.
        // A `pat` the rule follows with `|` (`|$it:pat| ..`, 2018-edition code) has no top-level
        // alternatives, as `pat_param` doesn't.
        let parsed_kind = if kind == "pat" && stop.first().is_some_and(|s| s == "|") {
            "pat_param"
        } else {
            kind
        };
        if let Some(end) = parsed_end(parsed_kind, toks, pos) {
            return Some(end);
        }
        match kind {
            "ident" => match t {
                Tok::Leaf { text, .. }
                    if text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_') =>
                {
                    Some(pos + 1)
                }
                _ => None,
            },
            "lifetime" => match t {
                Tok::Leaf { text, .. } if text.starts_with('\'') => Some(pos + 1),
                _ => None,
            },
            "literal" => match t {
                Tok::Leaf { text, .. } if text == "-" => match toks.get(pos + 1) {
                    Some(Tok::Leaf { kind, .. }) if is_literal(*kind) => Some(pos + 2),
                    _ => None,
                },
                Tok::Leaf { kind, text, .. }
                    if is_literal(*kind) || text == "true" || text == "false" =>
                {
                    Some(pos + 1)
                }
                _ => None,
            },
            // One token tree, where joined punctuation is one token (`=>`, `::`, `..=`).
            "tt" => Some(pos + joint_punct_len(toks, pos)),
            // `pub`, `pub(crate)`, .., or nothing at all.
            "vis" => match t {
                Tok::Leaf { text, .. } if text == "pub" => match toks.get(pos + 1) {
                    Some(Tok::Group { open, .. }) if open == "(" => Some(pos + 2),
                    _ => Some(pos + 1),
                },
                _ => Some(pos),
            },
            "block" => match t {
                Tok::Group { open, .. } if open == "{" => Some(pos + 1),
                _ => None,
            },
            // `expr`, `ty`, `pat`, `path`, `stmt`, `item`, `meta`, ..: everything up to the
            // literal tokens that follow the fragment in the rule (or the end).
            // One type, as rustc parses it: `$($t:ty)*` takes `u8 Vec<u8> &str` as three.
            "ty" if let Some(end) = ty_end(toks, pos, stop) => Some(end),
            "item" if let Some(end) = item_end(toks, pos) => Some(end),
            // A token none of them can start with (`@internal`, `=>`, `,`): no match, as with
            // rustc, so a tt-muncher's `(@internal, ..)` rule isn't taken by an `$e:expr` one.
            _ if !can_begin_fragment(kind, toks, pos) => None,
            _ => {
                // Like rustc's fragment parsers: an `expr` never holds a top-level `,`, `;` or
                // `=>`; a type or path holds commas only between `<` and `>`.
                let generic = matches!(kind, "ty" | "path");
                let mut angle = 0i32;
                let mut end = pos;
                while end < toks.len() {
                    // (A `,` between `<` and `>` is the type's own: `size_of::<R<u8, E>>()`.)
                    if angle == 0 && !stop.is_empty() && starts_with(toks, end, stop) {
                        break;
                    }
                    let t = toks[end].text();
                    // A closure's parameters hold commas too: `|a, b| a + b`.
                    if t == Some("|")
                        && kind == "expr"
                        && (end == pos || (end == pos + 1 && toks[pos].text() == Some("move")))
                        && let Some(close) =
                            (end + 1..toks.len()).find(|&k| toks[k].text() == Some("|"))
                    {
                        end = close + 1;
                        continue;
                    }
                    // An expression's turbofish (`parse::<Map<K, V>>()`) holds commas too.
                    let prev = |k: usize| end.checked_sub(k).and_then(|j| toks[j].text());
                    let turbofish =
                        prev(1) == Some("::") || (prev(1) == Some(":") && prev(2) == Some(":"));
                    if generic || angle > 0 || (t == Some("<") && turbofish) {
                        match t {
                            Some("<") => angle += 1,
                            Some(">") if angle > 0 && !matches!(prev(1), Some("-") | Some("=")) => {
                                angle -= 1
                            }
                            _ => {}
                        }
                    }
                    let fat_arrow =
                        t == Some("=") && toks.get(end + 1).and_then(Tok::text) == Some(">");
                    if angle == 0 && (matches!(t, Some(",") | Some(";") | Some("=>")) || fat_arrow)
                    {
                        break;
                    }
                    end += 1;
                }
                (end > pos).then_some(end)
            }
        }
    }
}

/// Whether `toks[pos]` can start an `expr`, `ty`, `pat`, `path`, `stmt`, `item` or `meta`
/// fragment: a punctuation token like `@`, `,`, `=>` or `+` can't (`::`, `..`, `&&`, `||` and
/// `<<` can, and `?` starts a type: `?Sized`). (A lone `$` is let through: macros that write
/// macros pass theirs along.)
fn can_begin_fragment(kind: &str, toks: &[Tok], pos: usize) -> bool {
    let Some(Tok::Leaf { text, kind: tk, .. }) = toks.get(pos) else {
        return true;
    };
    if !tk.is_punct() {
        return true;
    }
    let n = joint_punct_len(toks, pos);
    let op: String = toks[pos..pos + n].iter().filter_map(Tok::text).collect();
    if matches!(
        op.as_str(),
        "::" | ".." | "..=" | "..." | "&&" | "||" | "<<"
    ) {
        return true;
    }
    match text.as_str() {
        "@" | "," | ";" | "=" | ">" | "+" | "/" | "%" | "^" | "~" | ":" | "." => false,
        "?" => kind == "ty",
        _ => true,
    }
}

/// How many tokens at `toks[pos..]` make one of Rust's multi-character operators, written
/// together (`=>` is one `tt` to rustc; `= >` is two); 1 for anything else.
fn joint_punct_len(toks: &[Tok], pos: usize) -> usize {
    const OPS: [&str; 24] = [
        "<<=", ">>=", "...", "..=", "::", "->", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=",
        "-=", "*=", "/=", "%=", "^=", "&=", "|=", "<<", ">>", "..",
    ];
    let mut s = String::new();
    let mut k = pos;
    let mut lens = Vec::new();
    while let Some(Tok::Leaf {
        text, kind, spaced, ..
    }) = toks.get(k)
    {
        if !kind.is_punct() || s.len() >= 3 {
            break;
        }
        s.push_str(text);
        k += 1;
        lens.push((s.clone(), k - pos));
        if *spaced {
            break;
        }
    }
    lens.iter()
        .rev()
        .find(|(op, _)| OPS.contains(&op.as_str()))
        .map_or(1, |&(_, n)| n)
}

/// The end of the `kind` fragment at `toks[pos..]` by rust-analyzer's parser (the same prefix
/// parse rustc's macro matcher does), or `None` when it isn't a parsed kind, the parser reports
/// an error, or it stops inside a group.
fn parsed_end(kind: &str, toks: &[Tok], pos: usize) -> Option<usize> {
    use ra_ap_parser::{Input, PrefixEntryPoint, Step};
    let entry = match kind {
        "expr" | "expr_2021" => PrefixEntryPoint::Expr,
        "ty" => PrefixEntryPoint::Ty,
        "pat" => PrefixEntryPoint::PatTop,
        "pat_param" => PrefixEntryPoint::Pat,
        "path" => PrefixEntryPoint::Path,
        "stmt" => PrefixEntryPoint::Stmt,
        "item" => PrefixEntryPoint::Item,
        "meta" => PrefixEntryPoint::MetaItem,
        _ => return None,
    };
    fn delim(s: &str) -> Option<SyntaxKind> {
        Some(match s {
            "(" => SyntaxKind::L_PAREN,
            ")" => SyntaxKind::R_PAREN,
            "[" => SyntaxKind::L_BRACK,
            "]" => SyntaxKind::R_BRACK,
            "{" => SyntaxKind::L_CURLY,
            "}" => SyntaxKind::R_CURLY,
            _ => return None,
        })
    }
    // Pushes `toks`, flattened; `false` for a token the parser can't be given.
    fn push(input: &mut Input, toks: &[Tok]) -> bool {
        let ed = crate::macros::EDITION;
        for (i, t) in toks.iter().enumerate() {
            match t {
                Tok::Leaf {
                    text, kind, spaced, ..
                } => {
                    if *kind == SyntaxKind::IDENT {
                        let ctx = SyntaxKind::from_contextual_keyword(text, ed)
                            .unwrap_or(SyntaxKind::IDENT);
                        input.push_ident(ctx, ed);
                    } else {
                        input.push(*kind, ed);
                    }
                    // `:` `:` is `::` only when written together.
                    let next_punct = matches!(
                        toks.get(i + 1),
                        Some(Tok::Leaf { kind: k, .. }) if k.is_punct()
                    );
                    if kind.is_punct() && !spaced && next_punct {
                        input.was_joint();
                    }
                }
                Tok::Group {
                    open, close, toks, ..
                } => {
                    let (Some(o), Some(c)) = (delim(open), delim(close)) else {
                        return false;
                    };
                    input.push(o, ed);
                    if !push(input, toks) {
                        return false;
                    }
                    input.push(c, ed);
                }
            }
        }
        true
    }
    // The flattened length at the end of each token from `pos`.
    fn flat_len(t: &Tok) -> usize {
        match t {
            Tok::Leaf { .. } => 1,
            Tok::Group { toks, .. } => 2 + toks.iter().map(flat_len).sum::<usize>(),
        }
    }
    // No fragment runs past a top-level `;` (an item may end with one), so the parser isn't
    // handed the rest of a long invocation each time.
    let to = toks[pos..]
        .iter()
        .position(|t| t.text() == Some(";"))
        .map_or(toks.len(), |k| pos + k + 1);
    let toks = &toks[..to];
    let mut input = Input::with_capacity(toks.len() - pos);
    if !push(&mut input, &toks[pos..]) {
        return None;
    }
    let ends: Vec<usize> = toks[pos..]
        .iter()
        .scan(0, |n, t| {
            *n += flat_len(t);
            Some(*n)
        })
        .collect();
    let out = entry.parse(&input);
    let mut n = 0usize;
    for step in out.iter() {
        match step {
            Step::Token { n_input_tokens, .. } => n += n_input_tokens as usize,
            Step::FloatSplit { .. } => n += 1,
            Step::Error { .. } => return None,
            Step::Enter { .. } | Step::Exit => {}
        }
    }
    if n == 0 {
        return None;
    }
    ends.iter().position(|&e| e == n).map(|k| pos + k + 1)
}

fn starts_with(toks: &[Tok], at: usize, seq: &[String]) -> bool {
    // (A `:` doesn't stop at half of a `::`: `$t:ty : $n:ident` takes `core::num::X: Y`.)
    let colon = |k: usize| toks.get(k).and_then(Tok::text) == Some(":");
    let joint = |k: usize| matches!(toks.get(k), Some(Tok::Leaf { spaced: false, .. }));
    let n = seq.len();
    seq.iter()
        .enumerate()
        .all(|(k, s)| toks.get(at + k).and_then(Tok::text) == Some(s.as_str()))
        && !(seq.first().is_some_and(|s| s == ":") && at > 0 && colon(at - 1) && joint(at - 1))
        && !(seq.last().is_some_and(|s| s == ":") && colon(at + n) && joint(at + n - 1))
}

/// The end of the one type at `toks[pos..]`: it ends at a token that can't continue it (`u8 &str`
/// is two types, `Vec<u8>`, `Fn(u8) -> u8`, `dyn A + B` and `<T as Tr>::X` are one each). `None`
/// when it isn't a type this recognizes (the caller then takes what comes before `stop`).
fn ty_end(toks: &[Tok], pos: usize, stop: &[String]) -> Option<usize> {
    #[derive(PartialEq)]
    enum Last {
        Ident,
        Call,
        Other,
    }
    let text = |k: usize| toks.get(k).and_then(Tok::text);
    let joint = |k: usize| matches!(toks.get(k), Some(Tok::Leaf { spaced: false, .. }));
    let mut end = pos;
    // Whether the type needs more tokens (after `&`, `::`, `dyn`, `->`, ..).
    let mut need = true;
    let mut last = Last::Other;
    // After `dyn` / `impl`, `+ Send` continues the type.
    let mut bounds = false;
    let mut angle = 0i32;
    // `for<'a>` is followed by the rest of the type.
    let mut hrtb = false;
    while end < toks.len() {
        let t = text(end);
        if angle > 0 {
            match t {
                Some("<") => angle += 1,
                Some(">") if !matches!(text(end - 1), Some("-" | "=")) => {
                    angle -= 1;
                    if angle == 0 {
                        need = hrtb;
                        hrtb = false;
                        last = Last::Other;
                    }
                }
                _ => {}
            }
            end += 1;
            continue;
        }
        if need {
            match (&toks[end], t) {
                (Tok::Group { .. }, _) => {
                    last = if end > pos && text(end - 1) == Some("fn") {
                        Last::Call
                    } else {
                        Last::Other
                    };
                    need = false;
                }
                (_, Some(":")) if text(end + 1) == Some(":") && joint(end) => end += 1,
                (_, Some("<")) => {
                    hrtb = end > pos && text(end - 1) == Some("for");
                    angle = 1;
                }
                (_, Some("dyn" | "impl")) => bounds = true,
                (
                    _,
                    Some("&" | "*" | "mut" | "const" | "unsafe" | "extern" | "fn" | "for" | "?"),
                ) => {}
                (_, Some(l)) if l.starts_with('\'') => {
                    if !(end > pos && text(end - 1) == Some("&")) {
                        need = false;
                        last = Last::Other;
                    }
                }
                (Tok::Leaf { kind, .. }, _) if *kind == SyntaxKind::STRING => {}
                (_, Some("!")) => {
                    need = false;
                    last = Last::Other;
                }
                (_, Some(x)) if x.starts_with(|c: char| c.is_alphabetic() || c == '_') => {
                    need = false;
                    last = Last::Ident;
                }
                _ => return None,
            }
            end += 1;
            continue;
        }
        if !stop.is_empty() && starts_with(toks, end, stop) {
            break;
        }
        match (&toks[end], t) {
            (_, Some(":")) if text(end + 1) == Some(":") && joint(end) => {
                end += 2;
                need = true;
            }
            (_, Some("<")) if last == Last::Ident => {
                angle = 1;
                end += 1;
            }
            (_, Some("!"))
                if last == Last::Ident && matches!(toks.get(end + 1), Some(Tok::Group { .. })) =>
            {
                end += 2;
                last = Last::Other;
            }
            (Tok::Group { open, .. }, _) if last == Last::Ident && open == "(" => {
                end += 1;
                last = Last::Call;
            }
            (_, Some("-")) if last == Last::Call && text(end + 1) == Some(">") => {
                end += 2;
                need = true;
            }
            (_, Some("+")) if bounds => {
                end += 1;
                need = true;
            }
            _ => break,
        }
    }
    (!need && angle == 0 && end > pos).then_some(end)
}

/// The end of the item at `toks[pos..]`: after its `;`, or its `{ .. }` body (`impl<A, B> ..`
/// holds commas outside any group).
fn item_end(toks: &[Tok], pos: usize) -> Option<usize> {
    let text = |k: usize| toks.get(k).and_then(Tok::text);
    let mut k = pos;
    // Attributes and visibility.
    while text(k) == Some("#") && matches!(toks.get(k + 1), Some(Tok::Group { .. })) {
        k += 2;
    }
    if text(k) == Some("pub") {
        k += 1;
        if matches!(toks.get(k), Some(Tok::Group { open, .. }) if open == "(") {
            k += 1;
        }
    }
    // These end at their `;` (their value may hold a `{ .. }`).
    let semi = match text(k) {
        Some("static" | "type" | "use") => true,
        Some("const") => !matches!(text(k + 1), Some("fn" | "unsafe" | "async" | "extern")),
        Some("extern") => text(k + 1) == Some("crate"),
        _ => false,
    };
    for (e, t) in toks.iter().enumerate().skip(k) {
        match t {
            Tok::Leaf { text, .. } if text == ";" => return Some(e + 1),
            Tok::Group { open, .. } if open == "{" && !semi => return Some(e + 1),
            _ => {}
        }
    }
    (toks.len() > pos).then_some(toks.len())
}

fn is_literal(k: SyntaxKind) -> bool {
    matches!(
        k,
        SyntaxKind::INT_NUMBER
            | SyntaxKind::FLOAT_NUMBER
            | SyntaxKind::STRING
            | SyntaxKind::CHAR
            | SyntaxKind::BYTE
            | SyntaxKind::BYTE_STRING
            | SyntaxKind::C_STRING
            | SyntaxKind::TRUE_KW
            | SyntaxKind::FALSE_KW
    )
}

fn tpl_names(t: &[Tpl], out: &mut Vec<String>) {
    for x in t {
        match x {
            Tpl::Var(n, _) => out.push(n.clone()),
            Tpl::Group(_, _, inner, _) | Tpl::Rep(inner, _, _) => tpl_names(inner, out),
            Tpl::Tok(..) => {}
        }
    }
}

/// Hygiene for the names a macro's own body binds (`let x`, `let (a, b)`, `for x in`,
/// `|x|`): each expansion renames them (`x__h3`), so they neither capture nor shadow the
/// caller's variables. Names that come in through `$fragments` are the caller's and stay.
fn hygienic(t: &[Tpl], n: usize) -> Vec<Tpl> {
    fn is_ident(s: &str) -> bool {
        s.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !matches!(
                s,
                "mut"
                    | "ref"
                    | "in"
                    | "let"
                    | "for"
                    | "self"
                    | "Self"
                    | "true"
                    | "false"
                    | "_"
                    | "move"
            )
    }
    fn text(t: &Tpl) -> Option<&str> {
        match t {
            Tpl::Tok(s, _) => Some(s.as_str()),
            _ => None,
        }
    }
    // The names bound in `t` (and its groups and repetitions).
    fn bound(t: &[Tpl], out: &mut Vec<String>) {
        let mut i = 0;
        while i < t.len() {
            match &t[i] {
                Tpl::Group(o, _, inner, _) => {
                    // `|a, b|` parses as tokens; a closure's group body is searched like others.
                    bound(inner, out);
                    let _ = o;
                }
                Tpl::Rep(inner, _, _) => bound(inner, out),
                Tpl::Tok(k, _) if k == "let" || k == "for" => {
                    let mut j = i + 1;
                    if t.get(j).and_then(text) == Some("mut") {
                        j += 1;
                    }
                    // (A loop is `for pat in ..`; `impl Tr for Vec<T>` and `for<'a>` bind
                    // nothing.)
                    if k == "for" && t.get(j + 1).and_then(text) != Some("in") {
                        i += 1;
                        continue;
                    }
                    match t.get(j) {
                        // (Not a pattern's constructor: `if let Some(x) = ..`, `let Point { .. }`.)
                        Some(Tpl::Tok(name, _))
                            if is_ident(name)
                                && !matches!(t.get(j + 1), Some(Tpl::Group(..)))
                                && !(t.get(j + 1).and_then(text) == Some(":")
                                    && t.get(j + 2).and_then(text) == Some(":")) =>
                        {
                            out.push(name.clone())
                        }
                        // `let (a, b) = ..`
                        Some(Tpl::Group(o, _, inner, _)) if o == "(" => {
                            for x in inner {
                                if let Tpl::Tok(name, _) = x
                                    && is_ident(name)
                                {
                                    out.push(name.clone());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // `|x|` / `|a, b|`
                Tpl::Tok(k, _) if k == "|" => {
                    let mut j = i + 1;
                    let mut names = Vec::new();
                    while let Some(Tpl::Tok(name, _)) = t.get(j) {
                        if is_ident(name) {
                            names.push(name.clone());
                            j += 1;
                            if t.get(j).and_then(text) == Some(",") {
                                j += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    if t.get(j).and_then(text) == Some("|") {
                        out.extend(names);
                        i = j;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
    fn rename(t: &[Tpl], names: &[String], n: usize, braces: bool) -> Vec<Tpl> {
        let mut out = Vec::with_capacity(t.len());
        for (i, x) in t.iter().enumerate() {
            let prev = i.checked_sub(1).and_then(|j| text(&t[j]));
            let next = t.get(i + 1).and_then(text);
            out.push(match x {
                // (Not a field: `.x`, `Point { x: 1 }`, `a::x`; nor a rule's marker:
                // `closures!(@closure ..)`.)
                Tpl::Tok(s, sp)
                    if names.contains(s)
                        && !matches!(prev, Some("." | "::" | "@"))
                        && !(braces && next == Some(":") && matches!(prev, None | Some(",")))
                        && !(prev == Some(":") && i >= 2 && text(&t[i - 2]) == Some(":")) =>
                {
                    Tpl::Tok(format!("{s}__h{n}"), *sp)
                }
                Tpl::Group(o, c, inner, sp) => {
                    Tpl::Group(o.clone(), c.clone(), rename(inner, names, n, o == "{"), *sp)
                }
                Tpl::Rep(inner, sep, op) => {
                    Tpl::Rep(rename(inner, names, n, braces), sep.clone(), *op)
                }
                other => other.clone(),
            });
        }
        out
    }
    let mut names = Vec::new();
    bound(t, &mut names);
    if names.is_empty() {
        return t.to_vec();
    }
    rename(t, &names, n, false)
}

fn transcribe(t: &[Tpl], b: &Bindings, out: &mut String) -> Result<(), String> {
    for (i, x) in t.iter().enumerate() {
        match x {
            Tpl::Tok(s, spaced) => {
                out.push_str(s);
                if *spaced {
                    out.push(' ');
                }
            }
            // `stringify!($e)` of an expression: its text as written, without the parentheses
            // that keep its precedence elsewhere.
            Tpl::Group(o, c, inner, spaced)
                if out.trim_end().ends_with("stringify!")
                    && matches!(inner.as_slice(), [Tpl::Var(n, _)]
                        if matches!(b.get(n), Some(Bound::One(_, Frag::Expr)))) =>
            {
                let [Tpl::Var(n, _)] = inner.as_slice() else {
                    unreachable!()
                };
                if let Some(Bound::One(text, _)) = b.get(n) {
                    out.push_str(o);
                    out.push_str(text);
                    out.push_str(c);
                }
                if *spaced {
                    out.push(' ');
                }
            }
            Tpl::Group(o, c, inner, spaced) => {
                out.push_str(o);
                transcribe(inner, b, out)?;
                out.push_str(c);
                if *spaced {
                    out.push(' ');
                }
            }
            Tpl::Var(n, spaced) => {
                match b.get(n) {
                    Some(Bound::One(text, frag)) => {
                        let angle = matches!(t.get(i + 1), Some(Tpl::Tok(s, _)) if s == "<");
                        if *frag == Frag::Expr || (*frag == Frag::Ty && angle) {
                            out.push('(');
                            out.push_str(text);
                            out.push(')');
                        } else {
                            out.push_str(text);
                        }
                    }
                    Some(Bound::Seq(_)) => {
                        return Err(format!(
                            "variable `{n}` is still repeating at this depth (use it inside `$( .. )*`)"
                        ));
                    }
                    // Not a capture: `$x` stays as written.
                    None => {
                        out.push('$');
                        out.push_str(n);
                    }
                }
                if *spaced {
                    out.push(' ');
                }
            }
            Tpl::Rep(inner, sep, spaced) => {
                let mut names = Vec::new();
                tpl_names(inner, &mut names);
                let lens: Vec<usize> = names
                    .iter()
                    .filter_map(|n| match b.get(n) {
                        Some(Bound::Seq(v)) => Some(v.len()),
                        _ => None,
                    })
                    .collect();
                let Some(&count) = lens.first() else {
                    return Err("attempted to repeat an expression containing no syntax variables matched as repeating at this depth".into());
                };
                if lens.iter().any(|&l| l != count) {
                    return Err("meta-variables repeat with mismatched counts".into());
                }
                for k in 0..count {
                    let mut ib = b.clone();
                    for n in &names {
                        if let Some(Bound::Seq(v)) = b.get(n) {
                            ib.insert(n.clone(), v[k].clone());
                        }
                    }
                    if k > 0
                        && let Some(s) = sep
                    {
                        out.push_str(s);
                        out.push(' ');
                    }
                    let at = out.len();
                    transcribe(inner, &ib, out)?;
                    // Repetitions are separate tokens: `$($a : $b)*` → `a : b c : d`, not `bc`,
                    // and `"x" as` isn't the literal `"x"as` with a suffix.
                    // (Punctuation stays joined: `$($t)*` of `::` is `::`.)
                    let word = |c: char| c.is_alphanumeric() || c == '_';
                    // Nor is `x #y` the reserved prefix `x#`, or `r "s"` the raw string `r"s"`.
                    let lit_end = |c: char| word(c) || matches!(c, '"' | '\'');
                    let lit_start = |c: char| word(c) || matches!(c, '"' | '\'' | '#');
                    if k > 0
                        && sep.is_none()
                        && ((out[..at].ends_with(lit_end) && out[at..].starts_with(word))
                            || (out[..at].ends_with(word) && out[at..].starts_with(lit_start)))
                    {
                        out.insert(at, ' ');
                    }
                }
                if *spaced {
                    out.push(' ');
                }
            }
        }
    }
    Ok(())
}

/// The rules of a `macro_rules! name { (..) => { .. }; .. }` definition.
fn rules_of(def: &ast::MacroRules) -> Result<Vec<Rule>, String> {
    let tt = def.token_tree().ok_or("expected the macro's rules")?;
    let (_, _, toks) = token_tree(tt.syntax());
    let mut rules = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let Some(Tok::Group { toks: m, .. }) = toks.get(i) else {
            return Err("expected a rule like `($x:expr) => { .. }`".into());
        };
        // `=>` may be one token or two.
        let mut j = i + 1;
        match toks.get(j).and_then(Tok::text) {
            Some("=>") => j += 1,
            Some("=") if toks.get(j + 1).and_then(Tok::text) == Some(">") => j += 2,
            _ => return Err("expected `=>` after a macro matcher".into()),
        }
        let Some(Tok::Group { toks: t, .. }) = toks.get(j) else {
            return Err("expected the macro's expansion after `=>`".into());
        };
        rules.push(Rule {
            matcher: parse_matcher(m)?,
            transcriber: parse_transcriber(t)?,
        });
        i = j + 1;
        if toks.get(i).and_then(Tok::text) == Some(";") {
            i += 1;
        }
    }
    Ok(rules)
}

/// The bundled crates' macros a file uses, as `macro_rules!` (`anyhow!`, `bail!`, `ensure!`).
fn bundled_macros(src: &str) -> String {
    let mut out = String::new();
    if src.contains("anyhow") {
        out.push_str(ANYHOW_MACROS);
    }
    if uses_log(src) {
        out.push_str(&own_macros_left_out(LOG_MACROS, src));
    }
    if src.contains("tokio::") || src.contains("use tokio") {
        out.push_str(&own_macros_left_out(TOKIO_MACROS, src));
    }
    if src.contains("izip!") || src.contains("iproduct!") {
        out.push_str(&own_macros_left_out(ITERTOOLS_MACROS, src));
    }
    // `std::pin::pin!(fut)` / `futures::pin_mut!(fut)`: pinning is nothing in JS.
    if src.contains("pin!") || src.contains("pin_mut!") {
        out.push_str(&own_macros_left_out(PIN_MACROS, src));
    }
    out
}

const PIN_MACROS: &str = r#"
macro_rules! pin {
    ($e:expr $(,)?) => { $e };
}
macro_rules! pin_mut {
    ($($x:ident),* $(,)?) => {};
}
"#;

/// `macros` without the ones `src` defines itself (`macro_rules! debug { .. }` of its own).
fn own_macros_left_out(macros: &str, src: &str) -> String {
    let mut out = String::new();
    for block in macros.split("macro_rules! ").skip(1) {
        let name: String = block
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        // (`macro_rules! log_enabled` doesn't define `log`.)
        let pat = format!("macro_rules! {name}");
        let defined = src.match_indices(&pat).any(|(k, _)| {
            !src[k + pat.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
        });
        if !defined {
            out.push_str("macro_rules! ");
            out.push_str(block);
        }
    }
    out
}

/// itertools' `izip!(a, b, ..)` (items zipped into flat tuples) and `iproduct!(a, b, ..)` (the
/// cartesian product, as flat tuples; one argument gives 1-tuples, none `once(())`), up to six
/// iterables.
const ITERTOOLS_MACROS: &str = r#"
macro_rules! izip {
    ($a:expr $(,)?) => { ($a).into_iter() };
    ($a:expr, $b:expr $(,)?) => { ($a).into_iter().zip($b) };
    ($a:expr, $b:expr, $c:expr $(,)?) => {
        ($a).into_iter().zip($b).zip($c).map(|((a, b), c)| (a, b, c))
    };
    ($a:expr, $b:expr, $c:expr, $d:expr $(,)?) => {
        ($a).into_iter().zip($b).zip($c).zip($d).map(|(((a, b), c), d)| (a, b, c, d))
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr $(,)?) => {
        ($a).into_iter().zip($b).zip($c).zip($d).zip($e)
            .map(|((((a, b), c), d), e)| (a, b, c, d, e))
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr $(,)?) => {
        ($a).into_iter().zip($b).zip($c).zip($d).zip($e).zip($f)
            .map(|(((((a, b), c), d), e), f)| (a, b, c, d, e, f))
    };
}
macro_rules! iproduct {
    () => { std::iter::once(()) };
    ($a:expr $(,)?) => { ($a).into_iter().map(|a| (a,)) };
    ($a:expr, $b:expr $(,)?) => { ($a).into_iter().cartesian_product($b) };
    ($a:expr, $b:expr, $c:expr $(,)?) => {
        ($a).into_iter().cartesian_product($b).cartesian_product($c).map(|((a, b), c)| (a, b, c))
    };
    ($a:expr, $b:expr, $c:expr, $d:expr $(,)?) => {
        ($a).into_iter().cartesian_product($b).cartesian_product($c).cartesian_product($d)
            .map(|(((a, b), c), d)| (a, b, c, d))
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr $(,)?) => {
        ($a).into_iter().cartesian_product($b).cartesian_product($c).cartesian_product($d)
            .cartesian_product($e)
            .map(|((((a, b), c), d), e)| (a, b, c, d, e))
    };
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $f:expr $(,)?) => {
        ($a).into_iter().cartesian_product($b).cartesian_product($c).cartesian_product($d)
            .cartesian_product($e).cartesian_product($f)
            .map(|(((((a, b), c), d), e), f)| (a, b, c, d, e, f))
    };
}
"#;

/// Whether `src` uses the bundled `log` (or sets up a logger).
pub(crate) fn uses_log(src: &str) -> bool {
    src.contains("log::")
        || src.contains("use log")
        || src.contains("env_logger")
        || src.contains("simple_logger")
}

/// The bundled `log`'s macros: `info!(..)` and the other levels (also with `target: ..` and
/// `logger: ..`, which is the global logger here), `log!(level, ..)` and `log_enabled!(level)`.
const LOG_MACROS: &str = r#"
macro_rules! log {
    (logger: $l:expr, $($rest:tt)+) => { log!($($rest)+) };
    (target: $t:expr, $lvl:expr, $($arg:tt)+) => { __log_emit($lvl, &format!($($arg)+)) };
    ($lvl:expr, $($arg:tt)+) => { __log_emit($lvl, &format!($($arg)+)) };
}
macro_rules! log_enabled {
    (logger: $l:expr, $($rest:tt)+) => { log_enabled!($($rest)+) };
    (target: $t:expr, $lvl:expr) => { __log_enabled($lvl) };
    ($lvl:expr) => { __log_enabled($lvl) };
}
macro_rules! error {
    (logger: $l:expr, $($rest:tt)+) => { error!($($rest)+) };
    (target: $t:expr, $($arg:tt)+) => { __log_emit(LogLevel::Error, &format!($($arg)+)) };
    ($($arg:tt)+) => { __log_emit(LogLevel::Error, &format!($($arg)+)) };
}
macro_rules! warn {
    (logger: $l:expr, $($rest:tt)+) => { warn!($($rest)+) };
    (target: $t:expr, $($arg:tt)+) => { __log_emit(LogLevel::Warn, &format!($($arg)+)) };
    ($($arg:tt)+) => { __log_emit(LogLevel::Warn, &format!($($arg)+)) };
}
macro_rules! info {
    (logger: $l:expr, $($rest:tt)+) => { info!($($rest)+) };
    (target: $t:expr, $($arg:tt)+) => { __log_emit(LogLevel::Info, &format!($($arg)+)) };
    ($($arg:tt)+) => { __log_emit(LogLevel::Info, &format!($($arg)+)) };
}
macro_rules! debug {
    (logger: $l:expr, $($rest:tt)+) => { debug!($($rest)+) };
    (target: $t:expr, $($arg:tt)+) => { __log_emit(LogLevel::Debug, &format!($($arg)+)) };
    ($($arg:tt)+) => { __log_emit(LogLevel::Debug, &format!($($arg)+)) };
}
macro_rules! trace {
    (logger: $l:expr, $($rest:tt)+) => { trace!($($rest)+) };
    (target: $t:expr, $($arg:tt)+) => { __log_emit(LogLevel::Trace, &format!($($arg)+)) };
    ($($arg:tt)+) => { __log_emit(LogLevel::Trace, &format!($($arg)+)) };
}
"#;

/// `anyhow!("{x}")` / `anyhow!(err)`, `bail!(..)` and `ensure!(cond, ..)`, as anyhow defines them.
const ANYHOW_MACROS: &str = r#"
macro_rules! anyhow {
    ($msg:literal $(,)?) => { anyhow::Error::msg(format!($msg)) };
    ($err:expr $(,)?) => { anyhow::__wrap($err) };
    ($fmt:literal, $($arg:tt)*) => { anyhow::Error::msg(format!($fmt, $($arg)*)) };
}
macro_rules! bail {
    ($($t:tt)*) => { return Err(anyhow!($($t)*)) };
}
macro_rules! ensure {
    ($cond:expr, $($t:tt)+) => {
        if !$cond {
            return Err(anyhow!($($t)*));
        }
    };
    ($cond:expr $(,)?) => {
        if !$cond {
            return Err(anyhow::Error::msg(format!("Condition failed: `{}`", stringify!($cond))));
        }
    };
}
"#;

/// An invocation of a file macro.
struct Site {
    name: String,
    range: (usize, usize),
    /// Its argument token tree.
    tt: SyntaxNode,
    /// In an expression (its expansion is one expression), rather than in statement or item
    /// position (its statements or items go in as they are; an item's `;` goes away).
    in_expr: bool,
}

/// Whether a macro path names another crate's macro (`std::assert!`, `$crate::__priv::mem::
/// offset_of!` in a macro that is itself `offset_of!`), not one of the file's.
fn foreign_path(segs: &[String]) -> bool {
    // (`std::pin::pin!` is the bundled `pin!`.)
    if segs.ends_with(&["pin".to_string(), "pin".to_string()]) {
        return false;
    }
    segs.split_last().is_some_and(|(_, quals)| {
        quals
            .iter()
            .any(|q| matches!(q.as_str(), "std" | "core" | "alloc") || q.starts_with("__"))
    })
}

/// Whether `log::debug!` (a bundled crate's path) names that crate's macro while the file's
/// `debug` is its own `macro_rules!` (aho-corasick's `debug!` is `log!(log::debug!(..))`, which
/// would otherwise expand itself forever). The bundled definitions are left out then.
fn bundled_crate_path(segs: &[String], def: &ast::MacroRules) -> bool {
    let [first, _, ..] = segs else { return false };
    if !matches!(first.as_str(), "log" | "anyhow" | "tokio" | "futures" | "itertools") {
        return false;
    }
    let d = def.syntax().text().to_string();
    ![PIN_MACROS, LOG_MACROS, ANYHOW_MACROS, TOKIO_MACROS, ITERTOOLS_MACROS]
        .iter()
        .any(|m| m.contains(d.as_str()))
}

fn outer_sites(file: &SourceFile, defs: &HashMap<String, ast::MacroRules>) -> Vec<Site> {
    let in_def = |n: &SyntaxNode| n.ancestors().any(|a| ast::MacroRules::can_cast(a.kind()));
    // Tokens a proc macro emits (`quote! { icu::langid!(#repr) }`) aren't expanded here.
    let in_quote = |n: &SyntaxNode| {
        n.ancestors().filter_map(ast::MacroCall::cast).any(|c| {
            c.path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_some_and(|n| {
                    matches!(
                        n.text(),
                        "quote" | "quote_spanned" | "parse_quote" | "parse_quote_spanned"
                    )
                })
        })
    };
    let mut sites: Vec<Site> = Vec::new();
    for c in file.syntax().descendants().filter_map(ast::MacroCall::cast) {
        let Some(name) = c
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
        else {
            continue;
        };
        if !defs.contains_key(name.text()) || in_def(c.syntax()) {
            continue;
        }
        // (Not `std::name!` / `$crate::__priv::name!`, another crate's macro of the same name.)
        let segs: Vec<String> = c
            .path()
            .map(|p| {
                p.syntax()
                    .descendants()
                    .filter_map(ast::PathSegment::cast)
                    .map(|s| s.syntax().text().to_string())
                    .collect()
            })
            .unwrap_or_default();
        if foreign_path(&segs) || bundled_crate_path(&segs, &defs[name.text()]) {
            continue;
        }
        let Some(tt) = c.token_tree() else { continue };
        let parent = c.syntax().parent();
        let in_expr = match parent.as_ref().map(|p| p.kind()) {
            Some(SyntaxKind::MACRO_EXPR) => {
                let pp = parent.as_ref().and_then(|p| p.parent()).map(|p| p.kind());
                !matches!(pp, Some(SyntaxKind::EXPR_STMT | SyntaxKind::STMT_LIST))
            }
            _ => false,
        };
        let r = c.syntax().text_range();
        sites.push(Site {
            name: name.text().to_string(),
            range: (r.start().into(), r.end().into()),
            tt: tt.syntax().clone(),
            in_expr,
        });
    }
    // `name ! (..)` inside a token tree (`println!("{}", square!(3))`).
    for tt in file
        .syntax()
        .descendants()
        .filter(|n| n.kind() == SyntaxKind::TOKEN_TREE)
    {
        if in_def(&tt) || in_quote(&tt) {
            continue;
        }
        let items: Vec<NodeOrToken<SyntaxNode, ra_ap_syntax::SyntaxToken>> = tt
            .children_with_tokens()
            .filter(|e| !matches!(e.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
            .collect();
        for (k, w) in items.windows(3).enumerate() {
            // The path before it (`log::info!(..)`), unless it's another crate's macro.
            let text = |j: usize| items.get(j).map(|e| e.to_string()).unwrap_or_default();
            let mut first = k;
            let mut segs = vec![text(k)];
            while first >= 3
                && text(first - 1) == ":"
                && text(first - 2) == ":"
                && matches!(&items[first - 3], NodeOrToken::Token(t)
                    if t.kind() == SyntaxKind::IDENT || matches!(t.text(), "crate" | "self" | "super"))
            {
                first -= 3;
                segs.insert(0, text(first));
            }
            if foreign_path(&segs) {
                continue;
            }
            if let (NodeOrToken::Token(id), NodeOrToken::Token(bang), NodeOrToken::Node(args)) =
                (&w[0], &w[1], &w[2])
                && id.kind() == SyntaxKind::IDENT
                && bang.text() == "!"
                && args.kind() == SyntaxKind::TOKEN_TREE
                && let Some(def) = defs.get(id.text())
                && !bundled_crate_path(&segs, def)
            {
                let start = items[first].text_range().start();
                sites.push(Site {
                    name: id.text().to_string(),
                    range: (start.into(), args.text_range().end().into()),
                    tt: args.clone(),
                    in_expr: true,
                });
            }
        }
    }
    // The outermost ones: an invocation inside another's arguments waits for the next round.
    sites.sort_by_key(|s| (s.range.0, std::cmp::Reverse(s.range.1)));
    let mut out: Vec<Site> = Vec::new();
    for site in sites {
        if out.last().is_none_or(|l| site.range.0 >= l.range.1) {
            out.push(site);
        }
    }
    out
}

/// The ranges of the `use` items (or of the trees in a `use a::{b, c}` list) that import a macro
/// of the file by path: `pub(crate) use my_macro;`, `use crate::my_macro;`. Nothing else may
/// have the name (a `fn` and a macro can share one).
fn macro_uses(
    file: &SourceFile,
    defs: &HashMap<String, Vec<ast::MacroRules>>,
) -> Vec<(usize, usize)> {
    let range = |n: &SyntaxNode| -> (usize, usize) {
        let r = n.text_range();
        (r.start().into(), r.end().into())
    };
    let taken: Vec<String> = file
        .syntax()
        .descendants()
        .filter(|n| !ast::MacroRules::can_cast(n.kind()))
        .filter_map(|n| ast::Item::cast(n).and_then(|i| ast::AnyHasName::cast(i.syntax().clone())))
        .filter_map(|i| i.name())
        .map(|n| n.text().to_string())
        .collect();
    let is_macro = |name: &str| defs.contains_key(name) && !taken.iter().any(|t| t == name);
    // Whether a path starts at this crate (`crate::`, `self::`, `super::`).
    let in_crate = |p: &ast::Path| {
        let first = p
            .syntax()
            .descendants()
            .find_map(ast::PathSegment::cast)
            .map(|s| s.syntax().text().to_string());
        matches!(first.as_deref(), Some("crate" | "self" | "super"))
    };
    let last_name = |p: &ast::Path| {
        p.segment()
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string())
    };
    let mut out = Vec::new();
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        let Some(tree) = u.use_tree() else { continue };
        if tree.rename().is_some() || tree.star_token().is_some() {
            continue;
        }
        match (tree.path(), tree.use_tree_list()) {
            (Some(p), None) => {
                if (p.qualifier().is_none() || in_crate(&p))
                    && last_name(&p).is_some_and(|n| is_macro(&n))
                {
                    out.push(range(u.syntax()));
                }
            }
            (prefix, Some(list)) => {
                if prefix.as_ref().is_some_and(|p| !in_crate(p)) {
                    continue;
                }
                let trees: Vec<ast::UseTree> = list.use_trees().collect();
                let gone: Vec<&ast::UseTree> = trees
                    .iter()
                    .filter(|t| {
                        t.use_tree_list().is_none()
                            && t.rename().is_none()
                            && t.star_token().is_none()
                            && t.path().is_some_and(|p| {
                                p.qualifier().is_none()
                                    && last_name(&p).is_some_and(|n| is_macro(&n))
                            })
                    })
                    .collect();
                if gone.is_empty() {
                    continue;
                }
                if gone.len() == trees.len() {
                    out.push(range(u.syntax()));
                    continue;
                }
                for t in gone {
                    let (a, mut b) = range(t.syntax());
                    // (With its `,`.)
                    let mut next = t.syntax().next_sibling_or_token();
                    while let Some(e) = next {
                        match e.kind() {
                            SyntaxKind::WHITESPACE | SyntaxKind::COMMENT => {
                                next = e.next_sibling_or_token()
                            }
                            SyntaxKind::COMMA => {
                                b = e.text_range().end().into();
                                break;
                            }
                            _ => break,
                        }
                    }
                    out.push((a, b));
                }
            }
            _ => {}
        }
    }
    out
}

/// The marker `imported_macros` writes for the crate root's `#![recursion_limit = "N"]` when the
/// file is a module of it.
const ROOT_LIMIT: &str = "// crate root: recursion_limit = ";

/// The deepest a `#![recursion_limit]` takes expansions here (each level re-reads the file).
const MAX_RECURSION_LIMIT: usize = 1024;

/// How deep macro expansions may go: rustc's 128, or the crate's `#![recursion_limit = "N"]` (in
/// the file, or its crate root's, which `imported_macros` passes along), up to
/// `MAX_RECURSION_LIMIT`.
fn recursion_limit(text: &str) -> usize {
    text.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            let rest = l
                .strip_prefix(ROOT_LIMIT)
                .or_else(|| l.strip_prefix("#![recursion_limit").map(|r| r.trim_start()))?;
            let digits: String = rest
                .trim_start_matches(['=', ' ', '"'])
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse::<usize>().ok()
        })
        .max()
        .map_or(128, |n| n.min(MAX_RECURSION_LIMIT))
}

/// The `#![recursion_limit = "N"]` of the crate root `filename` is a module of (through the
/// `mod` items that declare it), as `ROOT_LIMIT`'s line.
fn root_recursion_limit(filename: &str, resolver: crate::typeck::Resolver) -> String {
    let mut file = filename.to_string();
    for _ in 0..16 {
        let Some((parent, text, _)) = declaring_file(&file, resolver) else {
            break;
        };
        if text.contains("#![recursion_limit") {
            let n = recursion_limit(&text);
            return format!("\n{ROOT_LIMIT}{n}\n");
        }
        file = parent;
    }
    String::new()
}

/// The definition an invocation at `at` uses, of those of its name: one whose `#[cfg]` holds
/// (`#[cfg(feature = "std")]` / `#[cfg(not(feature = "std"))]` alternatives), the last one
/// before the invocation (a macro defined again replaces the first from there on).
fn pick_def(defs: &[ast::MacroRules], at: usize) -> ast::MacroRules {
    let on: Vec<&ast::MacroRules> = defs
        .iter()
        .filter(|d| !crate::macros::under_false_cfg(d.syntax()))
        .collect();
    let candidates = if on.is_empty() {
        defs.iter().collect()
    } else {
        on
    };
    let before = candidates
        .iter()
        .rev()
        .find(|d| usize::from(d.syntax().text_range().start()) < at);
    before
        .or(candidates.first())
        .map_or_else(|| defs[0].clone(), |d| (*d).clone())
}

/// Expand every `macro_rules!` macro in `src`. Errors are (start, end, message) at original
/// offsets.
/// `expand` on text already rewritten by earlier passes, whose `map` leads back to the source.
fn expand_with(
    src: &str,
    map: OffsetMap,
) -> Result<Option<Expansion>, Vec<(usize, usize, String)>> {
    let mut map = map;
    let mut expansions = 0usize;
    let mut text = src.to_string();
    let mut rounds = 0;
    let mut changed = false;
    let limit = recursion_limit(src);
    loop {
        let file = crate::macros::parse_tree(&text);
        // Every definition of each name, in source order (`#[cfg]`-gated alternatives, or a
        // macro defined again further down).
        let mut all: HashMap<String, Vec<ast::MacroRules>> = HashMap::new();
        for d in file
            .syntax()
            .descendants()
            .filter_map(ast::MacroRules::cast)
        {
            if let Some(n) = d.name() {
                all.entry(n.text().to_string()).or_default().push(d);
            }
        }
        if all.is_empty() {
            break;
        }
        let defs: HashMap<String, ast::MacroRules> = all
            .iter()
            .filter_map(|(n, ds)| ds.last().map(|d| (n.clone(), d.clone())))
            .collect();
        // Every outermost invocation of a file macro, outside the definitions: macro calls in
        // the syntax tree, and `name!(..)` inside other macros' arguments (token trees).
        let sites = outer_sites(&file, &defs);
        if sites.is_empty() {
            // Done: drop the definitions.
            let mut ranges: Vec<(usize, usize)> = all
                .values()
                .flatten()
                .map(|d| {
                    let r = d.syntax().text_range();
                    (r.start().into(), r.end().into())
                })
                .collect();
            // (And `pub(crate) use my_macro;`, the macro's path-based import.)
            ranges.extend(macro_uses(&file, &all));
            ranges.sort();
            ranges.dedup();
            for (a, b) in ranges.into_iter().rev() {
                text.replace_range(a..b, "");
                map.replace(a, b, 0);
            }
            changed = true;
            break;
        }
        rounds += 1;
        // Expand them all against this text, then splice from the last so offsets stay valid.
        let mut edits: Vec<((usize, usize), String)> = Vec::new();
        // (Each definition's rules once per round, by where it starts.)
        let mut rule_sets: HashMap<usize, std::rc::Rc<Vec<Rule>>> = HashMap::new();
        for site in &sites {
            let (a, b) = site.range;
            let err = |msg: String| vec![(map.orig(a, false), map.orig(b, true), msg)];
            if rounds > limit {
                let hint = if limit < MAX_RECURSION_LIMIT {
                    format!(
                        "; a `#![recursion_limit = \"{}\"]` attribute on the crate raises it",
                        limit * 2
                    )
                } else {
                    ", the most CarciniScript expands".to_string()
                };
                return Err(err(format!(
                    "recursion limit reached while expanding macros ({limit} levels{hint})"
                )));
            }
            let name = site.name.clone();
            let def = pick_def(&all[&name], a);
            let rules = match rule_sets.get(&usize::from(def.syntax().text_range().start())) {
                Some(r) => r.clone(),
                None => {
                    let r = std::rc::Rc::new(
                        rules_of(&def)
                            .map_err(|m| err(format!("in `macro_rules! {name}`: {m}")))?,
                    );
                    rule_sets.insert(usize::from(def.syntax().text_range().start()), r.clone());
                    r
                }
            };
            let (_, _, toks) = token_tree(&site.tt);
            let m = Matcher { src: &text };
            let mut out = None;
            for rule in rules.iter() {
                let mut bind = Bindings::new();
                if m.seq(&rule.matcher, &toks, 0, &[], &mut bind) == Some(toks.len()) {
                    let mut s = String::new();
                    expansions += 1;
                    let tpl = hygienic(&rule.transcriber, expansions);
                    transcribe(&tpl, &bind, &mut s)
                        .map_err(|e| err(format!("in `{name}!`: {e}")))?;
                    out = Some(s);
                    break;
                }
            }
            let Some(body) = out else {
                return Err(err(format!(
                    "no rules of `macro_rules! {name}` matched this invocation"
                )));
            };
            edits.push((
                (a, b),
                if site.in_expr {
                    // An expression's trailing `;` is dropped, as rustc does (`None =>
                    // bail!(..)` of `return Err(..);`).
                    let body = body.trim_end();
                    let body = body.strip_suffix(';').unwrap_or(body);
                    format!("({body})")
                } else {
                    body
                },
            ));
        }
        for ((a, b), replacement) in edits.into_iter().rev() {
            text.replace_range(a..b, &replacement);
            map.replace(a, b, replacement.len());
        }
        changed = true;
    }
    Ok(changed.then_some(Expansion { text, map }))
}

/// The source the compiler actually sees: macros expanded and inline modules flattened, with
/// the map back to `src` and the original names of the types that were renamed.
pub struct Preprocessed {
    pub text: String,
    pub map: OffsetMap,
    pub names: HashMap<String, String>,
}

/// `None` when `src` needs neither step.
pub fn preprocess(src: &str) -> Result<Option<Preprocessed>, Vec<(usize, usize, String)>> {
    preprocess_with(src, "", None)
}

/// `preprocess`, with `macros` (definitions from other files, see `imported_macros`) usable too.
/// They go after `src`, so offsets in `src` stay put, and are dropped once expanded.
/// `include_str!("path")` (and `include_bytes!`), read through the resolver that loads `mod`
/// files, written in as a literal. (Found in the text, so also among other macros' arguments.)
/// The path is from the directory of `filename` (or from the root as `/lib/words.txt`), else as
/// written.
fn include_strs(
    text: &mut String,
    map: &mut OffsetMap,
    filename: &str,
    resolver: crate::typeck::Resolver,
) -> Result<bool, Vec<(usize, usize, String)>> {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut errors = Vec::new();
    for name in ["include_str", "include_bytes"] {
        let pat = format!("{name}!");
        let mut from = 0;
        while let Some(k) = text[from..].find(&pat) {
            let a = from + k;
            from = a + pat.len();
            // (Not the tail of a longer name, nor in a line comment.)
            let before = text[..a].chars().next_back();
            // (Nor in an attribute: `#[doc = include_str!("../README.md")]` isn't read.)
            let line = text[..a].rsplit('\n').next().unwrap_or("");
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                || line.contains("//")
                || line.trim_start().starts_with("#[")
                || line.trim_start().starts_with("#![")
            {
                continue;
            }
            let rest = &text[from..];
            let t = rest.trim_start();
            let Some(t) = t.strip_prefix('(') else {
                continue;
            };
            let t2 = t.trim_start();
            let Some(t3) = t2.strip_prefix('"') else {
                continue;
            };
            let Some(q) = t3.find('"') else { continue };
            let path = t3[..q].replace("\\\\", "/");
            let after = t3[q + 1..].trim_start();
            let Some(after) = after.strip_prefix(')') else {
                continue;
            };
            let b = text.len() - after.len();
            match read_next_to(filename, &path, resolver) {
                Some((_, content)) => {
                    let new = if name == "include_str" {
                        format!("{content:?}")
                    } else {
                        let bytes: Vec<String> =
                            content.bytes().map(|x| format!("{x}u8")).collect();
                        format!("&[{}]", bytes.join(", "))
                    };
                    edits.push((a, b, new));
                }
                None => errors.push((
                    map.orig(a, false),
                    map.orig(b, true),
                    format!("couldn't read `{path}`: `{name}!` reads files next to the script"),
                )),
            }
            from = b;
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let changed = !edits.is_empty();
    for (a, b, new) in edits {
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
    }
    Ok(changed)
}

/// Macros named like keywords of the 2021 edition the file is parsed as. `safe` (bitvec's
/// `safe! { .. }`): the parser takes `safe` before an item as the `safe fn` qualifier and fails
/// on `safe!`, so its definition and calls are spelled `Safe` (macros have their own namespace,
/// so a type `Safe` is fine). `try` (2015-edition code's `try!(x)`, std's or the file's own
/// `macro_rules! try`) is a keyword since 2018, spelled `r#try` as rustc does.
fn rename_keyword_macros(text: &mut String, map: &mut OffsetMap) -> bool {
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let named = |name: &str| {
        text.match_indices("macro_rules!").any(|(k, m)| {
            let rest = text[k + m.len()..].trim_start();
            rest.starts_with(name) && !rest.as_bytes().get(name.len()).is_some_and(|&c| word(c))
        })
    };
    let safe = named("safe");
    let try_ = text.contains("try!");
    if !safe && !try_ {
        return false;
    }
    // The `safe` / `try` tokens before a `!` (not `!=`) or after `macro_rules!`.
    let file = crate::macros::parse_tree(text);
    let toks: Vec<ra_ap_syntax::SyntaxToken> = file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        // (`safe` is lexed as an identifier, but the parser may have made it the keyword.)
        let to = match (t.kind(), t.text()) {
            (SyntaxKind::IDENT | SyntaxKind::SAFE_KW, "safe") if safe => "Safe",
            (SyntaxKind::TRY_KW, "try") => "r#try",
            _ => continue,
        };
        let next = |k: usize| toks.get(i + k).map(|t| t.text().to_string());
        let call = next(1).as_deref() == Some("!")
            && !(next(2).as_deref() == Some("=")
                && toks[i + 1].text_range().end() == toks[i + 2].text_range().start());
        let def = i >= 2 && toks[i - 1].text() == "!" && toks[i - 2].text() == "macro_rules";
        if call || def {
            let r = t.text_range();
            edits.push((r.start().into(), r.end().into(), to));
        }
    }
    for &(a, b, to) in edits.iter().rev() {
        text.replace_range(a..b, to);
        map.replace(a, b, to.len());
    }
    !edits.is_empty()
}

/// `macro_rules ! name { .. }` (generated code spaces it out) as `macro_rules! name`, which the
/// passes below look for.
fn join_macro_rules_bang(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let toks: Vec<ra_ap_syntax::SyntaxToken> = file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let mut edits: Vec<(usize, usize)> = Vec::new();
    for w in toks.windows(2) {
        if w[0].text() == "macro_rules"
            && w[1].text() == "!"
            && w[0].text_range().end() < w[1].text_range().start()
        {
            edits.push((
                w[0].text_range().end().into(),
                w[1].text_range().start().into(),
            ));
        }
    }
    for &(a, b) in edits.iter().rev() {
        text.replace_range(a..b, "");
        map.replace(a, b, 0);
    }
    !edits.is_empty()
}

/// `pub use __make_provider as make_provider;` (also `use crate::m as n;`) for a macro of the
/// file: the `use` becomes a copy of the definition under the new name, so `make_provider!(..)`
/// expands as `__make_provider!(..)` does. Not when an item of the file has the old name too (the
/// `use` would import that as well).
fn macro_use_renames(text: &mut String, map: &mut OffsetMap) -> bool {
    let file = crate::macros::parse_tree(text);
    let mut defs: HashMap<String, ast::MacroRules> = HashMap::new();
    for d in file
        .syntax()
        .descendants()
        .filter_map(ast::MacroRules::cast)
    {
        if let Some(n) = d.name() {
            defs.insert(n.text().to_string(), d);
        }
    }
    if defs.is_empty() {
        return false;
    }
    let taken: Vec<String> = file
        .syntax()
        .descendants()
        .filter(|n| !ast::MacroRules::can_cast(n.kind()))
        .filter_map(|n| ast::Item::cast(n).and_then(|i| ast::AnyHasName::cast(i.syntax().clone())))
        .filter_map(|i| i.name())
        .map(|n| n.text().to_string())
        .collect();
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for u in file.syntax().children().filter_map(ast::Use::cast) {
        let Some(tree) = u.use_tree() else { continue };
        let (Some(p), Some(alias)) = (tree.path(), tree.rename().and_then(|r| r.name())) else {
            continue;
        };
        let segs: Vec<String> = p
            .segments()
            .filter_map(|s| s.name_ref())
            .map(|n| n.text().to_string())
            .collect();
        let (Some(name), rest) = (segs.last(), &segs[..segs.len().saturating_sub(1)]) else {
            continue;
        };
        if !rest.iter().all(|s| matches!(s.as_str(), "crate" | "self"))
            || taken.iter().any(|t| t == name)
        {
            continue;
        }
        let Some(tt) = defs.get(name).and_then(|d| d.token_tree()) else {
            continue;
        };
        let body = tt.syntax().text().to_string();
        let semi = if body.starts_with('{') { "" } else { ";" };
        let r = u.syntax().text_range();
        edits.push((
            r.start().into(),
            r.end().into(),
            format!("macro_rules! {} {body}{semi}", alias.text()),
        ));
    }
    for (a, b, new) in edits.iter().rev() {
        text.replace_range(*a..*b, new);
        map.replace(*a, *b, new.len());
    }
    !edits.is_empty()
}

/// `include!("tables.rs")`: the file's text in place of the call (an item's `;` too), before
/// `macro_rules!` expansion, so the macros it defines and calls are expanded with the file's.
/// Its own `include!`s are read next to it (`include!("../data/mod.rs")` including
/// `"tables.rs"` reads `../data/tables.rs`). A file that can't be read is left for `stdgaps`,
/// which reports it.
/// The file `path` names from `filename` (`include_str!("words.txt")`, `include!("a.rs")`): next
/// to the file (`..` and `/from/root` as in `#[path]`), else, as earlier versions read it, by
/// `path` as written.
fn read_next_to(
    filename: &str,
    path: &str,
    resolver: crate::typeck::Resolver,
) -> Option<(String, String)> {
    crate::typeck::relative_path(filename, path)
        .and_then(|p| resolver(&p).map(|t| (p, t)))
        .or_else(|| resolver(path).map(|t| (path.to_string(), t)))
}

fn include_items(
    text: &mut String,
    map: &mut OffsetMap,
    filename: &str,
    resolver: crate::typeck::Resolver,
) -> bool {
    // `src` is the text of `file`, whose `include!`s are next to it.
    fn inlined(src: &str, file: &str, resolver: crate::typeck::Resolver, depth: usize) -> String {
        let mut out = src.to_string();
        let mut edits = Vec::new();
        for (a, b, path) in include_calls(src) {
            if depth < 8
                && let Some((full, content)) = read_next_to(file, &path, resolver)
            {
                edits.push((a, b, inlined(&content, &full, resolver, depth + 1)));
            }
        }
        for (a, b, new) in edits.into_iter().rev() {
            out.replace_range(a..b, &new);
        }
        out
    }
    let mut edits = Vec::new();
    for (a, b, path) in include_calls(text) {
        let Some((full, content)) = read_next_to(filename, &path, resolver) else {
            continue;
        };
        edits.push((a, b, inlined(&content, &full, resolver, 1)));
    }
    for (a, b, new) in edits.iter().rev() {
        text.replace_range(*a..*b, new);
        map.replace(*a, *b, new.len());
    }
    !edits.is_empty()
}

/// The `include!("path")` calls of `text` with a string literal: (start, end, path), an item's
/// `;` included. Not `x::include!`, nor inside a `macro_rules!` definition.
fn include_calls(text: &str) -> Vec<(usize, usize, String)> {
    if !text.contains("include!") {
        return Vec::new();
    }
    let file = crate::macros::parse_tree(text);
    file.syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|m| {
            m.path()
                .is_some_and(|p| p.qualifier().is_none() && p.to_string() == "include")
                && !m
                    .syntax()
                    .ancestors()
                    .any(|a| a.kind() == SyntaxKind::MACRO_RULES)
        })
        .filter_map(|m| {
            let tt = m.token_tree()?;
            let toks: Vec<ra_ap_syntax::SyntaxToken> = tt
                .syntax()
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !t.kind().is_trivia())
                .collect();
            // `(` "path" `)`
            let [_, lit, _] = toks.as_slice() else {
                return None;
            };
            let path = <ast::String as ra_ap_syntax::AstToken>::cast(lit.clone())?
                .value()
                .ok()?
                .replace('\\', "/");
            let r = m.syntax().text_range();
            Some((r.start().into(), r.end().into(), path))
        })
        .collect()
}

/// std's `try!` (2015 edition), for files that don't define their own: `?`.
const TRY_MACRO: &str = r#"
macro_rules! r#try {
    ($e:expr $(,)?) => { $e? };
}
"#;

pub fn preprocess_with(
    src: &str,
    macros: &str,
    procs: Option<(&str, crate::typeck::Resolver)>,
) -> Result<Option<Preprocessed>, Vec<(usize, usize, String)>> {
    // (std's `try!`, unless the file or its macros define one.)
    let with_try;
    let try_call = src.match_indices("try!").any(|(k, _)| {
        !src[..k].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '#')
            && !src[k + 4..].starts_with('=')
    });
    let defines_try = |s: &str| {
        s.lines().any(|l| {
            l.trim_start()
                .strip_prefix("macro_rules!")
                .map(str::trim_start)
                .and_then(|r| r.strip_prefix("try"))
                .is_some_and(|r| !r.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        })
    };
    let macros = if try_call && !defines_try(src) && !defines_try(macros) {
        with_try = format!("{macros}{TRY_MACRO}");
        with_try.as_str()
    } else {
        macros
    };
    let full = if macros.is_empty() {
        src.to_string()
    } else {
        format!("{src}\n{macros}")
    };
    let mut text = full.clone();
    let mut map = OffsetMap::identity(full.len());
    let mut changed = false;
    if text.contains("macro_rules ") {
        changed |= join_macro_rules_bang(&mut text, &mut map);
    }
    // Items under a `#[cfg(..)]` that's off go first (alternative definitions of one name).
    changed |= crate::macros::strip_cfg(&mut text);
    // Proc macros first: their output may define or use `macro_rules!` macros.
    if let Some((filename, resolver)) = procs {
        // (`jsx!` components can be `pub fn`s of the `mod` files.)
        if text.contains("jsx!") {
            crate::jsx::set_mod_sources(&text, filename, resolver);
        }
        // `include_str!("words.txt")`: the file next to this one, as a string literal.
        if text.contains("include_str!") {
            changed |= include_strs(&mut text, &mut map, filename, resolver)?;
        }
        // (`include!("tables.rs")` among the items may define and call macros.)
        if text.contains("include!") && include_items(&mut text, &mut map, filename, resolver) {
            changed = true;
            crate::macros::strip_cfg(&mut text);
        }
        changed |= crate::procmacro::expand(&mut text, &mut map, filename, resolver)?;
    }
    // The bundled crates written as rewrites (`#[derive(thiserror::Error)]`, `bitflags!`, ..).
    if crate::bundled::uses_bundled(&text) {
        changed |= crate::bundled::expand(&mut text, &mut map)?;
    }
    if text.contains("macro_rules!") {
        changed |= rename_keyword_macros(&mut text, &mut map);
    }
    if text.contains("macro_rules!") && text.contains(" as ") {
        changed |= macro_use_renames(&mut text, &mut map);
    }
    if text.contains("macro_rules!")
        && let Some(e) = expand_with(&text, map.clone())?
    {
        text = e.text;
        map = e.map;
        changed = true;
        // (Expansions can have `#[cfg]`s too.)
        crate::macros::strip_cfg(&mut text);
    }
    // `extern crate itertools as it;`
    if crate::alias::has_crate_renames(&text) {
        changed |= crate::alias::expand_crate_renames(&mut text, &mut map);
    }
    // `I: IntoIterator<Item: Display>`
    if crate::assocbounds::has_assoc_bounds(&text) {
        changed |= crate::assocbounds::expand_assoc_bounds(&mut text, &mut map);
    }
    // `use geo::shapes::Circle;` with `pub mod shapes { .. }` inside `geo.rs`.
    if crate::alias::has_inline_module_imports(&text) {
        changed |= crate::alias::expand_inline_module_imports(&mut text, &mut map, procs);
    }
    let mut names = HashMap::new();
    if crate::modflat::has_inline_modules(&text) {
        let before = map.clone();
        let flat = crate::modflat::flatten(&mut text, &mut map);
        if !flat.errors.is_empty() {
            return Err(flat
                .errors
                .into_iter()
                .map(|(a, b, e)| (before.orig(a, false), before.orig(b, true), e))
                .collect());
        }
        names = flat.names;
        changed = true;
    }
    if crate::variants::has_variant_imports(&text) {
        crate::variants::expand(&mut text, &mut map);
        changed = true;
    }
    if crate::alias::has_import_renames(&text) {
        changed |= crate::alias::expand_import_renames(&mut text, &mut map, procs);
    }
    if crate::alias::has_use_renames(&text) {
        changed |= crate::alias::expand_use_renames(&mut text, &mut map);
    }
    if crate::alias::has_aliases(&text) {
        crate::alias::expand(&mut text, &mut map);
        changed = true;
    }
    // (`<u8>::max(a, b)` is `u8::max(a, b)`, for the next one.)
    if crate::ufcs::has_type_anchors(&text) {
        changed |= crate::ufcs::expand_type_anchors(&mut text, &mut map);
    }
    if crate::ufcs::has_prim_paths(&text) {
        changed |= crate::ufcs::expand(&mut text, &mut map);
    }
    // Nested `fn`s named like another function.
    if crate::nested::has_duplicate_fns(&text) {
        changed |= crate::nested::expand(&mut text, &mut map);
    }
    // `std::path` / `OsStr` / `fmt::Arguments` names, `format_args!`, `env!`, `include!`.
    if crate::stdgaps::has_rewrites(&text) {
        changed |= crate::stdgaps::expand(&mut text, &mut map, procs.map(|p| p.1))?;
    }
    Ok(changed.then_some(Preprocessed { text, map, names }))
}

/// The `macro_rules!` definitions this file can use from its `mod x;` files: those marked
/// `#[macro_export]`, and all of a module's when its `mod` has `#[macro_use]`.
pub fn imported_macros(src: &str, filename: &str, resolver: crate::typeck::Resolver) -> String {
    let theirs = crate_macros(src, filename, resolver);
    // A bundled crate's macro that the crate defines itself (regex-automata's `log!`,
    // wasmparser's `bail!`) is the crate's.
    let mut out = own_macros_left_out(&bundled_macros(src), &theirs);
    out.push_str(&theirs);
    if !out.is_empty() || src.contains("macro_rules!") {
        out.push_str(&root_recursion_limit(filename, resolver));
    }
    out
}

/// The macros of the crate's other files that `src` sees.
fn crate_macros(src: &str, filename: &str, resolver: crate::typeck::Resolver) -> String {
    use ra_ap_syntax::ast::{HasAttrs, HasModuleItem};
    let mut out = String::new();
    // The macros in scope where the file's `mod` item is (Rust's textual scoping), and the
    // crate's `#[macro_export]` ones.
    out.push_str(&inherited_macros(filename, resolver, 0));
    if !src.contains("mod ") {
        return out;
    }
    let file = crate::macros::parse_tree(src);
    for item in file.items() {
        let ast::Item::Module(m) = item else { continue };
        if m.item_list().is_some() {
            continue;
        }
        let Some(name) = m.name() else { continue };
        let n = name.text().to_string();
        let macro_use = m
            .attrs()
            .any(|a| a.syntax().text().to_string().contains("macro_use"));
        let explicit = crate::typeck::path_attr(&m);
        let candidates = crate::typeck::module_candidates(filename, &n, explicit.as_deref());
        let Some(text) = candidates.iter().find_map(|(p, _)| resolver(p)) else {
            continue;
        };
        if !text.contains("macro_rules!") {
            continue;
        }
        let module = crate::macros::parse_tree(&text);
        for d in module.items().filter_map(|i| match i {
            ast::Item::MacroRules(d) => Some(d),
            _ => None,
        }) {
            let exported = d
                .attrs()
                .any(|a| a.syntax().text().to_string().contains("macro_export"));
            if exported || macro_use {
                out.push_str(&d.syntax().text().to_string());
                out.push('\n');
            }
        }
    }
    out
}

/// The file that declares module file `filename` with `mod name;` (`main.rs`, `lib.rs` or
/// `mod.rs` next to it, or `dir.rs` for `dir/name.rs`), with its text and the module's name.
fn declaring_file(
    filename: &str,
    resolver: crate::typeck::Resolver,
) -> Option<(String, String, String)> {
    let (dir, base) = match filename.rfind('/') {
        Some(i) => (&filename[..i], &filename[i + 1..]),
        None => ("", filename),
    };
    let stem = base.strip_suffix(".rs")?;
    if matches!(stem, "main" | "lib") {
        return None;
    }
    // (`dir/mod.rs` is the module `dir`, declared next to `dir`.)
    let (name, search) = if stem == "mod" {
        let (up, last) = match dir.rfind('/') {
            Some(i) => (&dir[..i], &dir[i + 1..]),
            None => ("", dir),
        };
        (last.to_string(), up.to_string())
    } else {
        (stem.to_string(), dir.to_string())
    };
    if name.is_empty() {
        return None;
    }
    let join = |d: &str, f: &str| {
        if d.is_empty() {
            f.to_string()
        } else {
            format!("{d}/{f}")
        }
    };
    let mut candidates = vec![
        join(&search, "main.rs"),
        join(&search, "lib.rs"),
        join(&search, "mod.rs"),
    ];
    if !search.is_empty() {
        let (up, last) = match search.rfind('/') {
            Some(i) => (&search[..i], &search[i + 1..]),
            None => ("", search.as_str()),
        };
        candidates.push(join(up, &format!("{last}.rs")));
    }
    let decl = format!("mod {name};");
    candidates.into_iter().find_map(|p| {
        let text = resolver(&p)?;
        text.contains(&decl).then(|| (p, text, name.clone()))
    })
}

/// The `macro_rules!` a module file inherits from where it's declared: the declaring file's
/// macros above its `mod name;` and those of the `#[macro_use] mod` files above it (and so on up
/// to the crate root), plus every `#[macro_export]` macro of the files the root loads.
fn inherited_macros(filename: &str, resolver: crate::typeck::Resolver, depth: usize) -> String {
    use ra_ap_syntax::ast::{HasAttrs, HasModuleItem};
    if depth > 16 {
        return String::new();
    }
    let Some((parent, text, name)) = declaring_file(filename, resolver) else {
        return String::new();
    };
    if !text.contains("macro_rules!") && !text.contains("macro_use") && depth > 0 {
        return inherited_macros(&parent, resolver, depth + 1);
    }
    let mut out = inherited_macros(&parent, resolver, depth + 1);
    let file = crate::macros::parse_tree(&text);
    let module_text = |m: &ast::Module, n: &str| {
        let explicit = crate::typeck::path_attr(m);
        crate::typeck::module_candidates(&parent, n, explicit.as_deref())
            .iter()
            .find_map(|(p, _)| resolver(p))
    };
    let mut before = true;
    for item in file.items() {
        match &item {
            ast::Item::Module(m) if m.item_list().is_none() => {
                let Some(n) = m.name().map(|n| n.text().to_string()) else {
                    continue;
                };
                if n == name {
                    before = false;
                    continue;
                }
                let macro_use = m
                    .attrs()
                    .any(|a| a.syntax().text().to_string().contains("macro_use"));
                let Some(mt) = module_text(m, &n).filter(|t| t.contains("macro_rules!")) else {
                    continue;
                };
                let module = crate::macros::parse_tree(&mt);
                for d in module.items().filter_map(|i| match i {
                    ast::Item::MacroRules(d) => Some(d),
                    _ => None,
                }) {
                    let exported = d
                        .attrs()
                        .any(|a| a.syntax().text().to_string().contains("macro_export"));
                    if (macro_use && before) || exported {
                        out.push_str(&d.syntax().text().to_string());
                        out.push('\n');
                    }
                }
            }
            // The declaring file's own macros: those above the `mod`, and exported ones.
            ast::Item::MacroRules(d) => {
                let exported = d
                    .attrs()
                    .any(|a| a.syntax().text().to_string().contains("macro_export"));
                if before || exported {
                    out.push_str(&d.syntax().text().to_string());
                    out.push('\n');
                }
            }
            _ => {}
        }
    }
    out
}

/// The bundled tokio's `join!` (every future is started, then each awaited, so they run
/// together) and `try_join!` (the first error ends it), for up to six futures.
const TOKIO_MACROS: &str = r#"
macro_rules! join {
    ($a:expr $(,)?) => {{ let __ja = $a; (__ja.await,) }};
    ($a:expr, $b:expr $(,)?) => {{ let __ja = $a; let __jb = $b; (__ja.await, __jb.await,) }};
    ($a:expr, $b:expr, $c:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; (__ja.await, __jb.await, __jc.await,) }};
    ($a:expr, $b:expr, $c:expr, $d:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; let __jd = $d; (__ja.await, __jb.await, __jc.await, __jd.await,) }};
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; let __jd = $d; let __je = $e; (__ja.await, __jb.await, __jc.await, __jd.await, __je.await,) }};
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $g:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; let __jd = $d; let __je = $e; let __jg = $g; (__ja.await, __jb.await, __jc.await, __jd.await, __je.await, __jg.await,) }};
}
macro_rules! try_join {
    ($a:expr $(,)?) => {{ let __ja = $a; async { Ok((__ja.await?,)) }.await }};
    ($a:expr, $b:expr $(,)?) => {{ let __ja = $a; let __jb = $b; async { Ok((__ja.await?, __jb.await?,)) }.await }};
    ($a:expr, $b:expr, $c:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; async { Ok((__ja.await?, __jb.await?, __jc.await?,)) }.await }};
    ($a:expr, $b:expr, $c:expr, $d:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; let __jd = $d; async { Ok((__ja.await?, __jb.await?, __jc.await?, __jd.await?,)) }.await }};
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; let __jd = $d; let __je = $e; async { Ok((__ja.await?, __jb.await?, __jc.await?, __jd.await?, __je.await?,)) }.await }};
    ($a:expr, $b:expr, $c:expr, $d:expr, $e:expr, $g:expr $(,)?) => {{ let __ja = $a; let __jb = $b; let __jc = $c; let __jd = $d; let __je = $e; let __jg = $g; async { Ok((__ja.await?, __jb.await?, __jc.await?, __jd.await?, __je.await?, __jg.await?,)) }.await }};
}
"#;
