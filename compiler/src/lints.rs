//! rustc's warnings: the lints a Rust user sees from `cargo build` (`unused_variables`,
//! `unused_mut`, `unused_imports`, `dead_code`, `unused_assignments`, `unused_must_use`,
//! `unreachable_code`, `unreachable_patterns`, the naming lints, `unused_parens`, `while_true`, `deprecated`),
//! plus `inexact_64bit`, which is this compiler's own: 64-bit integers are exact to 2^53 here.
//!
//! They read the file as written (not the expanded text the checker sees), so their positions
//! are the source's and code the compiler writes is never linted; types and the mutations that
//! needed `mut` come from the checker. Every lint leans towards silence: a warning rustc gives
//! may be missing (a use this pass can't see counts), but one rustc doesn't give never shows.
//!
//! Each lint's level follows `#[allow(..)]` / `#[warn(..)]` / `#[deny(..)]` / `#[forbid(..)]` /
//! `#[expect(..)]` on the items, statements and blocks around it and `#![..]` on the file, by the
//! lint's name, its group (`unused`, `nonstandard_style`) or `warnings`. `rustc_diag::finish`
//! orders the diagnostics as rustc reports them and adds rustc's notes on where levels come from.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ra_ap_syntax::{
    AstNode, SyntaxKind as K, SyntaxNode, SyntaxToken, T, TextSize,
    ast::{self, HasArgList, HasGenericParams, HasLoopBody, HasName, HasVisibility},
};

use crate::codegen::{Child, Diagnostic, SubSpan};
use crate::lineindex::LineIndex;
use crate::ty::{IntKind, Ty};

type Range = (usize, usize);

mod own_attrs;

/// What the lints need from the rest of the compile.
pub struct Cx<'a> {
    /// The checker's types, by range in the text it checked (`map` takes source offsets there).
    pub types: &'a crate::typeck::Types,
    pub map: Option<&'a crate::mbe::OffsetMap>,
    /// Bindings a mutation needed `mut` for, by their name's start in the source.
    pub mut_roots: HashSet<usize>,
    /// Reads the files of the script's `mod x;` declarations.
    pub resolver: crate::typeck::Resolver<'a>,
}

/// The lints for `src` (the file as written).
pub fn check(src: &str, cx: &Cx) -> Vec<Diagnostic> {
    let file = crate::macros::parse_tree(src);
    let mut l = Linter {
        src,
        idx: LineIndex::new(src),
        file,
        cx,
        out: Vec::new(),
        own_mut_methods: HashSet::new(),
        never_fns: HashSet::new(),
        own: Default::default(),
    };
    l.run();
    l.out
}

fn range(n: &SyntaxNode) -> Range {
    let r = n.text_range();
    (usize::from(r.start()), usize::from(r.end()))
}

fn trange(t: &SyntaxToken) -> Range {
    let r = t.text_range();
    (usize::from(r.start()), usize::from(r.end()))
}

/// The lint group a lint belongs to (as `#[allow(..)]` can name it).
fn group_of(lint: &str) -> Option<&'static str> {
    match lint {
        "non_snake_case" | "non_camel_case_types" | "non_upper_case_globals" => {
            Some("nonstandard_style")
        }
        "while_true" | "inexact_64bit" | "deprecated" => None,
        _ => Some("unused"),
    }
}

/// Oxford-comma list of ticked names: "`a`", "`a` and `b`", "`a`, `b`, and `c`".
fn list(names: &[String]) -> String {
    let t: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
    match t.len() {
        0 => String::new(),
        1 => t[0].clone(),
        2 => format!("{} and {}", t[0], t[1]),
        n => format!("{}, and {}", t[..n - 1].join(", "), t[n - 1]),
    }
}

struct Linter<'a> {
    src: &'a str,
    idx: LineIndex<'a>,
    file: ast::SourceFile,
    cx: &'a Cx<'a>,
    out: Vec<Diagnostic>,
    /// Method names the file declares with `&mut self`: a call of one needs its receiver `mut`.
    own_mut_methods: HashSet<String>,
    /// The file's functions declared `-> !`.
    never_fns: HashSet<String>,
    /// The file's own `#[deprecated]` and `#[must_use]` items.
    own: own_attrs::Own,
}

impl<'a> Linter<'a> {
    fn run(&mut self) {
        self.own = own_attrs::Own::collect(self);
        for f in self.file.syntax().descendants().filter_map(ast::Fn::cast) {
            let Some(name) = f.name().map(|n| n.text().to_string()) else { continue };
            if f.param_list().and_then(|p| p.self_param()).is_some_and(|s| {
                s.amp_token().is_some() && s.mut_token().is_some()
                    || s.ty().is_some_and(|t| t.syntax().text().to_string().contains("mut"))
            }) {
                self.own_mut_methods.insert(name.clone());
            }
            if matches!(f.ret_type().and_then(|r| r.ty()), Some(ast::Type::NeverType(_))) {
                self.never_fns.insert(name);
            }
        }
        // rustc's early lints, then the per-function ones, then dead code and the late lints
        // (`rustc_diag::finish` sorts them into rustc's order).
        self.unused_imports();
        self.while_true();
        self.unused_parens();
        self.deprecated();
        self.camel_case();
        self.unreachable_code();
        self.unreachable_patterns();
        self.bodies();
        self.dead_code();
        self.snake_case();
        self.upper_case_globals();
        self.must_use();
        self.inexact_64bit();
    }

    /// Code a normal build leaves out (`#[cfg(test)]`, a false `cfg`), which rustc doesn't lint.
    fn skipped(&self, n: &SyntaxNode) -> bool {
        crate::adt::test_only(n) || crate::macros::under_false_cfg(n)
    }

    /// The lint's level at `at`: `None` when allowed, else the severity and where an attribute
    /// set it (the name's range there and the name written).
    fn level(&self, at: usize, lint: &str) -> Option<(&'static str, Option<(usize, usize, String)>)> {
        let group = group_of(lint);
        let tok = self
            .file
            .syntax()
            .token_at_offset(TextSize::from(at.min(self.src.len()) as u32))
            .right_biased();
        let ancestors: Vec<SyntaxNode> = match tok {
            Some(t) => t.parent_ancestors().collect(),
            None => vec![self.file.syntax().clone()],
        };
        for node in ancestors {
            let attrs: Vec<ast::Attr> = node.children().filter_map(ast::Attr::cast).collect();
            let mut found = None;
            for attr in attrs {
                let Some(path) = attr.path() else { continue };
                let lvl = match path.syntax().text().to_string().as_str() {
                    "allow" | "expect" => "allow",
                    "warn" => "warn",
                    "deny" | "forbid" => "deny",
                    _ => continue,
                };
                let Some(tt) = attr_tt(&attr) else { continue };
                let toks: Vec<SyntaxToken> = tt
                    .syntax()
                    .descendants_with_tokens()
                    .filter_map(|e| e.into_token())
                    .filter(|t| !t.kind().is_trivia())
                    .collect();
                for (i, t) in toks.iter().enumerate() {
                    if t.kind() != K::IDENT {
                        continue;
                    }
                    // (Not a tool's lint, `clippy::x`.)
                    let tool = i > 0 && toks[i - 1].kind() == T![:]
                        || toks.get(i + 1).is_some_and(|n| n.kind() == T![:]);
                    let name = t.text();
                    if !tool && (name == lint || Some(name) == group || name == "warnings") {
                        // (`warnings` only changes the level of lints that would warn.)
                        if name == "warnings" && lvl == "warn" {
                            continue;
                        }
                        let (s, e) = trange(t);
                        found = Some((lvl, (s, e, name.to_string())));
                    }
                }
            }
            if let Some((lvl, at)) = found {
                return match lvl {
                    "allow" => None,
                    "warn" => Some(("warning", Some(at))),
                    _ => Some(("error", Some(at))),
                };
            }
        }
        Some(("warning", None))
    }

    fn diag(&self, start: usize, end: usize, message: impl Into<String>) -> Diagnostic {
        Diagnostic::at(&self.idx, start, end, message.into(), "warning")
    }

    fn span(&self, start: usize, end: usize, label: impl Into<String>) -> SubSpan {
        SubSpan::at(&self.idx, start, end, label.into())
    }

    fn primary(&self, start: usize, end: usize) -> SubSpan {
        let mut s = SubSpan::at(&self.idx, start, end, String::new());
        s.primary = true;
        s
    }

    /// A diagnostic whose primary spans are all of `spans` (the first is the main one).
    fn diag_spans(&self, spans: &[Range], message: impl Into<String>) -> Diagnostic {
        let mut d = self.diag(spans[0].0, spans[0].1, message);
        for &(s, e) in &spans[1..] {
            d.spans.push(self.primary(s, e));
        }
        d
    }

    fn edit_child(&self, message: &str, edits: &[(usize, usize, &str)]) -> Child {
        Child {
            kind: "help".into(),
            message: message.into(),
            spans: Vec::new(),
            edits: edits.iter().map(|&(s, e, t)| self.span(s, e, t)).collect(),
            ..Default::default()
        }
    }

    fn emit(&mut self, lint: &'static str, mut d: Diagnostic) {
        let Some((severity, attr)) = self.level(d.start, lint) else { return };
        d.severity = severity;
        d.lint = Some(lint.into());
        d.lint_attr = attr;
        self.out.push(d);
    }

    /// The checker's type for a source range.
    fn ty(&self, r: Range) -> Option<&Ty> {
        match self.cx.map {
            Some(m) => {
                let (s, e) = (m.cur(r.0)?, m.cur(r.1)?);
                self.cx.types.get(s, e)
            }
            None => self.cx.types.get(r.0, r.1),
        }
    }
}

// ------------------------------------------------------------------------------------ names

/// rustc's `char_has_case`.
fn has_case(c: char) -> bool {
    c.is_lowercase() || c.is_uppercase()
}

/// rustc's `NonCamelCaseTypes::is_camel_case`.
fn is_camel_case(name: &str) -> bool {
    let name = name.trim_matches('_');
    let Some(first) = name.chars().next() else { return true };
    let cs: Vec<char> = name.chars().collect();
    !first.is_lowercase()
        && !name.contains("__")
        && !cs
            .windows(2)
            .any(|w| has_case(w[0]) && w[1] == '_' || has_case(w[1]) && w[0] == '_')
}

/// rustc's `to_camel_case`.
fn to_camel_case(s: &str) -> String {
    let parts: Vec<String> = s
        .trim_matches('_')
        .split('_')
        .filter(|c| !c.is_empty())
        .map(|component| {
            let mut out = String::new();
            let mut new_word = true;
            let mut prev_lower = true;
            for c in component.chars() {
                if prev_lower && c.is_uppercase() {
                    new_word = true;
                }
                if new_word {
                    out.extend(c.to_uppercase());
                } else {
                    out.extend(c.to_lowercase());
                }
                prev_lower = c.is_lowercase();
                new_word = false;
            }
            out
        })
        .collect();
    let mut acc = String::new();
    let mut prev: Option<&String> = None;
    for next in &parts {
        if let Some(p) = prev {
            let l = p.chars().last().unwrap();
            let f = next.chars().next().unwrap();
            if !has_case(l) && !has_case(f) {
                acc.push('_');
            }
        }
        acc.push_str(next);
        prev = Some(next);
    }
    acc
}

/// rustc's `NonSnakeCase::is_snake_case`.
fn is_snake_case(ident: &str) -> bool {
    let ident = ident.trim_start_matches('\'').trim_matches('_');
    if ident.is_empty() {
        return true;
    }
    !ident.contains("__") && !ident.chars().any(char::is_uppercase)
}

/// rustc's `NonSnakeCase::to_snake_case`.
fn to_snake_case(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let rest = name.trim_start_matches(|c: char| {
        if c == '_' {
            words.push(String::new());
            true
        } else {
            false
        }
    });
    for s in rest.split('_') {
        let mut last_upper = false;
        let mut buf = String::new();
        if s.is_empty() {
            continue;
        }
        for ch in s.chars() {
            if !buf.is_empty() && buf != "'" && ch.is_uppercase() && !last_upper {
                words.push(std::mem::take(&mut buf));
            }
            last_upper = ch.is_uppercase();
            buf.extend(ch.to_lowercase());
        }
        words.push(buf);
    }
    words.join("_")
}

