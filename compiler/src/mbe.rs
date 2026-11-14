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

use crate::macros::EDITION;

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
        for s in &self.segs {
            if off < s.cur.1 || (off == s.cur.1 && s.cur.0 == s.cur.1) {
                if off < s.cur.0 {
                    continue;
                }
                return if s.synthetic {
                    if end { s.orig.1 } else { s.orig.0 }
                } else {
                    s.orig.0 + (off - s.cur.0)
                };
            }
        }
        self.segs
            .last()
            .map(|s| s.orig.1 + off.saturating_sub(s.cur.1))
            .unwrap_or(off)
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
        let spaced = items
            .get(i + 1)
            .is_some_and(|nx| matches!(nx.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT));
        match it {
            NodeOrToken::Token(t) => {
                if matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT) {
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
    None
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

/// A captured fragment: its text (and whether it's an `expr`), or one per repetition.
#[derive(Clone, Debug)]
enum Bound {
    One(String, bool),
    Seq(Vec<Bound>),
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
                    let text = if end == pos {
                        String::new()
                    } else {
                        let (s, e) = (toks[pos].range().0, toks[end - 1].range().1);
                        self.src[s..e].to_string()
                    };
                    b.insert(name.clone(), Bound::One(text, kind == "expr"));
                    pos = end;
                }
                Pat::Rep(inner, sep, op) => {
                    let mut names = Vec::new();
                    pat_names(inner, &mut names);
                    let mut iters: Vec<Bindings> = Vec::new();
                    // A greedy fragment at the end of the repetition stops at the separator.
                    let inner_follow: Vec<String> = match sep {
                        Some(s) => vec![s.clone()],
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
                            Some(s) if toks.get(pos).and_then(Tok::text) == Some(s.as_str()) => {
                                // Only consume the separator if another repetition follows.
                                let mut probe = Bindings::new();
                                if self
                                    .seq(inner, toks, pos + 1, &inner_follow, &mut probe)
                                    .is_some()
                                {
                                    pos += 1;
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
            "tt" => Some(pos + 1),
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
            _ => {
                // Like rustc's fragment parsers: an `expr` never holds a top-level `,`, `;` or
                // `=>`; a type or path holds commas only between `<` and `>`.
                let generic = matches!(kind, "ty" | "path");
                let mut angle = 0i32;
                let mut end = pos;
                while end < toks.len() {
                    if !stop.is_empty() && starts_with(toks, end, stop) {
                        break;
                    }
                    let t = toks[end].text();
                    if generic {
                        match t {
                            Some("<") => angle += 1,
                            Some(">") if angle > 0 => angle -= 1,
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

fn starts_with(toks: &[Tok], at: usize, seq: &[String]) -> bool {
    seq.iter()
        .enumerate()
        .all(|(k, s)| toks.get(at + k).and_then(Tok::text) == Some(s.as_str()))
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

fn transcribe(t: &[Tpl], b: &Bindings, out: &mut String) -> Result<(), String> {
    for x in t {
        match x {
            Tpl::Tok(s, spaced) => {
                out.push_str(s);
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
                    Some(Bound::One(text, expr)) => {
                        if *expr {
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
                    transcribe(inner, &ib, out)?;
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

fn outer_sites(file: &SourceFile, defs: &HashMap<String, ast::MacroRules>) -> Vec<Site> {
    let in_def = |n: &SyntaxNode| n.ancestors().any(|a| ast::MacroRules::can_cast(a.kind()));
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
        if in_def(&tt) {
            continue;
        }
        let items: Vec<NodeOrToken<SyntaxNode, ra_ap_syntax::SyntaxToken>> = tt
            .children_with_tokens()
            .filter(|e| !matches!(e.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT))
            .collect();
        for w in items.windows(3) {
            if let (NodeOrToken::Token(id), NodeOrToken::Token(bang), NodeOrToken::Node(args)) =
                (&w[0], &w[1], &w[2])
                && id.kind() == SyntaxKind::IDENT
                && bang.text() == "!"
                && args.kind() == SyntaxKind::TOKEN_TREE
                && defs.contains_key(id.text())
            {
                sites.push(Site {
                    name: id.text().to_string(),
                    range: (
                        id.text_range().start().into(),
                        args.text_range().end().into(),
                    ),
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

/// Expand every `macro_rules!` macro in `src`. Errors are (start, end, message) at original
/// offsets.
pub fn expand(src: &str) -> Result<Option<Expansion>, Vec<(usize, usize, String)>> {
    let mut text = src.to_string();
    let mut map = OffsetMap::identity(src.len());
    let mut rounds = 0;
    let mut changed = false;
    loop {
        let file = SourceFile::parse(&text, EDITION).tree();
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
            break;
        }
        // Every outermost invocation of a file macro, outside the definitions: macro calls in
        // the syntax tree, and `name!(..)` inside other macros' arguments (token trees).
        let sites = outer_sites(&file, &defs);
        if sites.is_empty() {
            // Done: drop the definitions.
            let mut ranges: Vec<(usize, usize)> = defs
                .values()
                .map(|d| {
                    let r = d.syntax().text_range();
                    (r.start().into(), r.end().into())
                })
                .collect();
            ranges.sort();
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
        for site in &sites {
            let (a, b) = site.range;
            let err = |msg: String| vec![(map.orig(a, false), map.orig(b, true), msg)];
            if rounds > 128 {
                return Err(err(
                    "recursion limit reached while expanding macros (128 levels)".into(),
                ));
            }
            let name = site.name.clone();
            let rules = rules_of(&defs[&name])
                .map_err(|m| err(format!("in `macro_rules! {name}`: {m}")))?;
            let (_, _, toks) = token_tree(&site.tt);
            let m = Matcher { src: &text };
            let mut out = None;
            for rule in &rules {
                let mut bind = Bindings::new();
                if m.seq(&rule.matcher, &toks, 0, &[], &mut bind) == Some(toks.len()) {
                    let mut s = String::new();
                    transcribe(&rule.transcriber, &bind, &mut s)
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
    preprocess_with(src, "")
}

/// `preprocess`, with `macros` (definitions from other files, see `imported_macros`) usable too.
/// They go after `src`, so offsets in `src` stay put, and are dropped once expanded.
pub fn preprocess_with(
    src: &str,
    macros: &str,
) -> Result<Option<Preprocessed>, Vec<(usize, usize, String)>> {
    let full = if macros.is_empty() {
        src.to_string()
    } else {
        format!("{src}\n{macros}")
    };
    let mut text = full.clone();
    let mut map = OffsetMap::identity(full.len());
    let mut changed = false;
    if full.contains("macro_rules!")
        && let Some(e) = expand(&full)?
    {
        text = e.text;
        map = e.map;
        changed = true;
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
    if crate::alias::has_aliases(&text) {
        crate::alias::expand(&mut text, &mut map);
        changed = true;
    }
    Ok(changed.then_some(Preprocessed { text, map, names }))
}

/// The `macro_rules!` definitions this file can use from its `mod x;` files: those marked
/// `#[macro_export]`, and all of a module's when its `mod` has `#[macro_use]`.
pub fn imported_macros(src: &str, filename: &str, resolver: crate::typeck::Resolver) -> String {
    use ra_ap_syntax::ast::{HasAttrs, HasModuleItem};
    if !src.contains("mod ") {
        return String::new();
    }
    let file = SourceFile::parse(src, EDITION).tree();
    let mut out = String::new();
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
        let stem = filename
            .rsplit('/')
            .next()
            .and_then(|f| f.strip_suffix(".rs"))
            .filter(|s| !matches!(*s, "mod" | "main" | "lib"));
        let mut candidates = vec![
            crate::typeck::module_path(filename, &n),
            crate::typeck::module_path(filename, &format!("{n}/mod")),
        ];
        if let Some(st) = stem {
            candidates.push(crate::typeck::module_path(filename, &format!("{st}/{n}")));
        }
        let Some(text) = candidates.iter().find_map(|p| resolver(p)) else {
            continue;
        };
        if !text.contains("macro_rules!") {
            continue;
        }
        let module = SourceFile::parse(&text, EDITION).tree();
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