impl Linter<'_> {
    /// `non_camel_case_types` (an early lint): types, traits, variants and type parameters.
    fn camel_case(&mut self) {
        let mut found: Vec<(&'static str, ast::Name)> = Vec::new();
        for n in self.file.syntax().descendants() {
            if !NAMED_KINDS.contains(&n.kind()) || self.skipped(&n) {
                continue;
            }
            let item: Option<(&'static str, Option<ast::Name>)> = if let Some(s) = ast::Struct::cast(n.clone()) {
                Some(("type", s.name()))
            } else if let Some(e) = ast::Enum::cast(n.clone()) {
                Some(("type", e.name()))
            } else if let Some(u) = ast::Union::cast(n.clone()) {
                Some(("type", u.name()))
            } else if let Some(t) = ast::TypeAlias::cast(n.clone()) {
                // (Not an associated type in a trait impl: the trait names it.)
                let in_trait_impl = n
                    .ancestors()
                    .find_map(ast::Impl::cast)
                    .is_some_and(|i| i.trait_().is_some());
                (!in_trait_impl).then_some(("type", t.name()))
            } else if let Some(t) = ast::Trait::cast(n.clone()) {
                Some(("trait", t.name()))
            } else if let Some(v) = ast::Variant::cast(n.clone()) {
                Some(("variant", v.name()))
            } else if let Some(p) = ast::TypeParam::cast(n.clone()) {
                Some(("type parameter", p.name()))
            } else {
                None
            };
            if let Some((kind, Some(name))) = item {
                found.push((kind, name));
            }
        }
        for (kind, name) in found {
            let text = name.text().to_string();
            if is_camel_case(&text) {
                continue;
            }
            let camel = to_camel_case(&text);
            let (s, e) = range(name.syntax());
            let mut d = self.diag(s, e, format!("{kind} `{text}` should have an upper camel case name"));
            d.label = Some(format!("help: convert the identifier to upper camel case: `{camel}`"));
            self.emit("non_camel_case_types", d);
        }
    }

    /// `non_snake_case` (a late lint): functions, methods, modules, fields, parameters and
    /// bindings. A trait impl's methods are the trait's to name.
    fn snake_case(&mut self) {
        let mut found: Vec<(&'static str, Range, String)> = Vec::new();
        for n in self.file.syntax().descendants() {
            if !NAMED_KINDS.contains(&n.kind()) || self.skipped(&n) {
                continue;
            }
            if let Some(f) = ast::Fn::cast(n.clone()) {
                let Some(name) = f.name() else { continue };
                let parent_impl = n.parent().and_then(|p| p.parent()).and_then(ast::Impl::cast);
                let in_trait = n.parent().and_then(|p| p.parent()).and_then(ast::Trait::cast).is_some();
                if parent_impl.as_ref().is_some_and(|i| i.trait_().is_some()) {
                    continue;
                }
                let kind = if in_trait {
                    "trait method"
                } else if parent_impl.is_some() {
                    "method"
                } else {
                    "function"
                };
                found.push((kind, range(name.syntax()), name.text().to_string()));
            } else if let Some(m) = ast::Module::cast(n.clone()) {
                if let Some(name) = m.name() {
                    found.push(("module", range(name.syntax()), name.text().to_string()));
                }
            } else if let Some(f) = ast::RecordField::cast(n.clone()) {
                if let Some(name) = f.name() {
                    found.push(("structure field", range(name.syntax()), name.text().to_string()));
                }
            }
        }
        // Parameters and bindings: the ones the body pass declared.
        for (r, name) in self.snake_bindings() {
            found.push(("variable", r, name));
        }
        found.sort_by_key(|(_, r, _)| *r);
        for (kind, (s, e), text) in found {
            if is_snake_case(&text) {
                continue;
            }
            let sc = to_snake_case(&text);
            let notice = if sc == text.to_lowercase() { " (notice the capitalization)" } else { "" };
            let mut d = self.diag(s, e, format!("{kind} `{text}` should have a snake case name"));
            d.label = Some(format!("help: convert the identifier to snake case{notice}: `{sc}`"));
            self.emit("non_snake_case", d);
        }
    }

    /// The bindings `non_snake_case` looks at: parameters and `let` / pattern bindings that
    /// aren't a struct pattern's shorthand field (`S { Field }` names the field).
    fn snake_bindings(&self) -> Vec<(Range, String)> {
        let mut out = Vec::new();
        for ip in self.file.syntax().descendants().filter_map(ast::IdentPat::cast) {
            if self.skipped(ip.syntax()) {
                continue;
            }
            let Some(name) = ip.name() else { continue };
            let text = name.text().to_string();
            if ip.syntax().parent().is_some_and(|p| ast::RecordPatField::cast(p).is_some_and(|f| f.name_ref().is_none())) {
                continue;
            }
            // (A capitalized name in a `match` or `if let` may be a constant or a variant.)
            if text.starts_with(|c: char| c.is_uppercase()) && !binding_position(ip.syntax()) {
                continue;
            }
            out.push((range(name.syntax()), text));
        }
        out
    }

    /// `non_upper_case_globals` (a late lint): `const` and `static` items, and associated
    /// constants outside trait impls.
    fn upper_case_globals(&mut self) {
        let mut found: Vec<(&'static str, ast::Name)> = Vec::new();
        for n in self.file.syntax().descendants() {
            if !NAMED_KINDS.contains(&n.kind()) || self.skipped(&n) {
                continue;
            }
            let assoc = n.parent().and_then(|p| p.parent()).and_then(ast::Impl::cast);
            if assoc.as_ref().is_some_and(|i| i.trait_().is_some()) {
                continue;
            }
            if let Some(c) = ast::Const::cast(n.clone()) {
                let kind = if assoc.is_some() { "associated constant" } else { "constant" };
                if let Some(name) = c.name() {
                    found.push((kind, name));
                }
            } else if let Some(s) = ast::Static::cast(n.clone())
                && let Some(name) = s.name()
            {
                found.push(("static variable", name));
            }
        }
        for (kind, name) in found {
            let text = name.text().to_string();
            if !text.chars().any(char::is_lowercase) {
                continue;
            }
            let upper = to_snake_case(&text).to_uppercase();
            let (s, e) = range(name.syntax());
            let mut d = self.diag(s, e, format!("{kind} `{text}` should have an upper case name"));
            d.label = Some(format!("help: convert the identifier to upper case: `{upper}`"));
            self.emit("non_upper_case_globals", d);
        }
    }
}

/// Whether an identifier pattern is certainly a binding: in a `let`, a parameter (of a
/// function or closure) or a `for` (where a constant or variant can't be).
fn binding_position(n: &SyntaxNode) -> bool {
    for a in n.ancestors().skip(1) {
        match a.kind() {
            K::LET_STMT | K::PARAM | K::FOR_EXPR => return true,
            K::MATCH_ARM | K::LET_EXPR => return false,
            k if ast::Pat::can_cast(k) => continue,
            K::RECORD_PAT_FIELD | K::RECORD_PAT_FIELD_LIST | K::PARAM_LIST => continue,
            _ => return false,
        }
    }
    false
}

// ------------------------------------------------------------------------------ early lints

/// Methods of std traits that must be imported to be called: the import is used when one of
/// them (or the trait's name) appears. Traits not listed count as used (the prelude's, and
/// the ones this table doesn't know).
const TRAIT_METHODS: &[(&str, &[&str])] = &[
    ("Write", &["write", "writeln", "write_str", "write_char", "write_fmt", "write_all", "flush", "by_ref"]),
    ("Read", &["read", "read_to_string", "read_to_end", "read_exact", "bytes", "take", "chain", "by_ref"]),
    ("BufRead", &["lines", "read_line", "split", "read_until", "fill_buf", "consume"]),
    ("Seek", &["seek", "rewind", "stream_position"]),
    ("FromStr", &["from_str"]),
    ("Hash", &["hash"]),
    ("Hasher", &["finish", "write", "write_u8", "write_u32", "write_u64", "write_usize"]),
    ("BuildHasher", &["build_hasher", "hash_one"]),
    ("Display", &[]),
    ("Debug", &[]),
    ("Error", &["source", "description", "cause"]),
    ("Sum", &["sum"]),
    ("Product", &["product"]),
    ("Borrow", &["borrow"]),
    ("BorrowMut", &["borrow_mut"]),
];

/// Std traits in the prelude (importing one is never reported here) and others whose methods
/// this pass doesn't track.
const OTHER_STD_TRAITS: &[&str] = &[
    "Iterator", "IntoIterator", "DoubleEndedIterator", "ExactSizeIterator", "Extend", "FromIterator",
    "Clone", "Copy", "Default", "PartialEq", "Eq", "PartialOrd", "Ord", "ToString", "ToOwned", "From",
    "Into", "TryFrom", "TryInto", "AsRef", "AsMut", "Drop", "Fn", "FnMut", "FnOnce", "Send", "Sync",
    "Sized", "Unpin", "Any", "Future", "IntoFuture", "Add", "Sub", "Mul", "Div", "Rem", "Neg", "Not",
    "BitAnd", "BitOr", "BitXor", "Shl", "Shr", "AddAssign", "SubAssign", "MulAssign", "DivAssign",
    "RemAssign", "BitAndAssign", "BitOrAssign", "BitXorAssign", "ShlAssign", "ShrAssign", "Index",
    "IndexMut", "Deref", "DerefMut", "RangeBounds", "Termination", "Octal", "Binary", "LowerHex",
    "UpperHex", "LowerExp", "UpperExp", "Pointer", "SliceIndex", "Step", "Pattern", "Wake",
    "OsStrExt", "OsStringExt", "PermissionsExt", "MetadataExt", "FileExt", "CommandExt",
];

impl Linter<'_> {
    /// The identifiers of the file outside `use` items and the code a normal build leaves out:
    /// the names a `use` can be used by (in macro calls too).
    fn used_idents(&self) -> HashSet<String> {
        fn walk(l: &Linter, n: &SyntaxNode, out: &mut HashSet<String>) {
            for el in n.children_with_tokens() {
                match el {
                    ra_ap_syntax::NodeOrToken::Token(t) => {
                        if t.kind() == K::IDENT {
                            out.insert(t.text().trim_start_matches("r#").to_string());
                        }
                    }
                    ra_ap_syntax::NodeOrToken::Node(c) => {
                        if c.kind() == K::USE || (ast::Item::can_cast(c.kind()) && l.skipped(&c)) {
                            continue;
                        }
                        walk(l, &c, out);
                    }
                }
            }
        }
        let mut out = HashSet::new();
        walk(self, self.file.syntax(), &mut out);
        out
    }

    /// The text of the script's `mod x;` files (for traits declared there).
    fn module_texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        for m in self.file.syntax().descendants().filter_map(ast::Module::cast) {
            if m.item_list().is_some() {
                continue;
            }
            let Some(name) = m.name().map(|n| n.text().to_string()) else { continue };
            for p in [format!("{name}.rs"), format!("{name}/mod.rs")] {
                if let Some(t) = (self.cx.resolver)(&p) {
                    out.push(t);
                    break;
                }
            }
        }
        out
    }

    /// `unused_imports`: a name a `use` brings in that nothing uses.
    fn unused_imports(&mut self) {
        let uses: Vec<ast::Use> = self
            .file
            .syntax()
            .descendants()
            .filter_map(ast::Use::cast)
            .filter(|u| u.visibility().is_none() && !self.skipped(u.syntax()))
            .collect();
        if uses.is_empty() {
            return;
        }
        let idents = self.used_idents();
        let modules = self.module_texts();
        // Traits the script declares, with their methods.
        let mut own_traits: HashMap<String, Vec<String>> = HashMap::new();
        let trees: Vec<ast::SourceFile> = modules
            .iter()
            .map(|t| ast::SourceFile::parse(t, crate::macros::EDITION).tree())
            .collect();
        for f in std::iter::once(&self.file).chain(&trees) {
            for t in f.syntax().descendants().filter_map(ast::Trait::cast) {
                let Some(n) = t.name() else { continue };
                let methods = t
                    .assoc_item_list()
                    .map(|l| {
                        l.assoc_items()
                            .filter_map(|i| match i {
                                ast::AssocItem::Fn(f) => f.name().map(|n| n.text().to_string()),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                own_traits.insert(n.text().to_string(), methods);
            }
        }
        let local_mods: HashSet<String> = self
            .file
            .syntax()
            .descendants()
            .filter_map(ast::Module::cast)
            .filter_map(|m| m.name().map(|n| n.text().to_string()))
            .collect();
        for u in uses {
            let Some(tree) = u.use_tree() else { continue };
            let mut leaves = Vec::new();
            use_leaves(&tree, &[], &mut leaves);
            let mut unused: Vec<(Range, String)> = Vec::new();
            for (leaf, path, name) in leaves {
                if idents.contains(&name) {
                    continue;
                }
                let first = path.first().map(String::as_str).unwrap_or("");
                // (A trait is known by its own name, whatever `as` calls it.)
                let item = path.last().filter(|l| *l != "self").unwrap_or(&name);
                let used = if matches!(first, "std" | "core" | "alloc") {
                    if let Some((_, methods)) = TRAIT_METHODS.iter().find(|(t, _)| t == item) {
                        methods.iter().any(|m| idents.contains(*m))
                    } else {
                        OTHER_STD_TRAITS.contains(&item.as_str())
                    }
                } else if matches!(first, "crate" | "self" | "super") || local_mods.contains(first) {
                    match own_traits.get(item) {
                        Some(methods) => methods.iter().any(|m| idents.contains(m)),
                        None => false,
                    }
                } else {
                    // Another crate's: a capitalized name may be a trait used for its methods.
                    item.starts_with(|c: char| c.is_uppercase()) || own_traits.contains_key(item)
                };
                if !used {
                    unused.push((range(leaf.syntax()), leaf.syntax().text().to_string()));
                }
            }
            if unused.is_empty() {
                continue;
            }
            unused.sort_by_key(|(r, _)| *r);
            let mut names: Vec<String> = unused.iter().map(|(_, n)| n.clone()).collect();
            names.sort();
            let message = if names.len() == 1 {
                format!("unused import: {}", list(&names))
            } else {
                format!("unused imports: {}", list(&names))
            };
            let spans: Vec<Range> = unused.iter().map(|(r, _)| *r).collect();
            let d = self.diag_spans(&spans, message);
            self.emit("unused_imports", d);
        }
    }

    /// `while_true`: `while true { .. }`.
    fn while_true(&mut self) {
        let found: Vec<Range> = self
            .file
            .syntax()
            .descendants()
            .filter_map(ast::WhileExpr::cast)
            .filter(|w| !self.skipped(w.syntax()))
            .filter_map(|w| {
                let c = w.condition()?;
                let ast::Expr::Literal(l) = &c else { return None };
                (l.syntax().text() == "true").then(|| (range(w.syntax()).0, range(c.syntax()).1))
            })
            .collect();
        for (s, e) in found {
            let mut d = self.diag(s, e, "denote infinite loops with `loop { ... }`");
            d.label = Some("help: use `loop`".into());
            self.emit("while_true", d);
        }
    }

    /// `unused_parens`: parentheses around a whole value where rustc doesn't need them.
    fn unused_parens(&mut self) {
        let mut found: Vec<(Range, &'static str)> = Vec::new();
        for p in self.file.syntax().descendants().filter_map(ast::ParenExpr::cast) {
            let Some(inner) = p.expr() else { continue };
            let Some(ctx) = paren_context(&p) else { continue };
            let r = range(p.syntax());
            if self.src[r.0..r.1].contains('\n') || self.skipped(p.syntax()) {
                continue;
            }
            // (`if (S { a } == s)`: without them the `{` would open the block.)
            let block_ctx = matches!(
                ctx,
                "`if` condition" | "`while` condition" | "`match` scrutinee expression" | "`for` iterator expression"
            );
            if block_ctx && inner.syntax().descendants().any(|n| n.kind() == K::RECORD_EXPR) {
                continue;
            }
            found.push((r, ctx));
        }
        for ((s, e), ctx) in found {
            let bytes = self.src.as_bytes();
            let word = |b: Option<&u8>| b.is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_');
            let left = if s > 0 && word(bytes.get(s - 1)) { " " } else { "" };
            let right = if word(bytes.get(e)) { " " } else { "" };
            let mut d = self.diag_spans(&[(s, s + 1), (e - 1, e)], format!("unnecessary parentheses around {ctx}"));
            d.children.push(self.edit_child("remove these parentheses", &[(s, s + 1, left), (e - 1, e, right)]));
            self.emit("unused_parens", d);
        }
    }
}

/// A `use` tree's leaves: each with its path and the name it brings in (`self` in a list
/// brings in its parent's name). Globs and `as _` aren't leaves.
fn use_leaves(t: &ast::UseTree, prefix: &[String], out: &mut Vec<(ast::UseTree, Vec<String>, String)>) {
    let mut path: Vec<String> = prefix.to_vec();
    if let Some(p) = t.path() {
        for seg in p.syntax().descendants().filter_map(ast::PathSegment::cast) {
            path.push(seg.syntax().text().to_string());
        }
    }
    if let Some(l) = t.use_tree_list() {
        for sub in l.use_trees() {
            use_leaves(&sub, &path, out);
        }
        return;
    }
    if t.star_token().is_some() {
        return;
    }
    let name = match t.rename() {
        Some(r) => match r.name() {
            Some(n) => n.text().to_string(),
            None => return,
        },
        None => {
            let last = path.last().cloned().unwrap_or_default();
            if last == "self" {
                match path.len().checked_sub(2).and_then(|i| path.get(i)) {
                    Some(p) => p.clone(),
                    None => return,
                }
            } else {
                last
            }
        }
    };
    if name.is_empty() || name == "_" {
        return;
    }
    out.push((t.clone(), path, name));
}

/// Where rustc's `unused_parens` looks: what the parenthesized value is, in its words.
fn paren_context(p: &ast::ParenExpr) -> Option<&'static str> {
    let parent = p.syntax().parent()?;
    let is = |e: Option<ast::Expr>| e.is_some_and(|e| e.syntax() == p.syntax());
    if let Some(l) = ast::LetStmt::cast(parent.clone()) {
        return (is(l.initializer()) && l.let_else().is_none()).then_some("assigned value");
    }
    if let Some(b) = ast::BinExpr::cast(parent.clone()) {
        let assign = matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { op: None }));
        return (assign && is(b.rhs())).then_some("assigned value");
    }
    if let Some(i) = ast::IfExpr::cast(parent.clone()) {
        return is(i.condition()).then_some("`if` condition");
    }
    if let Some(w) = ast::WhileExpr::cast(parent.clone()) {
        return is(w.condition()).then_some("`while` condition");
    }
    if let Some(m) = ast::MatchExpr::cast(parent.clone()) {
        return is(m.expr()).then_some("`match` scrutinee expression");
    }
    if let Some(f) = ast::ForExpr::cast(parent.clone()) {
        return is(f.iterable()).then_some("`for` iterator expression");
    }
    if ast::ReturnExpr::can_cast(parent.kind()) {
        return Some("`return` value");
    }
    if ast::ArgList::can_cast(parent.kind()) {
        let call = parent.parent()?;
        if ast::CallExpr::can_cast(call.kind()) {
            return Some("function argument");
        }
        if ast::MethodCallExpr::can_cast(call.kind()) {
            return Some("method argument");
        }
    }
    None
}

// ------------------------------------------------------------------------- unreachable code

/// The panicking macros: nothing after one runs.
fn diverging_macro(name: &str) -> bool {
    matches!(name, "panic" | "unreachable" | "todo" | "unimplemented")
}

impl Linter<'_> {
    /// Whether evaluating `e` never finishes (it returns, breaks, panics or loops forever).
    fn diverges(&self, e: &ast::Expr) -> bool {
        match e {
            ast::Expr::ReturnExpr(_) | ast::Expr::BreakExpr(_) | ast::Expr::ContinueExpr(_) => true,
            ast::Expr::MacroExpr(m) => m
                .macro_call()
                .and_then(|mc| crate::macros::macro_name(&mc))
                .is_some_and(|n| diverging_macro(&n)),
            ast::Expr::CallExpr(c) => {
                let Some(ast::Expr::PathExpr(p)) = c.expr() else { return false };
                let text: String = p.syntax().text().to_string().split_whitespace().collect();
                text.ends_with("process::exit")
                    || text.ends_with("process::abort")
                    || self.never_fns.contains(&text)
            }
            ast::Expr::ParenExpr(p) => p.expr().is_some_and(|i| self.diverges(&i)),
            ast::Expr::BlockExpr(b) => {
                if b.async_token().is_some() || b.const_token().is_some() || b.label().is_some() {
                    return false;
                }
                let Some(list) = b.stmt_list() else { return false };
                list.statements().any(|s| match s {
                    ast::Stmt::ExprStmt(es) => es.expr().is_some_and(|x| self.diverges(&x)),
                    _ => false,
                }) || list.tail_expr().is_some_and(|t| self.diverges(&t))
            }
            ast::Expr::IfExpr(i) => {
                let then = i.then_branch().is_some_and(|b| self.diverges(&ast::Expr::BlockExpr(b)));
                let els = match i.else_branch() {
                    Some(ast::ElseBranch::Block(b)) => self.diverges(&ast::Expr::BlockExpr(b)),
                    Some(ast::ElseBranch::IfExpr(e)) => self.diverges(&ast::Expr::IfExpr(e)),
                    None => false,
                };
                then && els
            }
            ast::Expr::MatchExpr(m) => {
                let arms: Vec<ast::MatchArm> = m.match_arm_list().map(|l| l.arms().collect()).unwrap_or_default();
                !arms.is_empty() && arms.iter().all(|a| a.expr().is_some_and(|x| self.diverges(&x)))
            }
            ast::Expr::LoopExpr(l) => !loop_breaks(l),
            _ => false,
        }
    }

    /// `unreachable_code`: the first statement after one that never finishes.
    fn unreachable_code(&mut self) {
        let mut found: Vec<(Range, Range, bool)> = Vec::new();
        for list in self.file.syntax().descendants().filter_map(ast::StmtList::cast) {
            if self.skipped(list.syntax()) {
                continue;
            }
            let stmts: Vec<ast::Stmt> = list.statements().collect();
            for (i, s) in stmts.iter().enumerate() {
                let ast::Stmt::ExprStmt(es) = s else { continue };
                let Some(e) = es.expr() else { continue };
                if !self.diverges(&e) {
                    continue;
                }
                let next = stmts[i + 1..].iter().find_map(|n| match n {
                    ast::Stmt::Item(_) => None,
                    ast::Stmt::LetStmt(l) => Some((range(l.syntax()), true)),
                    ast::Stmt::ExprStmt(x) => x.expr().map(|x| (range(x.syntax()), true)),
                });
                let next = next.or_else(|| list.tail_expr().map(|t| (range(t.syntax()), false)));
                if let Some((r, stmt)) = next {
                    found.push((r, range(e.syntax()), stmt));
                }
                break;
            }
        }
        for (r, cause, stmt) in found {
            let what = if stmt { "unreachable statement" } else { "unreachable expression" };
            let mut d = self.diag(r.0, r.1, what);
            d.label = Some(what.into());
            d.spans.push(self.span(cause.0, cause.1, "any code following this expression is unreachable"));
            self.emit("unreachable_code", d);
        }
    }
}

/// Whether a `break` leaves the loop `l` (not one of a loop or closure inside it).
fn loop_breaks(l: &ast::LoopExpr) -> bool {
    let label = l.label().and_then(|x| x.lifetime()).map(|x| x.text().to_string());
    fn walk(n: &SyntaxNode, depth: usize, label: &Option<String>) -> bool {
        for c in n.children() {
            if ast::ClosureExpr::can_cast(c.kind()) || ast::Item::can_cast(c.kind()) {
                continue;
            }
            if let Some(b) = ast::BreakExpr::cast(c.clone()) {
                match b.lifetime() {
                    Some(lt) => {
                        if Some(lt.text().to_string()) == *label {
                            return true;
                        }
                    }
                    None if depth == 0 => return true,
                    None => {}
                }
            }
            let inner = matches!(c.kind(), K::LOOP_EXPR | K::WHILE_EXPR | K::FOR_EXPR);
            if walk(&c, depth + inner as usize, label) {
                return true;
            }
        }
        false
    }
    walk(l.syntax(), 0, &label)
}

// --------------------------------------------------------------------- unreachable patterns

/// A pattern as far as `unreachable_patterns` reads it.
#[derive(Clone, Debug, PartialEq)]
enum Pat {
    /// `_` (`true`) or a binding (`false`).
    Any(bool),
    Lit(String),
    /// A variant, unit struct or constant by its last path segment, with its fields (`None`
    /// for `V(..)`, or a record pattern of bindings).
    Ctor(String, Option<Vec<Pat>>),
    Tuple(Vec<Pat>),
    Or(Vec<Pat>),
    Unknown,
}

fn read_pat(p: &ast::Pat) -> Pat {
    let last_seg = |path: Option<ast::Path>| {
        path.and_then(|p| p.segment())
            .map(|s| s.syntax().text().to_string())
            .unwrap_or_default()
    };
    match p {
        ast::Pat::WildcardPat(_) => Pat::Any(true),
        ast::Pat::IdentPat(ip) => {
            if ip.pat().is_some() {
                return Pat::Unknown;
            }
            let Some(n) = ip.name() else { return Pat::Unknown };
            let t = n.text().to_string();
            if t.starts_with(|c: char| c.is_uppercase()) {
                Pat::Ctor(t, Some(Vec::new()))
            } else {
                Pat::Any(false)
            }
        }
        ast::Pat::LiteralPat(l) => Pat::Lit(l.syntax().text().to_string().split_whitespace().collect()),
        ast::Pat::PathPat(pp) => Pat::Ctor(last_seg(pp.path()), Some(Vec::new())),
        ast::Pat::TupleStructPat(tp) => {
            let fields: Vec<ast::Pat> = tp.fields().collect();
            if fields.iter().any(|f| matches!(f, ast::Pat::RestPat(_))) {
                if fields.len() == 1 {
                    return Pat::Ctor(last_seg(tp.path()), None);
                }
                return Pat::Unknown;
            }
            Pat::Ctor(last_seg(tp.path()), Some(fields.iter().map(read_pat).collect()))
        }
        ast::Pat::RecordPat(rp) => {
            let Some(list) = rp.record_pat_field_list() else { return Pat::Unknown };
            let all_any = list
                .fields()
                .all(|f| f.pat().is_some_and(|p| matches!(read_pat(&p), Pat::Any(_))));
            if all_any { Pat::Ctor(last_seg(rp.path()), None) } else { Pat::Unknown }
        }
        ast::Pat::TuplePat(tp) => {
            let fields: Vec<ast::Pat> = tp.fields().collect();
            if fields.iter().any(|f| matches!(f, ast::Pat::RestPat(_))) {
                return Pat::Unknown;
            }
            Pat::Tuple(fields.iter().map(read_pat).collect())
        }
        ast::Pat::OrPat(op) => Pat::Or(op.pats().map(|p| read_pat(&p)).collect()),
        ast::Pat::ParenPat(pp) => pp.pat().map(|p| read_pat(&p)).unwrap_or(Pat::Unknown),
        ast::Pat::RefPat(rp) => rp.pat().map(|p| read_pat(&p)).unwrap_or(Pat::Unknown),
        _ => Pat::Unknown,
    }
}

fn has_unknown(p: &Pat) -> bool {
    match p {
        Pat::Unknown => true,
        Pat::Ctor(_, Some(fs)) | Pat::Tuple(fs) | Pat::Or(fs) => fs.iter().any(has_unknown),
        _ => false,
    }
}

/// Whether `a` matches every value `b` does.
fn covers(a: &Pat, b: &Pat) -> bool {
    match (a, b) {
        (Pat::Any(_), _) => true,
        (_, Pat::Or(bs)) => bs.iter().all(|b| covers(a, b)),
        (Pat::Or(xs), _) => xs.iter().any(|a| covers(a, b)),
        (Pat::Lit(x), Pat::Lit(y)) => x == y,
        (Pat::Ctor(n, fa), Pat::Ctor(m, fb)) => {
            n == m
                && match (fa, fb) {
                    (None, _) => true,
                    (Some(fa), None) => fa.iter().all(|f| matches!(f, Pat::Any(_))),
                    (Some(fa), Some(fb)) => {
                        fa.len() == fb.len() && fa.iter().zip(fb).all(|(a, b)| covers(a, b))
                    }
                }
        }
        (Pat::Tuple(xs), Pat::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(a, b)| covers(a, b))
        }
        _ => false,
    }
}

/// Whether some value matches both.
fn intersects(a: &Pat, b: &Pat) -> bool {
    match (a, b) {
        (Pat::Any(_), _) | (_, Pat::Any(_)) => true,
        (Pat::Or(xs), _) => xs.iter().any(|x| intersects(x, b)),
        (_, Pat::Or(ys)) => ys.iter().any(|y| intersects(a, y)),
        (Pat::Lit(x), Pat::Lit(y)) => x == y,
        (Pat::Ctor(n, fa), Pat::Ctor(m, fb)) => {
            n == m
                && match (fa, fb) {
                    (Some(fa), Some(fb)) => {
                        fa.len() != fb.len() || fa.iter().zip(fb).all(|(a, b)| intersects(a, b))
                    }
                    _ => true,
                }
        }
        (Pat::Tuple(xs), Pat::Tuple(ys)) => {
            xs.len() != ys.len() || xs.iter().zip(ys).all(|(a, b)| intersects(a, b))
        }
        _ => false,
    }
}

impl Linter<'_> {
    /// The constructors of the type `name` is one of: `Some`/`None`, `Ok`/`Err`, `true` /
    /// `false`, or an enum of the file's (when only one has a variant of that name).
    fn ctor_set(&self, name: &str) -> Option<Vec<String>> {
        let std: &[&[&str]] = &[&["Some", "None"], &["Ok", "Err"], &["true", "false"]];
        if let Some(s) = std.iter().find(|s| s.contains(&name)) {
            return Some(s.iter().map(|x| x.to_string()).collect());
        }
        let mut found = None;
        for e in self.file.syntax().descendants().filter_map(ast::Enum::cast) {
            let vs: Vec<String> = e
                .variant_list()
                .map(|l| {
                    l.variants()
                        .filter_map(|v| v.name().map(|n| n.text().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if vs.iter().any(|v| v == name) {
                if found.is_some() {
                    return None;
                }
                found = Some(vs);
            }
        }
        found
    }

    /// Whether the patterns together match every value (their constructors cover the type).
    fn exhausts(&self, pats: &[&Pat]) -> bool {
        let mut names: HashSet<String> = HashSet::new();
        let mut add = |p: &Pat| match p {
            Pat::Ctor(n, f)
                if f.as_ref().is_none_or(|f| f.iter().all(|x| matches!(x, Pat::Any(_)))) =>
            {
                names.insert(n.clone());
            }
            Pat::Lit(l) if l == "true" || l == "false" => {
                names.insert(l.clone());
            }
            _ => {}
        };
        for p in pats {
            match p {
                Pat::Or(alts) => alts.iter().for_each(&mut add),
                p => add(p),
            }
        }
        let Some(any) = names.iter().next() else { return false };
        self.ctor_set(any)
            .is_some_and(|set| set.iter().all(|c| names.contains(c)))
    }

    /// `unreachable_patterns`: a `match` arm no value can reach, because an earlier arm takes
    /// all its values, or earlier arms together take every value.
    fn unreachable_patterns(&mut self) {
        let mut found: Vec<Diagnostic> = Vec::new();
        for m in self.file.syntax().descendants().filter_map(ast::MatchExpr::cast) {
            if self.skipped(m.syntax()) {
                continue;
            }
            let arms: Vec<ast::MatchArm> = m
                .match_arm_list()
                .map(|l| l.arms().collect())
                .unwrap_or_default();
            let pats: Vec<Option<(ast::Pat, Pat)>> = arms
                .iter()
                .map(|a| a.pat().map(|p| { let r = read_pat(&p); (p, r) }))
                .collect();
            if pats.iter().any(|p| p.as_ref().is_none_or(|(_, r)| has_unknown(r))) {
                continue;
            }
            let pats: Vec<(ast::Pat, Pat)> = pats.into_iter().flatten().collect();
            let mut reachable: Vec<usize> = Vec::new();
            for (j, arm) in arms.iter().enumerate() {
                let (node, pj) = &pats[j];
                let guarded = arm.guard().is_some();
                if matches!(pj, Pat::Or(_)) {
                    if !guarded {
                        reachable.push(j);
                    }
                    continue;
                }
                let covered = reachable.iter().any(|&i| covers(&pats[i].1, pj))
                    || (matches!(pj, Pat::Any(_)) && {
                        let ps: Vec<&Pat> = reachable.iter().map(|&i| &pats[i].1).collect();
                        self.exhausts(&ps)
                    });
                if !covered {
                    if !guarded {
                        reachable.push(j);
                    }
                    continue;
                }
                let inter: Vec<usize> = reachable
                    .iter()
                    .copied()
                    .filter(|&i| intersects(&pats[i].1, pj))
                    .collect();
                let (s, e) = range(node.syntax());
                let mut d = self.diag(s, e, "unreachable pattern");
                d.label = Some("no value can reach this".into());
                if let [i] = inter.as_slice() {
                    let (a, b) = range(pats[*i].0.syntax());
                    let label = if matches!(pats[*i].1, Pat::Any(_)) {
                        "matches any value"
                    } else {
                        "matches all the relevant values"
                    };
                    d.spans.push(self.span(a, b, label));
                } else {
                    let mut spans: Vec<SubSpan> = inter
                        .iter()
                        .map(|&i| {
                            let (a, b) = range(pats[i].0.syntax());
                            self.span(a, b, "matches some of the same values")
                        })
                        .collect();
                    let mut p = self.span(s, e, "collectively making this unreachable");
                    p.primary = true;
                    spans.push(p);
                    d.children.push(Child {
                        kind: "note".into(),
                        message: "multiple earlier patterns match some of the same values".into(),
                        spans,
                        edits: Vec::new(),
                        ..Default::default()
                    });
                }
                found.push(d);
            }
        }
        for d in found {
            self.emit("unreachable_patterns", d);
        }
    }
}

// ---------------------------------------------------------------------------- variables

/// Methods that never need their receiver `mut` (they take `&self` or `self`), unless the file
/// declares one of the same name with `&mut self`.
const SHARED_METHODS: &[&str] = &[
    "len", "is_empty", "iter", "get", "contains", "contains_key", "clone", "to_string", "to_owned",
    "as_str", "as_ref", "as_slice", "as_bytes", "chars", "bytes", "char_indices", "keys", "values",
    "first", "last", "starts_with", "ends_with", "split", "split_whitespace", "lines", "trim",
    "trim_start", "trim_end", "find", "rfind", "to_uppercase", "to_lowercase", "to_vec", "join",
    "concat", "parse", "eq", "ne", "cmp", "partial_cmp", "abs", "pow", "min", "max", "sqrt",
    "floor", "ceil", "round", "is_some", "is_none", "is_ok", "is_err", "unwrap", "expect",
    "unwrap_or", "unwrap_or_default", "unwrap_or_else", "map", "and_then", "ok", "err", "ok_or",
    "copied", "cloned", "windows", "chunks", "binary_search", "into_iter", "count_ones", "repeat",
    "replace", "contains_char", "get_key_value", "range", "is_subset", "is_superset", "len_utf8",
    "is_alphabetic", "is_numeric", "is_alphanumeric", "is_whitespace", "is_ascii_digit",
    "to_digit", "fmt", "hash", "iter_rev", "split_at", "into_bytes", "into_boxed_slice",
    "difference", "intersection", "union", "symmetric_difference", "peek_first", "borrow",
    "upgrade", "downgrade", "type_id", "into_inner", "as_deref", "to_le_bytes", "to_be_bytes",
];

/// A binding the body pass follows.
struct Var {
    name: String,
    /// The binding's name.
    decl: Range,
    /// What `unused_variables` underlines: `mut x` or `x`.
    span: Range,
    /// `mut ` up to the name, for `unused_mut`.
    mut_kw: Option<Range>,
    /// `S { x }`: a struct pattern's shorthand field.
    shorthand: bool,
    /// `let x;`: assigned later.
    deferred: bool,
    /// Whether it may be reported at all (not `_x`, an or-pattern's, unreachable, ..).
    report: bool,
    used: bool,
    /// Assigned after its declaration.
    assigned: bool,
    needs_mut: bool,
    /// Its value may be read or changed where this pass doesn't follow (a closure captures
    /// it, a `&mut` to it is taken, an unknown macro names it): its assignments aren't reported.
    escaped: bool,
}

/// A value assigned to a binding.
struct Def {
    var: usize,
    span: Range,
    /// An assignment's left side, and whether its value is a literal (for the span rustc gives
    /// a type with drop glue: the left side).
    lhs: Option<(Range, bool)>,
    init: bool,
    read: bool,
    /// The assignments that replaced it before a read: their spans and left sides.
    killers: Vec<(Range, Option<(Range, bool)>)>,
    /// It may still be unread where the function returns.
    exits: bool,
}

/// The values of each binding that may reach a point (`dead` past a `return` or a panic).
#[derive(Clone, Default, PartialEq)]
struct Flow {
    defs: BTreeMap<usize, BTreeSet<usize>>,
    dead: bool,
}

impl Flow {
    fn dead() -> Flow {
        Flow { defs: BTreeMap::new(), dead: true }
    }

    fn join(a: Flow, b: Flow) -> Flow {
        if a.dead {
            return b;
        }
        if b.dead {
            return a;
        }
        let mut defs = a.defs;
        for (v, ds) in b.defs {
            defs.entry(v).or_default().extend(ds);
        }
        Flow { defs, dead: false }
    }
}

struct LoopCx {
    label: Option<String>,
    is_loop: bool,
    breaks: Flow,
    continues: Flow,
}

/// The format-style macros: their arguments are expressions, and string arguments may name
/// bindings (`{x}`).
fn format_macro(name: &str) -> bool {
    matches!(
        name,
        "println" | "print" | "eprintln" | "eprint" | "format" | "write" | "writeln" | "panic"
            | "format_args" | "assert" | "assert_eq" | "assert_ne" | "debug_assert"
            | "debug_assert_eq" | "debug_assert_ne" | "unreachable" | "todo" | "unimplemented"
            | "dbg" | "vec" | "matches"
    )
}

struct Body<'l, 'a> {
    l: &'l Linter<'a>,
    vars: Vec<Var>,
    by_decl: HashMap<Range, usize>,
    defs: Vec<Def>,
    def_at: HashMap<(usize, Range), usize>,
    scopes: Vec<HashMap<String, usize>>,
    flow: Flow,
    loops: Vec<LoopCx>,
    /// The first binding of each closure being walked: the ones before it are captured.
    closures: Vec<usize>,
    /// Offset of the text the current nodes were parsed from (a macro argument's).
    base: usize,
}

impl Body<'_, '_> {
    fn range(&self, n: &SyntaxNode) -> Range {
        let (s, e) = range(n);
        (s + self.base, e + self.base)
    }

    fn lookup(&self, name: &str) -> Option<usize> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }

    fn captured(&self, id: usize) -> bool {
        self.closures.last().is_some_and(|&first| id < first)
    }

    fn read(&mut self, id: usize) {
        self.vars[id].used = true;
        self.read_value(id);
    }

    /// The binding's current value is read (`x += 1` reads it without using the binding).
    fn read_value(&mut self, id: usize) {
        if self.captured(id) {
            self.vars[id].escaped = true;
            self.vars[id].used = true;
        }
        if self.flow.dead {
            return;
        }
        if let Some(ds) = self.flow.defs.get(&id) {
            for &d in ds {
                self.defs[d].read = true;
            }
        }
    }

    fn write(&mut self, id: usize, span: Range, lhs: Option<(Range, bool)>, init: bool) {
        if self.captured(id) {
            self.vars[id].escaped = true;
            self.vars[id].used = true;
        }
        if !init {
            self.vars[id].assigned = true;
        }
        if self.flow.dead {
            return;
        }
        let d = *self.def_at.entry((id, span)).or_insert_with(|| {
            self.defs.push(Def { var: id, span, lhs, init, read: false, killers: Vec::new(), exits: false });
            self.defs.len() - 1
        });
        let old: Vec<usize> = self.flow.defs.get(&id).map(|s| s.iter().copied().collect()).unwrap_or_default();
        for o in old {
            if o != d && !self.defs[o].killers.iter().any(|k| k.0 == span) {
                self.defs[o].killers.push((span, lhs));
            }
        }
        self.flow.defs.insert(id, BTreeSet::from([d]));
    }

    /// The function may return here: the values still unread stay unread.
    fn exit(&mut self) {
        if self.flow.dead {
            return;
        }
        for ds in self.flow.defs.values() {
            for &d in ds {
                self.defs[d].exits = true;
            }
        }
    }

    // ------------------------------------------------------------------------ bindings

    fn declare(&mut self, ip: &ast::IdentPat, in_or: bool) -> Option<usize> {
        let name = ip.name()?;
        let text = name.text().to_string();
        let decl = self.range(name.syntax());
        let mut_kw = ip.mut_token().map(|t| (trange(&t).0 + self.base, decl.0));
        let span = (mut_kw.map_or(decl.0, |m| m.0), decl.1);
        let shorthand = ip
            .syntax()
            .parent()
            .and_then(ast::RecordPatField::cast)
            .is_some_and(|f| f.name_ref().is_none());
        let report = !text.starts_with('_') && self.base == 0 && !self.flow.dead && !in_or;
        let id = match self.by_decl.get(&decl) {
            Some(&id) => id,
            None => {
                self.vars.push(Var {
                    name: text.clone(),
                    decl,
                    span,
                    mut_kw,
                    shorthand,
                    deferred: false,
                    report,
                    used: false,
                    assigned: false,
                    needs_mut: false,
                    escaped: false,
                });
                self.by_decl.insert(decl, self.vars.len() - 1);
                self.vars.len() - 1
            }
        };
        self.scopes.last_mut().unwrap().insert(text, id);
        Some(id)
    }

    /// Declares a pattern's bindings; whether it binds by `ref mut` (its value needs `mut`).
    fn bind(&mut self, p: &ast::Pat) -> bool {
        let mut ref_mut = false;
        let in_or = p.syntax().descendants().any(|n| n.kind() == K::OR_PAT);
        for ip in p.syntax().descendants().filter_map(ast::IdentPat::cast) {
            let text = ip.name().map(|n| n.text().to_string()).unwrap_or_default();
            // (A capitalized name in a `match` may be a constant or a unit variant.)
            if text.starts_with(|c: char| c.is_uppercase()) && !binding_position(ip.syntax()) {
                continue;
            }
            if ip.ref_token().is_some() && ip.mut_token().is_some() {
                ref_mut = true;
            }
            self.declare(&ip, in_or);
        }
        // (A macro in a pattern: whatever it names may be read.)
        for mc in p.syntax().descendants().filter_map(ast::MacroCall::cast) {
            self.opaque(&mc);
        }
        ref_mut
    }

    /// The binding a place expression is rooted at, and whether it goes through a `*`.
    fn place_root(&self, e: &ast::Expr) -> Option<(usize, bool)> {
        match e {
            ast::Expr::ParenExpr(p) => self.place_root(&p.expr()?),
            ast::Expr::PathExpr(p) => {
                let n = p.path()?.as_single_name_ref()?;
                Some((self.lookup(n.text().as_ref())?, false))
            }
            ast::Expr::FieldExpr(f) => self.place_root(&f.expr()?),
            ast::Expr::IndexExpr(i) => self.place_root(&i.base()?),
            ast::Expr::PrefixExpr(p) if p.op_kind() == Some(ast::UnaryOp::Deref) => {
                self.place_root(&p.expr()?).map(|(id, _)| (id, true))
            }
            _ => None,
        }
    }

    fn needs_mut(&mut self, e: &ast::Expr) {
        if let Some((id, through)) = self.place_root(e)
            && !through
        {
            self.vars[id].needs_mut = true;
        }
    }

    // ----------------------------------------------------------------------- functions

    fn function(&mut self, f: &ast::Fn) {
        let Some(body) = f.body() else { return };
        self.scopes = vec![HashMap::new()];
        if let Some(pl) = f.param_list() {
            for p in pl.params() {
                if let Some(pat) = p.pat() {
                    self.bind(&pat);
                }
            }
        }
        self.block(&body);
        self.exit();
    }

    fn block(&mut self, b: &ast::BlockExpr) {
        if b.async_token().is_some() {
            let saved = self.flow.clone();
            self.closures.push(self.vars.len());
            self.block_inner(b);
            self.closures.pop();
            self.flow = saved;
            return;
        }
        let label = b.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
        if label.is_some() {
            self.loops.push(LoopCx { label, is_loop: false, breaks: Flow::dead(), continues: Flow::dead() });
        }
        self.block_inner(b);
        if b.label().is_some() {
            let cx = self.loops.pop().unwrap();
            self.flow = Flow::join(std::mem::take(&mut self.flow), cx.breaks);
        }
    }

    fn block_inner(&mut self, b: &ast::BlockExpr) {
        let Some(list) = b.stmt_list() else { return };
        self.scopes.push(HashMap::new());
        for s in list.statements() {
            self.stmt(&s);
        }
        if let Some(t) = list.tail_expr() {
            self.expr(&t);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, s: &ast::Stmt) {
        // (A statement a false `cfg` leaves out.)
        if s.syntax().children().any(|c| c.kind() == K::ATTR) && crate::macros::under_false_cfg(s.syntax()) {
            return;
        }
        match s {
            ast::Stmt::LetStmt(l) => {
                if let Some(init) = l.initializer() {
                    self.expr(&init);
                }
                if let Some(els) = l.let_else().and_then(|e| e.block_expr()) {
                    let saved = self.flow.clone();
                    self.block(&els);
                    self.flow = saved;
                }
                let Some(pat) = l.pat() else { return };
                let ref_mut = self.bind(&pat);
                if ref_mut && let Some(init) = l.initializer() {
                    self.needs_mut(&init);
                }
                if let ast::Pat::IdentPat(ip) = &pat
                    && ip.pat().is_none()
                    && let Some(n) = ip.name()
                    && let Some(id) = self.lookup(n.text().as_ref())
                {
                    match l.initializer() {
                        Some(init) => {
                            let r = self.range(init.syntax());
                            self.write(id, r, None, true);
                        }
                        None => self.vars[id].deferred = true,
                    }
                }
            }
            ast::Stmt::ExprStmt(e) => {
                if let Some(x) = e.expr() {
                    self.expr(&x);
                }
            }
            ast::Stmt::Item(ast::Item::MacroCall(mc)) => self.macro_call(mc),
            ast::Stmt::Item(_) => {}
        }
    }

    // ---------------------------------------------------------------------- expressions

    /// The child expressions of `n` in order (through the nodes between, not into patterns,
    /// types or items).
    fn children(&mut self, n: &SyntaxNode) {
        for c in n.children() {
            if let Some(e) = ast::Expr::cast(c.clone()) {
                self.expr(&e);
            } else if !(ast::Pat::can_cast(c.kind()) || ast::Type::can_cast(c.kind()) || ast::Item::can_cast(c.kind()) || c.kind() == K::ATTR) {
                self.children(&c);
            }
        }
    }

    fn expr(&mut self, e: &ast::Expr) {
        match e {
            ast::Expr::PathExpr(p) => {
                if let Some(n) = p.path().and_then(|p| p.as_single_name_ref())
                    && let Some(id) = self.lookup(n.text().as_ref())
                {
                    self.read(id);
                }
            }
            ast::Expr::BinExpr(b) => self.bin(b),
            ast::Expr::RefExpr(r) => {
                if let Some(inner) = r.expr() {
                    self.expr(&inner);
                    if r.mut_token().is_some() {
                        self.needs_mut(&inner);
                        if let Some((id, _)) = self.place_root(&inner) {
                            self.vars[id].escaped = true;
                        }
                    }
                }
            }
            ast::Expr::MethodCallExpr(m) => {
                if let Some(recv) = m.receiver() {
                    self.expr(&recv);
                    let name = m.name_ref().map(|n| n.text().to_string()).unwrap_or_default();
                    if !SHARED_METHODS.contains(&name.as_str()) || self.l.own_mut_methods.contains(&name) {
                        self.needs_mut(&recv);
                    }
                }
                if let Some(args) = m.arg_list() {
                    self.children(args.syntax());
                }
            }
            ast::Expr::CallExpr(c) => {
                match c.expr() {
                    // (Calling a closure binding: an `FnMut` needs it `mut`.)
                    Some(ast::Expr::PathExpr(p))
                        if p.path().and_then(|p| p.as_single_name_ref()).is_some_and(|n| self.lookup(n.text().as_ref()).is_some()) =>
                    {
                        let n = p.path().and_then(|p| p.as_single_name_ref()).unwrap();
                        let id = self.lookup(n.text().as_ref()).unwrap();
                        self.read(id);
                        self.vars[id].needs_mut = true;
                    }
                    Some(callee) => self.expr(&callee),
                    None => {}
                }
                if let Some(args) = c.arg_list() {
                    self.children(args.syntax());
                }
                if self.base == 0 && self.l.diverges(e) {
                    self.flow = Flow::dead();
                }
            }
            ast::Expr::ClosureExpr(c) => {
                let saved = self.flow.clone();
                self.closures.push(self.vars.len());
                self.scopes.push(HashMap::new());
                if let Some(pl) = c.param_list() {
                    for p in pl.params() {
                        if let Some(pat) = p.pat() {
                            self.bind(&pat);
                        }
                    }
                }
                if let Some(body) = c.body() {
                    self.expr(&body);
                }
                self.scopes.pop();
                self.closures.pop();
                self.flow = saved;
            }
            ast::Expr::BlockExpr(b) => self.block(b),
            ast::Expr::IfExpr(i) => {
                self.scopes.push(HashMap::new());
                if let Some(c) = i.condition() {
                    self.expr(&c);
                }
                let after_cond = self.flow.clone();
                if let Some(t) = i.then_branch() {
                    self.block(&t);
                }
                self.scopes.pop();
                let then_flow = std::mem::replace(&mut self.flow, after_cond);
                match i.else_branch() {
                    Some(ast::ElseBranch::Block(b)) => self.block(&b),
                    Some(ast::ElseBranch::IfExpr(x)) => self.expr(&ast::Expr::IfExpr(x)),
                    None => {}
                }
                self.flow = Flow::join(then_flow, std::mem::take(&mut self.flow));
            }
            ast::Expr::LetExpr(l) => {
                if let Some(x) = l.expr() {
                    self.expr(&x);
                }
                if let Some(p) = l.pat()
                    && self.bind(&p)
                    && let Some(x) = l.expr()
                {
                    self.needs_mut(&x);
                }
            }
            ast::Expr::MatchExpr(m) => {
                let scrutinee = m.expr();
                if let Some(x) = &scrutinee {
                    self.expr(x);
                }
                let saved = self.flow.clone();
                let mut out = Flow::dead();
                for arm in m.match_arm_list().into_iter().flat_map(|l| l.arms()) {
                    self.flow = saved.clone();
                    self.scopes.push(HashMap::new());
                    if let Some(p) = arm.pat()
                        && self.bind(&p)
                        && let Some(x) = &scrutinee
                    {
                        self.needs_mut(x);
                    }
                    if let Some(g) = arm.guard().and_then(|g| g.condition()) {
                        self.expr(&g);
                    }
                    if let Some(b) = arm.expr() {
                        self.expr(&b);
                    }
                    self.scopes.pop();
                    out = Flow::join(out, std::mem::take(&mut self.flow));
                }
                self.flow = out;
            }
            ast::Expr::WhileExpr(w) => {
                let label = w.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
                self.run_loop(label, |b| {
                    if let Some(c) = w.condition() {
                        b.expr(&c);
                    }
                    Some(b.flow.clone())
                }, w.loop_body());
            }
            ast::Expr::LoopExpr(l) => {
                let label = l.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
                self.run_loop(label, |_| None, l.loop_body());
            }
            ast::Expr::ForExpr(f) => {
                if let Some(it) = f.iterable() {
                    self.expr(&it);
                }
                let label = f.label().and_then(|l| l.lifetime()).map(|l| l.text().to_string());
                let pat = f.pat();
                self.run_loop(label, |b| {
                    let exit = b.flow.clone();
                    if let Some(p) = &pat {
                        b.bind(p);
                    }
                    Some(exit)
                }, f.loop_body());
            }
            ast::Expr::BreakExpr(b) => {
                if let Some(x) = b.expr() {
                    self.expr(&x);
                }
                let label = b.lifetime().map(|l| l.text().to_string());
                let flow = std::mem::replace(&mut self.flow, Flow::dead());
                if let Some(cx) = self.target(&label) {
                    cx.breaks = Flow::join(std::mem::take(&mut cx.breaks), flow);
                }
            }
            ast::Expr::ContinueExpr(c) => {
                let label = c.lifetime().map(|l| l.text().to_string());
                let flow = std::mem::replace(&mut self.flow, Flow::dead());
                if let Some(cx) = self.target(&label) {
                    cx.continues = Flow::join(std::mem::take(&mut cx.continues), flow);
                }
            }
            ast::Expr::ReturnExpr(r) => {
                if let Some(x) = r.expr() {
                    self.expr(&x);
                }
                self.exit();
                self.flow = Flow::dead();
            }
            ast::Expr::TryExpr(t) => {
                if let Some(x) = t.expr() {
                    self.expr(&x);
                }
                self.exit();
            }
            ast::Expr::RecordExpr(r) => {
                if let Some(list) = r.record_expr_field_list() {
                    for f in list.fields() {
                        match f.expr() {
                            Some(x) => self.expr(&x),
                            None => {
                                if let Some(n) = f.name_ref()
                                    && let Some(id) = self.lookup(n.text().as_ref())
                                {
                                    self.read(id);
                                }
                            }
                        }
                    }
                    if let Some(s) = list.spread() {
                        self.expr(&s);
                    }
                }
            }
            ast::Expr::MacroExpr(m) => {
                if let Some(mc) = m.macro_call() {
                    self.macro_call(&mc);
                }
            }
            _ => self.children(e.syntax()),
        }
    }

    fn bin(&mut self, b: &ast::BinExpr) {
        let (Some(lhs), Some(rhs)) = (b.lhs(), b.rhs()) else {
            self.children(b.syntax());
            return;
        };
        match b.op_kind() {
            Some(ast::BinaryOp::Assignment { op }) => {
                self.expr(&rhs);
                let span = self.range(b.syntax());
                let lit = matches!(rhs, ast::Expr::Literal(_));
                self.assign(&lhs, span, op.is_some(), lit);
            }
            Some(ast::BinaryOp::LogicOp(_)) => {
                self.expr(&lhs);
                let saved = self.flow.clone();
                self.expr(&rhs);
                self.flow = Flow::join(std::mem::take(&mut self.flow), saved);
            }
            _ => {
                self.expr(&lhs);
                self.expr(&rhs);
            }
        }
    }

    /// An assignment to `lhs` (`compound`: `+=` and the like, which read it first).
    fn assign(&mut self, lhs: &ast::Expr, span: Range, compound: bool, lit: bool) {
        match lhs {
            ast::Expr::ParenExpr(p) => {
                if let Some(x) = p.expr() {
                    self.assign(&x, span, compound, lit);
                }
            }
            ast::Expr::PathExpr(p) if p.path().and_then(|p| p.as_single_name_ref()).is_some_and(|n| self.lookup(n.text().as_ref()).is_some()) => {
                let n = p.path().and_then(|p| p.as_single_name_ref()).unwrap();
                let id = self.lookup(n.text().as_ref()).unwrap();
                if compound {
                    self.read_value(id);
                }
                self.vars[id].needs_mut = true;
                let lhs = self.range(p.syntax());
                self.write(id, span, Some((lhs, lit)), false);
            }
            ast::Expr::TupleExpr(t) => {
                for f in t.fields() {
                    self.assign(&f, span, compound, lit);
                }
            }
            ast::Expr::UnderscoreExpr(_) => {}
            _ => {
                // A field, an element, or through a reference: the binding is read.
                self.expr(lhs);
                self.needs_mut(lhs);
            }
        }
    }

    /// Where a `break` or `continue` goes: the loop or block it names, or the innermost loop.
    fn target(&mut self, label: &Option<String>) -> Option<&mut LoopCx> {
        match label {
            Some(l) => self.loops.iter_mut().rev().find(|c| c.label.as_ref() == Some(l)),
            None => self.loops.iter_mut().rev().find(|c| c.is_loop),
        }
    }

    /// A loop: `head` runs at the top of each iteration (the condition, or the pattern's
    /// binding) and gives the flow that leaves the loop there; the body runs twice, so values
    /// from one iteration reach the next.
    fn run_loop(&mut self, label: Option<String>, mut head: impl FnMut(&mut Self) -> Option<Flow>, body: Option<ast::BlockExpr>) {
        let entry = self.flow.clone();
        self.loops.push(LoopCx { label, is_loop: true, breaks: Flow::dead(), continues: Flow::dead() });
        let mut back = Flow::dead();
        let mut exits = Flow::dead();
        for _ in 0..2 {
            self.flow = Flow::join(entry.clone(), back.clone());
            self.scopes.push(HashMap::new());
            if let Some(x) = head(self) {
                exits = Flow::join(exits, x);
            }
            if let Some(b) = &body {
                self.block(b);
            }
            self.scopes.pop();
            let cx = self.loops.last_mut().unwrap();
            let cont = std::mem::replace(&mut cx.continues, Flow::dead());
            back = Flow::join(std::mem::take(&mut self.flow), cont);
        }
        let cx = self.loops.pop().unwrap();
        self.flow = Flow::join(exits, cx.breaks);
    }

    // --------------------------------------------------------------------------- macros

    fn macro_call(&mut self, mc: &ast::MacroCall) {
        let name = crate::macros::macro_name(mc).unwrap_or_default();
        if !format_macro(&name) {
            self.opaque(mc);
            return;
        }
        let src = self.l.src;
        let args = match crate::macros::split_repeat(src, self.base, mc).filter(|_| name == "vec") {
            Some((a, b)) => vec![a, b],
            None => crate::macros::split_args(src, self.base, mc),
        };
        let args: Vec<crate::macros::MacroArg> = if name == "matches" { args.into_iter().take(1).collect() } else { args };
        let named: HashSet<String> = args.iter().filter_map(|a| a.name.clone()).collect();
        for (i, a) in args.iter().enumerate() {
            let (expr, errors) = crate::macros::parse_arg(a);
            match expr.filter(|_| errors.is_empty()) {
                Some(x) => {
                    let saved = std::mem::replace(&mut self.base, a.start);
                    self.expr(&x);
                    if i == 0 && matches!(name.as_str(), "write" | "writeln") {
                        self.needs_mut(&x);
                    }
                    self.base = saved;
                }
                None => {
                    // (Not an expression this pass reads: its names may all be used.)
                    for t in mc.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
                        let r = trange(&t);
                        if t.kind() == K::IDENT && r.0 + self.base >= a.start && r.1 + self.base <= a.end() {
                            self.opaque_name(t.text());
                        }
                    }
                }
            }
            if let Some(s) = crate::macros::format_string(a)
                && let Ok(pieces) = crate::fmt::parse(&s)
            {
                for p in pieces {
                    let crate::fmt::Piece::Arg { arg, spec } = p else { continue };
                    for r in [Some(arg), spec.width_arg, spec.precision_arg].into_iter().flatten() {
                        if let crate::fmt::ArgRef::Name(n) = r
                            && !named.contains(&n)
                            && let Some(id) = self.lookup(&n)
                        {
                            self.read(id);
                        }
                    }
                }
            }
        }
        if diverging_macro(&name) {
            self.flow = Flow::dead();
        }
    }

    /// A macro this pass can't read: every binding it names is used, and may be changed.
    fn opaque(&mut self, mc: &ast::MacroCall) {
        let names: Vec<String> = mc
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == K::IDENT)
            .map(|t| t.text().to_string())
            .collect();
        for n in names {
            self.opaque_name(&n);
        }
    }

    fn opaque_name(&mut self, n: &str) {
        if let Some(id) = self.lookup(n) {
            self.read(id);
            self.vars[id].escaped = true;
            self.vars[id].needs_mut = true;
        }
    }
}

impl Linter<'_> {
    /// `unused_variables`, `unused_mut` and `unused_assignments`, function by function.
    fn bodies(&mut self) {
        let fns: Vec<ast::Fn> = self
            .file
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|f| f.body().is_some() && !self.skipped(f.syntax()))
            .collect();
        let mut found: Vec<(&'static str, Diagnostic)> = Vec::new();
        for f in fns {
            let mut b = Body {
                l: self,
                vars: Vec::new(),
                by_decl: HashMap::new(),
                defs: Vec::new(),
                def_at: HashMap::new(),
                scopes: Vec::new(),
                flow: Flow::default(),
                loops: Vec::new(),
                closures: Vec::new(),
                base: 0,
            };
            b.function(&f);
            let (vars, defs) = (b.vars, b.defs);
            for v in &vars {
                if !v.report {
                    continue;
                }
                let needs_mut = v.needs_mut || self.cx.mut_roots.contains(&v.decl.0);
                if let Some(m) = v.mut_kw
                    && !needs_mut
                    && !v.deferred
                {
                    let mut d = self.diag(m.0, v.decl.1, "variable does not need to be mutable");
                    d.spans.push(self.span(m.0, m.1, "help: remove this `mut`"));
                    found.push(("unused_mut", d));
                }
                if !v.used {
                    let n = &v.name;
                    if v.assigned {
                        let mut d = self.diag(v.span.0, v.span.1, format!("variable `{n}` is assigned to, but never used"));
                        d.notes.push(format!("consider using `_{n}` instead"));
                        found.push(("unused_variables", d));
                    } else {
                        let mut d = self.diag(v.span.0, v.span.1, format!("unused variable: `{n}`"));
                        d.label = Some(if v.shorthand {
                            format!("help: try ignoring the field: `{n}: _`")
                        } else {
                            format!("help: if this is intentional, prefix it with an underscore: `_{n}`")
                        });
                        found.push(("unused_variables", d));
                    }
                }
            }
            for def in &defs {
                let v = &vars[def.var];
                if !v.report || v.escaped || def.read || (def.init && !v.used) {
                    continue;
                }
                if def.killers.is_empty() && !def.exits {
                    continue;
                }
                // (rustc doesn't lint a value a macro builds, `vec![..]`.)
                if def.init && is_macro_call(&self.src[def.span.0..def.span.1]) {
                    continue;
                }
                // An assignment of a type with drop glue is marked at its left side.
                let ty = self.ty(v.decl).cloned();
                let at = |span: Range, lhs: Option<(Range, bool)>| match (lhs, &ty) {
                    (Some((l, lit)), Some(t)) => match self.needs_drop(t, 0) {
                        Some(true) => l,
                        None if *t == Ty::Str && !lit => l,
                        _ => span,
                    },
                    _ => span,
                };
                let n = &v.name;
                let (s, e) = at(def.span, def.lhs);
                let mut d = self.diag(s, e, format!("value assigned to `{n}` is never read"));
                if let ([(k, kl)], false) = (def.killers.as_slice(), def.exits) {
                    let (ks, ke) = at(*k, *kl);
                    d.label = Some("this value is reassigned later and never used".into());
                    d.spans.push(self.span(ks, ke, format!("`{n}` is overwritten here before the previous value is read")));
                } else {
                    d.helps.push("maybe it is overwritten before being read?".into());
                }
                found.push(("unused_assignments", d));
            }
        }
        for (lint, d) in found {
            self.emit(lint, d);
        }
    }

    /// Whether dropping a value of type `t` runs code (rustc's drop glue); `None` when the
    /// type doesn't say (a `String` and a `&str` are both `Str` to the checker).
    fn needs_drop(&self, t: &Ty, depth: usize) -> Option<bool> {
        if depth > 4 {
            return None;
        }
        Some(match t {
            Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Unit | Ty::Never => false,
            Ty::Vec(_) | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..) | Ty::Dyn(_, _) | Ty::RefCell(_) => true,
            // (Inside another type, more likely a `String` than a `&str`.)
            Ty::Str if depth > 0 => true,
            Ty::Option(x) => self.needs_drop(x, depth + 1)?,
            Ty::Result(a, b) => self.needs_drop(a, depth + 1)? || self.needs_drop(b, depth + 1)?,
            Ty::Tuple(es) => {
                let mut any = false;
                for e in es {
                    any |= self.needs_drop(e, depth + 1)?;
                }
                any
            }
            Ty::Adt(n, _) => {
                let d = self.cx.types.adts.defs.get(n)?;
                if d.implements("Drop") {
                    return Some(true);
                }
                let mut any = false;
                for v in &d.variants {
                    for f in &v.fields {
                        any |= self.needs_drop(&f.ty, depth + 1)?;
                    }
                }
                any
            }
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------- dead code

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum ItemKind {
    Fn,
    Method,
    AssocFn,
    AssocConst,
    Struct,
    Enum,
    Const,
    Static,
    TypeAlias,
    Trait,
}

struct DeadItem {
    kind: ItemKind,
    name: String,
    name_range: Range,
    node: SyntaxNode,
    /// The inherent `impl` an associated item is in.
    impl_block: Option<ast::Impl>,
    root: bool,
}

/// Attributes that don't make an item used by something rustc can't see.
const PLAIN_ATTRS: &[&str] = &[
    "doc", "allow", "warn", "deny", "forbid", "expect", "cfg", "derive", "inline", "must_use", "deprecated",
    "rustfmt", "cold", "track_caller", "non_exhaustive", "repr", "default",
];

/// Derives that don't read a struct's fields as far as rustc's dead-code analysis goes (`Debug`
/// and `Clone` are ignored on purpose, `Copy` and `Default` don't read them).
const TRIVIAL_DERIVES: &[&str] = &["Debug", "Clone", "Copy", "Default"];

fn derives(attrs: impl Iterator<Item = ast::Attr>) -> Vec<String> {
    let mut out = Vec::new();
    for a in attrs {
        if a.path().is_some_and(|p| p.syntax().text() == "derive")
            && let Some(tt) = attr_tt(&a)
        {
            for t in tt.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
                if t.kind() == K::IDENT {
                    out.push(t.text().to_string());
                }
            }
        }
    }
    out
}

/// Whether the item can be named from outside the crate: `pub`, in modules that are too.
fn exported(n: &SyntaxNode) -> bool {
    let vis = |n: &SyntaxNode| {
        n.children()
            .find_map(ast::Visibility::cast)
            .is_some_and(|v| v.syntax().text() == "pub")
    };
    if !vis(n) {
        return false;
    }
    for a in n.ancestors().skip(1) {
        if ast::Module::can_cast(a.kind()) && !vis(&a) {
            return false;
        }
        if ast::Fn::can_cast(a.kind()) || ast::Impl::can_cast(a.kind()) {
            return false;
        }
    }
    true
}

/// The type an `impl` is for, by its last path segment's name.
fn impl_self_name(i: &ast::Impl) -> Option<String> {
    let ast::Type::PathType(p) = i.self_ty()? else { return None };
    Some(p.path()?.segment()?.name_ref()?.text().to_string())
}

/// What the names in live code say: the names used, the structs and variants constructed, and
/// the fields read.
#[derive(Default)]
struct Uses {
    mentioned: HashSet<String>,
    constructed: HashSet<String>,
    /// Fields read on a value of unknown type (any struct's field of that name).
    read: HashSet<String>,
    /// Fields read on a known struct: (struct, field).
    read_of: HashSet<(String, String)>,
    /// The file's structs (a type the checker names otherwise reads by field name only).
    structs: HashSet<String>,
    /// The fields each of them declares (a field it lacks was read through `Deref`).
    fields_of: HashMap<String, HashSet<String>>,
    /// Those with generic parameters (the checker may name their types otherwise): their fields
    /// are read by name.
    generic: HashSet<String>,
    /// Types named by a path (`B::LO`, `size_of::<M>()`): rustc doesn't call them never
    /// constructed, nor check their fields.
    as_type: HashSet<String>,
}

impl Uses {
    /// Every identifier counts as everything (a macro's tokens, another file's text).
    fn all(&mut self, name: &str) {
        self.mentioned.insert(name.to_string());
        self.constructed.insert(name.to_string());
        self.read.insert(name.to_string());
    }

    fn read_field(&mut self, owner: Option<String>, field: String) {
        match owner.filter(|o| self.structs.contains(o) && !self.generic.contains(o) && self.fields_of.get(o).is_some_and(|fs| fs.contains(&field))) {
            Some(o) => {
                self.read_of.insert((o, field));
            }
            None => {
                self.read.insert(field);
            }
        }
    }

    fn field_read(&self, owner: &str, field: &str) -> bool {
        self.read.contains(field) || self.read_of.contains(&(owner.to_string(), field.to_string()))
    }
}

impl Linter<'_> {
    /// The uses in a macro call's tokens: every name may be used and constructed, a `.name` may
    /// read a field, and a format string's `{name}` uses `name`.
    fn token_tree_uses(&self, tt: &ast::TokenTree, uses: &mut Uses) {
        let toks: Vec<SyntaxToken> = tt
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        for (i, t) in toks.iter().enumerate() {
            match t.kind() {
                K::IDENT => {
                    let name = t.text().to_string();
                    if i > 0 && toks[i - 1].kind() == T![.] {
                        uses.read.insert(name.clone());
                    }
                    uses.mentioned.insert(name.clone());
                    uses.constructed.insert(name);
                }
                K::INT_NUMBER if i > 0 && toks[i - 1].kind() == T![.] => {
                    uses.read.insert(t.text().to_string());
                }
                K::STRING => {
                    let Some(s) = <ast::String as ra_ap_syntax::AstToken>::cast(t.clone()).and_then(|s| crate::lineindex::string_value(&s)) else { continue };
                    let Ok(pieces) = crate::fmt::parse(&s) else { continue };
                    for p in pieces {
                        let crate::fmt::Piece::Arg { arg, spec } = p else { continue };
                        for r in [Some(arg), spec.width_arg, spec.precision_arg].into_iter().flatten() {
                            if let crate::fmt::ArgRef::Name(n) = r {
                                uses.mentioned.insert(n.clone());
                                uses.constructed.insert(n);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

impl Linter<'_> {
    fn dead_items(&self) -> Vec<DeadItem> {
        let mut items = Vec::new();
        let main_file = self.file.syntax().children().filter_map(ast::Fn::cast).any(|f| f.name().is_some_and(|n| n.text() == "main"));
        for n in self.file.syntax().descendants() {
            if !ast::Item::can_cast(n.kind()) && !ast::Variant::can_cast(n.kind()) {
                continue;
            }
            if self.skipped(&n) {
                continue;
            }
            let assoc_parent = n.parent().filter(|p| p.kind() == K::ASSOC_ITEM_LIST).and_then(|p| p.parent());
            let in_impl = assoc_parent.clone().and_then(ast::Impl::cast);
            let in_trait = assoc_parent.as_ref().is_some_and(|p| ast::Trait::can_cast(p.kind()));
            let in_extern = n.parent().is_some_and(|p| p.kind() == K::EXTERN_ITEM_LIST);
            let trait_impl = in_impl.as_ref().is_some_and(|i| i.trait_().is_some());
            if in_trait || trait_impl || in_extern {
                continue;
            }
            let (kind, name) = if let Some(f) = ast::Fn::cast(n.clone()) {
                let kind = match (&in_impl, f.param_list().and_then(|p| p.self_param()).is_some()) {
                    (Some(_), true) => ItemKind::Method,
                    (Some(_), false) => ItemKind::AssocFn,
                    (None, _) => ItemKind::Fn,
                };
                (kind, f.name())
            } else if let Some(s) = ast::Struct::cast(n.clone()) {
                (ItemKind::Struct, s.name())
            } else if let Some(e) = ast::Enum::cast(n.clone()) {
                (ItemKind::Enum, e.name())
            } else if let Some(c) = ast::Const::cast(n.clone()) {
                (if in_impl.is_some() { ItemKind::AssocConst } else { ItemKind::Const }, c.name())
            } else if let Some(s) = ast::Static::cast(n.clone()) {
                (ItemKind::Static, s.name())
            } else if let Some(t) = ast::TypeAlias::cast(n.clone()) {
                if in_impl.is_some() {
                    continue;
                }
                (ItemKind::TypeAlias, t.name())
            } else if let Some(t) = ast::Trait::cast(n.clone()) {
                (ItemKind::Trait, t.name())
            } else {
                continue;
            };
            let Some(name) = name else { continue };
            let text = name.text().to_string();
            let attrs: Vec<ast::Attr> = n.children().filter_map(ast::Attr::cast).collect();
            let special = attrs.iter().any(|a| {
                let p = a.path().map(|p| p.syntax().text().to_string()).unwrap_or_default();
                !PLAIN_ATTRS.contains(&p.as_str())
            });
            let at_root = n.parent().is_some_and(|p| p.kind() == K::SOURCE_FILE);
            let root = special
                || (kind == ItemKind::Fn && text == "main" && at_root)
                || exported(&n)
                // (A file without `main` is a module of the script's: what it exports is its
                // interface. Inherent methods too: `pub fn` in a `pub` type's impl.)
                || (!main_file && in_impl.as_ref().is_some_and(|_| n.children().any(|c| c.kind() == K::VISIBILITY)))
                || text.starts_with('_')
                || self.level(usize::from(name.syntax().text_range().start()), "dead_code").is_none();
            items.push(DeadItem {
                kind,
                name: text,
                name_range: range(name.syntax()),
                node: n.clone(),
                impl_block: in_impl.filter(|_| matches!(kind, ItemKind::Method | ItemKind::AssocFn | ItemKind::AssocConst)),
                root,
            });
        }
        items
    }

    /// Records the uses in `n`, skipping the tracked items inside it (`stop`).
    fn scan_uses(&self, n: &SyntaxNode, stop: &HashSet<Range>, uses: &mut Uses) {
        self.scan_uses_at(n, 0, stop, uses);
    }

    /// `scan_uses` of nodes parsed from the text at `base` (a macro argument's).
    fn scan_uses_at(&self, n: &SyntaxNode, base: usize, stop: &HashSet<Range>, uses: &mut Uses) {
        let self_ty = |at: &SyntaxNode| -> Option<String> {
            at.ancestors().find_map(ast::Impl::cast).and_then(|i| impl_self_name(&i))
        };
        let mut it = n.preorder_with_tokens();
        while let Some(ev) = it.next() {
            let ra_ap_syntax::WalkEvent::Enter(el) = ev else { continue };
            match el {
                ra_ap_syntax::NodeOrToken::Node(c) => {
                    if &c != n && base == 0 && (stop.contains(&range(&c)) || (ast::Item::can_cast(c.kind()) && self.skipped(&c))) {
                        it.skip_subtree();
                        continue;
                    }
                    if let Some(tt) = ast::TokenTree::cast(c.clone()) {
                        self.macro_uses(&tt, base, stop, uses);
                        it.skip_subtree();
                        continue;
                    }
                    let last = |p: Option<ast::Path>| -> Option<String> {
                        let seg = p?.segment()?;
                        if seg.self_type_token().is_some() {
                            return self_ty(&c);
                        }
                        Some(seg.name_ref()?.text().to_string())
                    };
                    // The types a path goes through (`B` of `B::LO`, `L` of `<L as T>::CAP`).
                    let qualifiers = |p: &ast::Path, uses: &mut Uses| {
                        let Some(q) = p.qualifier() else { return };
                        for t in q.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
                            match t.kind() {
                                K::IDENT => {
                                    uses.as_type.insert(t.text().to_string());
                                }
                                K::SELF_TYPE_KW => {
                                    if let Some(s) = self_ty(&c) {
                                        uses.as_type.insert(s);
                                    }
                                }
                                _ => {}
                            }
                        }
                    };
                    if let Some(r) = ast::RecordExpr::cast(c.clone()) {
                        if let Some(name) = last(r.path()) {
                            uses.constructed.insert(name);
                        }
                    } else if let Some(p) = ast::PathExpr::cast(c.clone()) {
                        if let Some(name) = last(p.path()) {
                            uses.constructed.insert(name);
                        }
                        if let Some(path) = p.path() {
                            qualifiers(&path, uses);
                        }
                        // (A type in its generic arguments too: `size_of::<Mixed>()`.)
                        for t in p.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
                            if t.kind() == K::IDENT {
                                uses.as_type.insert(t.text().to_string());
                            }
                        }
                    } else if let Some(p) = ast::PathPat::cast(c.clone()) {
                        if let Some(path) = p.path() {
                            qualifiers(&path, uses);
                        }
                    } else if let Some(f) = ast::FieldExpr::cast(c.clone()) {
                        if let Some(n) = f.name_ref() {
                            let owner = f.expr().and_then(|x| match self.ty({ let (s, e) = range(x.syntax()); (s + base, e + base) }) {
                                Some(Ty::Adt(a, _)) => Some(a.rsplit("__").next().unwrap_or(a).to_string()),
                                _ => None,
                            });
                            uses.read_field(owner, n.text().to_string());
                        }
                    } else if let Some(f) = ast::RecordPatField::cast(c.clone()) {
                        if matches!(f.pat(), Some(ast::Pat::WildcardPat(_))) {
                            continue;
                        }
                        let name = f.name_ref().map(|n| n.text().to_string()).or_else(|| match f.pat() {
                            Some(ast::Pat::IdentPat(ip)) => ip.name().map(|n| n.text().to_string()),
                            _ => None,
                        });
                        let owner = f.syntax().ancestors().find_map(ast::RecordPat::cast).and_then(|r| last(r.path()));
                        if let Some(name) = name {
                            uses.read_field(owner, name);
                        }
                    } else if let Some(t) = ast::TupleStructPat::cast(c.clone()) {
                        if let Some(path) = t.path() {
                            qualifiers(&path, uses);
                        }
                        // Each field the pattern binds or tests is read (`_` reads nothing).
                        let owner = last(t.path());
                        let fields: Vec<ast::Pat> = t.fields().collect();
                        let rest = fields.iter().position(|f| matches!(f, ast::Pat::RestPat(_)));
                        for (k, f) in fields.iter().enumerate() {
                            if rest.is_some_and(|r| k > r) {
                                // (Counted from the end: any of them.)
                                for j in k..k + 16 {
                                    uses.read_field(owner.clone(), j.to_string());
                                }
                            } else if !matches!(f, ast::Pat::WildcardPat(_) | ast::Pat::RestPat(_)) {
                                uses.read_field(owner.clone(), k.to_string());
                            }
                        }
                    } else if let Some(r) = ast::RecordPat::cast(c.clone())
                        && let Some(path) = r.path()
                    {
                        qualifiers(&path, uses);
                    }
                }
                // (A definition's own name isn't a use of it; a binding pattern's may name a constant.)
                ra_ap_syntax::NodeOrToken::Token(t) if t.kind() == K::IDENT
                    && t.parent().is_some_and(|p| p.kind() == K::NAME && p.parent().is_none_or(|pp| pp.kind() != K::IDENT_PAT)) => {}
                ra_ap_syntax::NodeOrToken::Token(t) => match t.kind() {
                    K::IDENT => {
                        uses.mentioned.insert(t.text().to_string());
                    }
                    K::SELF_TYPE_KW => {
                        if let Some(s) = t.parent().and_then(|p| self_ty(&p)) {
                            uses.mentioned.insert(s);
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    /// `dead_code`: functions, types, constants and associated items nothing live uses, structs
    /// and variants never constructed, fields never read. Live code is what `main` (or, in a
    /// module file, what the file exports) reaches, by name.
    fn dead_code(&mut self) {
        if crate::adt::test_build() {
            return;
        }
        let items = self.dead_items();
        if items.is_empty() {
            return;
        }
        let stop: HashSet<Range> = items.iter().filter(|i| i.kind != ItemKind::Trait).map(|i| range(&i.node)).collect();
        let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, it) in items.iter().enumerate() {
            by_name.entry(&it.name).or_default().push(i);
        }
        let mut uses = Uses::default();
        uses.structs = items.iter().filter(|i| i.kind == ItemKind::Struct).map(|i| i.name.clone()).collect();
        for v in self.file.syntax().descendants().filter_map(ast::Variant::cast) {
            if let Some(n) = v.name() {
                uses.structs.insert(n.text().to_string());
            }
        }
        for n in self.file.syntax().descendants() {
            let (name, list) = if let Some(s) = ast::Struct::cast(n.clone()) {
                (s.name(), s.field_list())
            } else if let Some(v) = ast::Variant::cast(n.clone()) {
                (v.name(), v.field_list())
            } else {
                continue;
            };
            let Some(name) = name else { continue };
            let generic_owner = n.ancestors().find_map(|a| ast::Struct::cast(a.clone()).map(|s| s.generic_param_list().is_some()).or_else(|| ast::Enum::cast(a).map(|e| e.generic_param_list().is_some())));
            if generic_owner == Some(true) {
                uses.generic.insert(name.text().to_string());
            }
            let fields: HashSet<String> = match list {
                Some(ast::FieldList::RecordFieldList(l)) => l.fields().filter_map(|f| f.name()).map(|n| n.text().to_string()).collect(),
                Some(ast::FieldList::TupleFieldList(l)) => (0..l.fields().count()).map(|i| i.to_string()).collect(),
                None => HashSet::new(),
            };
            uses.fields_of.entry(name.text().to_string()).or_default().extend(fields);
        }
        // (A `use` naming an enum's variants uses the enum.)
        for u in self.file.syntax().descendants().filter_map(ast::Use::cast) {
            if self.skipped(u.syntax()) {
                continue;
            }
            for t in u.syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
                if t.kind() == K::IDENT {
                    uses.mentioned.insert(t.text().to_string());
                }
            }
        }
        // What other files of the script may use, and code that isn't an item of its own:
        // trait impls, traits, macros.
        for t in self.module_texts() {
            for tok in ast::SourceFile::parse(&t, crate::macros::EDITION).tree().syntax().descendants_with_tokens().filter_map(|e| e.into_token()) {
                if tok.kind() == K::IDENT {
                    uses.all(tok.text());
                }
            }
        }
        for n in self.file.syntax().descendants() {
            let scan = match ast::Impl::cast(n.clone()) {
                Some(i) => i.trait_().is_some(),
                None => ast::Trait::can_cast(n.kind()) || ast::MacroRules::can_cast(n.kind()) || (n.kind() == K::MACRO_CALL && n.parent().is_some_and(|p| ast::Item::can_cast(p.kind()) || p.kind() == K::SOURCE_FILE || p.kind() == K::ITEM_LIST)),
            };
            if scan && !self.skipped(&n) {
                self.scan_uses(&n, &stop, &mut uses);
            }
        }
        // Struct items that construct themselves (derived `Default`, `Deserialize`, ..), and
        // whose fields derived impls read.
        let mut reads_all: HashSet<String> = HashSet::new();
        for it in &items {
            if !matches!(it.kind, ItemKind::Struct | ItemKind::Enum) {
                continue;
            }
            let ds = derives(it.node.children().filter_map(ast::Attr::cast));
            if ds.iter().any(|d| !TRIVIAL_DERIVES.contains(&d.as_str())) {
                reads_all.insert(it.name.clone());
                uses.constructed.insert(it.name.clone());
                if let Some(e) = ast::Enum::cast(it.node.clone()) {
                    for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
                        if let Some(n) = v.name() {
                            uses.constructed.insert(n.text().to_string());
                        }
                    }
                }
            }
            if ds.iter().any(|d| d == "Default") {
                uses.constructed.insert(it.name.clone());
                if let Some(e) = ast::Enum::cast(it.node.clone()) {
                    for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
                        if v.syntax().children().filter_map(ast::Attr::cast).any(|a| a.syntax().text().to_string().contains("default"))
                            && let Some(n) = v.name()
                        {
                            uses.constructed.insert(n.text().to_string());
                        }
                    }
                }
            }
        }
        // Other names for the file's types: `type A = B<..>;`, `use m::B as A;`.
        let mut aliases: Vec<(String, String)> = Vec::new();
        for n in self.file.syntax().descendants() {
            if let Some(t) = ast::TypeAlias::cast(n.clone()) {
                if let (Some(a), Some(ast::Type::PathType(p))) = (t.name(), t.ty())
                    && let Some(b) = p.path().and_then(|p| p.segment()).and_then(|s| s.name_ref())
                {
                    aliases.push((a.text().to_string(), b.text().to_string()));
                }
            } else if let Some(u) = ast::UseTree::cast(n)
                && let Some(r) = u.rename().and_then(|r| r.name())
                && let Some(b) = u.path().and_then(|p| p.segment()).and_then(|s| s.name_ref())
            {
                aliases.push((r.text().to_string(), b.text().to_string()));
            }
        }
        // The fixpoint: live items' code makes more items live.
        let mut live = vec![false; items.len()];
        let mut work: Vec<usize> = (0..items.len()).filter(|&i| items[i].root).collect();
        for &i in &work {
            live[i] = true;
        }
        let mut seen_mentions: HashSet<String> = HashSet::new();
        let mut seen_constructed: HashSet<String> = HashSet::new();
        loop {
            while let Some(i) = work.pop() {
                self.scan_uses(&items[i].node, &stop, &mut uses);
            }
            // (A use of an alias is a use of what it names.)
            for (alias, target) in &aliases {
                if uses.mentioned.contains(alias) {
                    uses.mentioned.insert(target.clone());
                }
                if uses.constructed.contains(alias) {
                    uses.constructed.insert(target.clone());
                }
                if uses.as_type.contains(alias) {
                    uses.as_type.insert(target.clone());
                }
                let fields: Vec<String> = uses.read_of.iter().filter(|(o, _)| o == alias).map(|(_, f)| f.clone()).collect();
                for f in fields {
                    uses.read_of.insert((target.clone(), f));
                }
            }
            let named: Vec<String> = uses.mentioned.iter().chain(&uses.constructed).chain(&uses.as_type).cloned().collect();
            for name in &named {
                let new_m = uses.mentioned.contains(name) && seen_mentions.insert(name.clone());
                let new_c = (uses.constructed.contains(name) || uses.as_type.contains(name)) && seen_constructed.insert(name.clone());
                if !new_m && !new_c {
                    continue;
                }
                for &i in by_name.get(name.as_str()).into_iter().flatten() {
                    let reached = if items[i].kind == ItemKind::Struct { uses.constructed.contains(name) || uses.as_type.contains(name) } else { uses.mentioned.contains(name) };
                    if reached && !live[i] {
                        live[i] = true;
                        work.push(i);
                    }
                }
            }
            if work.is_empty() {
                break;
            }
        }
        let mut found: Vec<Diagnostic> = Vec::new();
        // Associated items, grouped by their `impl`.
        let mut groups: BTreeMap<Range, (ast::Impl, Vec<usize>)> = BTreeMap::new();
        for (i, it) in items.iter().enumerate() {
            if live[i] {
                continue;
            }
            if let Some(imp) = &it.impl_block {
                groups.entry(range(imp.syntax())).or_insert_with(|| (imp.clone(), Vec::new())).1.push(i);
                continue;
            }
            let (s, e) = it.name_range;
            let n = &it.name;
            let message = match it.kind {
                ItemKind::Fn => format!("function `{n}` is never used"),
                ItemKind::Struct => format!("struct `{n}` is never constructed"),
                ItemKind::Enum => format!("enum `{n}` is never used"),
                ItemKind::Const => format!("constant `{n}` is never used"),
                ItemKind::Static => format!("static `{n}` is never used"),
                ItemKind::TypeAlias => format!("type alias `{n}` is never used"),
                ItemKind::Trait => format!("trait `{n}` is never used"),
                _ => continue,
            };
            found.push(self.diag(s, e, message));
        }
        for (_, (imp, members)) in groups {
            let kinds: HashSet<ItemKind> = members.iter().map(|&i| items[i].kind).collect();
            let many = members.len() > 1;
            let noun = match (kinds.len(), kinds.iter().next()) {
                (1, Some(ItemKind::Method)) => if many { "methods" } else { "method" },
                (1, Some(ItemKind::AssocFn)) => if many { "associated functions" } else { "associated function" },
                (1, Some(ItemKind::AssocConst)) => if many { "associated constants" } else { "associated constant" },
                _ => if many { "associated items" } else { "associated item" },
            };
            let names: Vec<String> = members.iter().map(|&i| items[i].name.clone()).collect();
            let spans: Vec<Range> = members.iter().map(|&i| items[i].name_range).collect();
            let verb = if many { "are" } else { "is" };
            let mut d = self.diag_spans(&spans, format!("{noun} {} {verb} never used", list(&names)));
            let start = range(imp.syntax()).0;
            let end = imp.self_ty().map(|t| range(t.syntax()).1).unwrap_or(start + 4);
            d.spans.push(self.span(start, end, format!("{noun} in this implementation")));
            found.push(d);
        }
        // Variants of live enums never constructed; fields of constructed structs and variants
        // never read.
        for (i, it) in items.iter().enumerate() {
            if !live[i] {
                continue;
            }
            let (owner_s, owner_e) = it.name_range;
            let ignored = {
                let mut v: Vec<String> = derives(it.node.children().filter_map(ast::Attr::cast))
                    .into_iter()
                    .filter(|d| d == "Debug" || d == "Clone")
                    .collect();
                v.sort();
                v.dedup();
                v
            };
            if let Some(e) = ast::Enum::cast(it.node.clone()) {
                let variants: Vec<ast::Variant> = e
                    .variant_list()
                    .into_iter()
                    .flat_map(|l| l.variants())
                    .filter(|v| !self.skipped(v.syntax()))
                    .collect();
                let dead: Vec<(String, Range)> = variants
                    .iter()
                    .filter_map(|v| v.name())
                    .map(|n| (n.text().to_string(), range(n.syntax())))
                    .filter(|(n, _)| !uses.constructed.contains(n) && !n.starts_with('_'))
                    .collect();
                if !dead.is_empty() {
                    let many = dead.len() > 1;
                    let names: Vec<String> = dead.iter().map(|(n, _)| n.clone()).collect();
                    let spans: Vec<Range> = dead.iter().map(|(_, r)| *r).collect();
                    let (noun, verb) = if many { ("variants", "are") } else { ("variant", "is") };
                    let mut d = self.diag_spans(&spans, format!("{noun} {} {verb} never constructed", list(&names)));
                    d.spans.push(self.span(owner_s, owner_e, format!("{noun} in this enum")));
                    d.notes.extend(derived_note(&it.name, &ignored));
                    found.push(d);
                }
                if reads_all.contains(&it.name) {
                    continue;
                }
                for v in &variants {
                    let Some(vn) = v.name() else { continue };
                    let vname = vn.text().to_string();
                    if !uses.constructed.contains(&vname) {
                        continue;
                    }
                    if let Some(d) = self.unread_fields(&vname, range(vn.syntax()), v.field_list(), "variant", &it.name, &ignored, &uses) {
                        found.push(d);
                    }
                }
            } else if let Some(s) = ast::Struct::cast(it.node.clone()) {
                if reads_all.contains(&it.name) || !uses.constructed.contains(&it.name) {
                    continue;
                }
                if let Some(d) = self.unread_fields(&it.name, (owner_s, owner_e), s.field_list(), "struct", &it.name, &ignored, &uses) {
                    found.push(d);
                }
            }
        }
        for d in found {
            self.emit("dead_code", d);
        }
    }

    /// The fields of a struct or variant (`owner`, of the type `ty_name`) that nothing reads, as
    /// rustc reports them: named ones at their names; tuple ones at their types, with rustc's
    /// advice (remove them, when they're the last ones of a struct; else make them `()`).
    #[allow(clippy::too_many_arguments)]
    fn unread_fields(
        &self,
        owner: &str,
        owner_at: Range,
        fields: Option<ast::FieldList>,
        what: &str,
        ty_name: &str,
        ignored: &[String],
        uses: &Uses,
    ) -> Option<Diagnostic> {
        // (name, span, index)
        let (all, tuple): (Vec<(String, Range)>, bool) = match fields? {
            ast::FieldList::RecordFieldList(fl) => (
                fl.fields()
                    .filter(|f| !self.skipped(f.syntax()))
                    .filter_map(|f| f.name())
                    .map(|n| (n.text().to_string(), range(n.syntax())))
                    .collect(),
                false,
            ),
            ast::FieldList::TupleFieldList(fl) => (
                fl.fields()
                    .enumerate()
                    .filter_map(|(i, f)| Some((i.to_string(), range(f.ty()?.syntax()))))
                    .collect(),
                true,
            ),
        };
        let dead: Vec<usize> = (0..all.len())
            .filter(|&i| !uses.field_read(owner, &all[i].0) && !all[i].0.starts_with('_'))
            .collect();
        if dead.is_empty() {
            return None;
        }
        let many = dead.len() > 1;
        let names: Vec<String> = dead.iter().map(|&i| all[i].0.clone()).collect();
        let spans: Vec<Range> = dead.iter().map(|&i| all[i].1).collect();
        let (noun, verb) = if many { ("fields", "are") } else { ("field", "is") };
        let mut d = self.diag_spans(&spans, format!("{noun} {} {verb} never read", list(&names)));
        d.spans.push(self.span(owner_at.0, owner_at.1, format!("{noun} in this {what}")));
        let trailing = dead.iter().enumerate().all(|(k, &i)| i == all.len() - dead.len() + k);
        let removable = tuple && what == "struct" && trailing;
        if removable {
            d.helps.push(format!("consider removing {}", if many { "these fields" } else { "this field" }));
        }
        d.notes.extend(derived_note(ty_name, ignored));
        if tuple && !removable {
            let (it, them) = if many { ("fields", "fields") } else { ("field", "field") };
            let edits: Vec<(usize, usize, &str)> = spans.iter().map(|&(s, e)| (s, e, "()")).collect();
            d.children.push(self.edit_child(
                &format!("consider changing the {it} to be of unit type to suppress this warning while preserving the field numbering, or remove the {them}"),
                &edits,
            ));
        }
        Some(d)
    }
}

// ---------------------------------------------------------------------------- late lints

/// The iterator adapters and the `#[must_use]` structs they return.
const ADAPTERS: &[(&str, &str)] = &[
    ("map", "Map"), ("filter", "Filter"), ("filter_map", "FilterMap"), ("enumerate", "Enumerate"),
    ("zip", "Zip"), ("rev", "Rev"), ("skip", "Skip"), ("take", "Take"), ("chain", "Chain"),
    ("cloned", "Cloned"), ("copied", "Copied"), ("flat_map", "FlatMap"), ("flatten", "Flatten"),
    ("inspect", "Inspect"), ("peekable", "Peekable"), ("skip_while", "SkipWhile"),
    ("take_while", "TakeWhile"), ("step_by", "StepBy"), ("scan", "Scan"), ("fuse", "Fuse"),
    ("map_while", "MapWhile"), ("cycle", "Cycle"),
];

fn int_name(k: IntKind) -> &'static str {
    match k {
        IntKind::I64 => "i64",
        IntKind::U64 => "u64",
        IntKind::Isize => "isize",
        IntKind::Usize => "usize",
        _ => "",
    }
}

/// An integer literal's value (`0x100_0000_01b3u64` and the like).
fn int_literal(text: &str) -> Option<u128> {
    let t: String = text.chars().filter(|&c| c != '_').collect();
    let (digits, radix) = if let Some(h) = t.strip_prefix("0x") {
        (h, 16)
    } else if let Some(b) = t.strip_prefix("0b") {
        (b, 2)
    } else if let Some(o) = t.strip_prefix("0o") {
        (o, 8)
    } else {
        (t.as_str(), 10)
    };
    let end = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    u128::from_str_radix(&digits[..end], radix).ok()
}

impl Linter<'_> {
    /// `unused_must_use`: a statement that drops a `Result` or an unconsumed iterator.
    fn must_use(&mut self) {
        let mut found: Vec<Diagnostic> = Vec::new();
        for es in self.file.syntax().descendants().filter_map(ast::ExprStmt::cast) {
            if es.semicolon_token().is_none() || self.skipped(es.syntax()) {
                continue;
            }
            let Some(e) = es.expr() else { continue };
            let r = range(e.syntax());
            let (msg, note): (String, Option<String>) = match &e {
                ast::Expr::MacroExpr(m) => {
                    let name = m.macro_call().and_then(|mc| crate::macros::macro_name(&mc)).unwrap_or_default();
                    if !matches!(name.as_str(), "write" | "writeln") {
                        continue;
                    }
                    ("unused `Result` that must be used".to_string(), Some("this `Result` may be an `Err` variant, which should be handled".to_string()))
                }
                ast::Expr::CallExpr(_) | ast::Expr::MethodCallExpr(_) => match self.ty(r) {
                    Some(Ty::Result(..)) => {
                        ("unused `Result` that must be used".to_string(), Some("this `Result` may be an `Err` variant, which should be handled".to_string()))
                    }
                    Some(Ty::Iter { .. }) => {
                        let ast::Expr::MethodCallExpr(m) = &e else { continue };
                        let name = m.name_ref().map(|n| n.text().to_string()).unwrap_or_default();
                        let Some((_, adapter)) = ADAPTERS.iter().find(|(a, _)| *a == name) else { continue };
                        (format!("unused `{adapter}` that must be used"), Some("iterators are lazy and do nothing unless consumed".to_string()))
                    }
                    _ => match self.own_must_use(&e) {
                        Some(x) => x,
                        None => continue,
                    },
                },
                _ => continue,
            };
            let mut d = self.diag(r.0, r.1, msg);
            d.notes.extend(note);
            d.children.push(self.edit_child("use `let _ = ...` to ignore the resulting value", &[(r.0, r.0, "let _ = ")]));
            found.push(d);
        }
        for d in found {
            self.emit("unused_must_use", d);
        }
    }

    /// The expressions of the file, and of the format-style macros' arguments (with the offset
    /// of the text each was parsed from).
    fn all_nodes(&self) -> Vec<(SyntaxNode, usize)> {
        let mut out = Vec::new();
        fn walk(l: &Linter, root: &SyntaxNode, base: usize, out: &mut Vec<(SyntaxNode, usize)>) {
            for n in root.descendants() {
                if let Some(mc) = ast::MacroCall::cast(n.clone())
                    && crate::macros::macro_name(&mc).is_some_and(|m| format_macro(&m))
                {
                    for a in crate::macros::split_args(l.src, base, &mc) {
                        if let (Some(x), errors) = crate::macros::parse_arg(&a)
                            && errors.is_empty()
                        {
                            walk(l, x.syntax(), a.start, out);
                        }
                    }
                }
                out.push((n, base));
            }
        }
        walk(self, self.file.syntax(), 0, &mut out);
        out
    }

    /// `inexact_64bit`: arithmetic meant to wrap at 2^64 on a 64-bit integer, which is a JS
    /// number here, exact only to 2^53: the `wrapping_*` / `overflowing_*` methods, and
    /// multiplying by a constant past 2^32 (hash and random number generator code).
    fn inexact_64bit(&mut self) {
        let mut found: Vec<(Range, String)> = Vec::new();
        for (n, base) in self.all_nodes() {
            let at = |n: &SyntaxNode| {
                let (s, e) = range(n);
                (s + base, e + base)
            };
            if let Some(m) = ast::MethodCallExpr::cast(n.clone()) {
                let Some(name) = m.name_ref().map(|n| n.text().to_string()) else { continue };
                let wraps = (name.starts_with("wrapping_") || name.starts_with("overflowing_"))
                    && !name.ends_with("_shl")
                    && !name.ends_with("_shr");
                if !wraps {
                    continue;
                }
                let Some(recv) = m.receiver() else { continue };
                // (Literals only: `3u64.wrapping_add(4)` is exact.)
                let literal = |e: &ast::Expr| e.syntax().descendants().all(|n| !matches!(n.kind(), K::PATH_EXPR | K::CALL_EXPR | K::FIELD_EXPR | K::INDEX_EXPR) && (n.kind() != K::METHOD_CALL_EXPR || n == *m.syntax()));
                if literal(&recv) && m.arg_list().is_none_or(|a| a.args().all(|x| literal(&x))) {
                    continue;
                }
                let Some(Ty::Int(k)) = self.ty(at(recv.syntax())) else { continue };
                let t = int_name(*k);
                if t.is_empty() || (base == 0 && self.skipped(&n)) {
                    continue;
                }
                found.push((at(m.syntax()), format!("`{t}` is exact only up to 2^53 here, so `{name}` doesn't wrap at 2^64")));
            } else if let Some(b) = ast::BinExpr::cast(n.clone()) {
                let mul = matches!(
                    b.op_kind(),
                    Some(ast::BinaryOp::ArithOp(ast::ArithOp::Mul))
                        | Some(ast::BinaryOp::Assignment { op: Some(ast::ArithOp::Mul) })
                );
                let (Some(lhs), Some(rhs)) = (b.lhs(), b.rhs()) else { continue };
                if !mul {
                    continue;
                }
                let big = |e: &ast::Expr| matches!(e, ast::Expr::Literal(l) if int_literal(&l.syntax().text().to_string()).is_some_and(|v| v > 1 << 32));
                if !big(&lhs) && !big(&rhs) {
                    continue;
                }
                let typed = if matches!(b.op_kind(), Some(ast::BinaryOp::Assignment { .. })) { lhs.syntax().clone() } else { b.syntax().clone() };
                let Some(Ty::Int(k)) = self.ty(at(&typed)) else { continue };
                let t = int_name(*k);
                if t.is_empty() || (base == 0 && self.skipped(&n)) {
                    continue;
                }
                found.push((at(b.syntax()), format!("`{t}` is exact only up to 2^53 here, so this product doesn't wrap at 2^64")));
            }
        }
        // (One warning for a chain, `a.wrapping_mul(b).wrapping_add(c)`: the whole of it.)
        found.dedup_by_key(|(r, _)| r.0);
        for ((s, e), label) in found {
            let mut d = self.diag(s, e, "64-bit wrapping arithmetic isn't exact here");
            d.label = Some(label);
            d.helps.push("declare it `u128` (exact, wrapping at 2^128) and mask the result to 64 bits with `& 0xffff_ffff_ffff_ffff`".into());
            self.emit("inexact_64bit", d);
        }
    }
}

/// The checker's and codegen's own warnings that are lints: a dropped `Future`
/// (`unused_must_use`, with rustc's note) and a 64-bit literal past 2^53 (`inexact_64bit`).
/// They take the level `#[allow(..)]` and the like give them.
pub fn adopt(src: &str, diags: &mut Vec<Diagnostic>) {
    const FUTURE: &str = "unused implementer of `Future` that must be used";
    const LITERAL: &str = "integer literal exceeds 2^53 and loses precision (64-bit integers are exact to 2^53 here)";
    if !diags.iter().any(|d| d.message == FUTURE || d.message == LITERAL) {
        return;
    }
    let file = crate::macros::parse_tree(src);
    let empty = crate::typeck::Types::default();
    let cx = Cx { types: &empty, map: None, mut_roots: HashSet::new(), resolver: &|_| None };
    let l = Linter {
        src,
        idx: LineIndex::new(src),
        file,
        cx: &cx,
        out: Vec::new(),
        own_mut_methods: HashSet::new(),
        never_fns: HashSet::new(),
        own: Default::default(),
    };
    diags.retain_mut(|d| {
        let lint = match d.message.as_str() {
            FUTURE => "unused_must_use",
            LITERAL => "inexact_64bit",
            _ => return true,
        };
        if d.lint.is_some() {
            return true;
        }
        let Some((severity, attr)) = l.level(d.start, lint) else { return false };
        d.severity = severity;
        d.lint = Some(lint.into());
        d.lint_attr = attr;
        if lint == "unused_must_use" {
            d.notes.push("futures do nothing unless you `.await` or poll them".into());
        } else {
            d.message = "integer literal exceeds 2^53 and loses precision".into();
            d.helps.push("64-bit integers are exact to 2^53 here".into());
        }
        true
    });
}

/// An attribute's arguments (`(unused, dead_code)` of `#[allow(unused, dead_code)]`).
fn attr_tt(a: &ast::Attr) -> Option<ast::TokenTree> {
    a.syntax().descendants().find_map(ast::TokenTree::cast)
}

/// Whether `text` is a macro call (`vec![..]`, `format!(..)`, `std::vec![..]`).
fn is_macro_call(text: &str) -> bool {
    let t = text.trim_start();
    let end = t
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(t.len());
    end > 0 && t[end..].starts_with('!')
}

impl Linter<'_> {
    /// The uses in a macro call's arguments: a format-style macro's are expressions, read with
    /// their types like the code around them; any other macro's tokens count by name.
    fn macro_uses(&self, tt: &ast::TokenTree, base: usize, stop: &HashSet<Range>, uses: &mut Uses) {
        let mc = tt.syntax().parent().and_then(ast::MacroCall::cast);
        let Some(mc) = mc.filter(|mc| crate::macros::macro_name(mc).is_some_and(|m| format_macro(&m) && m != "vec")) else {
            self.token_tree_uses(tt, uses);
            return;
        };
        // (A re-parsed argument doesn't know its `impl`: its `Self` is the impl's type.)
        let self_ty = mc.syntax().ancestors().find_map(ast::Impl::cast).and_then(|i| impl_self_name(&i));
        for a in crate::macros::split_args(self.src, base, &mc) {
            if let Some(s) = &self_ty
                && a.text.contains("Self")
            {
                uses.mentioned.insert(s.clone());
                uses.constructed.insert(s.clone());
            }
            match crate::macros::parse_arg(&a) {
                (Some(x), errors) if errors.is_empty() => {
                    self.scan_uses_at(x.syntax(), a.start, stop, uses);
                    if let Some(s) = crate::macros::format_string(&a)
                        && let Ok(pieces) = crate::fmt::parse(&s)
                    {
                        for p in pieces {
                            let crate::fmt::Piece::Arg { arg, spec } = p else { continue };
                            for r in [Some(arg), spec.width_arg, spec.precision_arg].into_iter().flatten() {
                                if let crate::fmt::ArgRef::Name(n) = r {
                                    uses.mentioned.insert(n.clone());
                                    uses.constructed.insert(n);
                                }
                            }
                        }
                    }
                }
                // (Not an expression: its names count.)
                _ => {
                    let parsed = ast::SourceFile::parse(&format!("m!({});", a.text), crate::macros::EDITION).tree();
                    if let Some(inner) = parsed.syntax().descendants().find_map(ast::TokenTree::cast) {
                        self.token_tree_uses(&inner, uses);
                    }
                }
            }
        }
    }
}

/// The nodes the naming lints look at.
const NAMED_KINDS: &[K] = &[
    K::STRUCT, K::ENUM, K::UNION, K::TYPE_ALIAS, K::TRAIT, K::VARIANT, K::TYPE_PARAM, K::FN,
    K::MODULE, K::RECORD_FIELD, K::CONST, K::STATIC,
];

/// rustc's note on dead code in a type with derived `Debug` / `Clone` (`ignored`), whose uses
/// don't count.
fn derived_note(ty_name: &str, ignored: &[String]) -> Option<String> {
    match ignored {
        [] => None,
        [one] => Some(format!(
            "`{ty_name}` has a derived impl for the trait `{one}`, but this is intentionally ignored during dead code analysis"
        )),
        many => Some(format!(
            "`{ty_name}` has derived impls for the traits {}, but these are intentionally ignored during dead code analysis",
            list(many)
        )),
    }
}
