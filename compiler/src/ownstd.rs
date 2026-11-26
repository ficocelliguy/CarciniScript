//! A file's own struct or enum named like a std type the compiler builds in (bumpalo's
//! `collections/vec.rs` has `pub struct Vec<'bump, T>`; see `NAMES` for the others). The checker and codegen know `Vec` as
//! std's everywhere, and the preludes compiled in after the program (`Vec::new()` in the io
//! prelude) need it to stay std's, so this pre-pass renames the file's type `Vec__file`, along
//! with every path that names it (`Vec<'b, T>`, `Vec::new_in(b)`, `Vec { .. }`, `self::Vec`).
//! `std::vec::Vec` / `alloc::vec::Vec` stay std's. An `OffsetMap` keeps positions pointing at the
//! original text, and the returned names map the type back to `Vec` for `{:?}`.

use std::collections::HashMap;

use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxToken, TextRange, TextSize, ast, ast::HasName};

use crate::mbe::OffsetMap;

/// The std and bundled-crate names a file's own type may shadow this way (env_logger's and
/// regex-syntax's `Formatter`, winnow's `Range`, jiff's `Error`, bumpalo's `String`, hashbrown's
/// `HashMap`, indexmap's `IndexSet`).
const NAMES: &[&str] = &[
    "Vec", "String", "Formatter", "Range", "RangeInclusive", "Error", "HashMap", "HashSet",
    "BTreeMap", "BTreeSet", "VecDeque", "BinaryHeap", "IndexMap", "IndexSet", "Box", "Rc", "Arc",
    "Weak", "Cow", "Duration", "Instant", "Mutex", "RwLock", "Path", "PathBuf", "Bound",
    "ControlFlow", "Iter", "IterMut", "IntoIter", "Keys", "Values", "ValuesMut", "Drain", "Entry",
    "Lines", "Chars", "Bytes", "Peekable", "Rev", "Take", "Skip", "StepBy", "Fuse",
];

/// The name the file's own `name` goes by.
fn renamed(name: &str) -> String {
    format!("{name}__file")
}

/// Whether `text` may declare its own `Vec` (a cheap check before parsing).
pub(crate) fn has_own_std_types(text: &str) -> bool {
    ["struct ", "enum "].iter().any(|kw| {
        text.match_indices(kw).any(|(i, m)| {
            let rest = &text[i + m.len()..];
            let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len());
            NAMES.contains(&&rest[..end])
        })
    })
}

pub(crate) fn expand(
    text: &mut String,
    map: &mut OffsetMap,
    names: &mut HashMap<String, String>,
) -> bool {
    let file = crate::macros::parse_tree(text);
    // The file's structs and enums with one of `NAMES`, each with where it's in scope: the whole
    // file for a top-level one, the block or module around one declared in a function body or a
    // `#[cfg(test)]` module (other inline modules are flattened by now).
    let mut own: Vec<(String, TextRange)> = Vec::new();
    for n in file.syntax().descendants() {
        let name = match ast::Item::cast(n.clone()) {
            Some(ast::Item::Struct(s)) => s.name(),
            Some(ast::Item::Enum(e)) => e.name(),
            _ => None,
        };
        let Some(name) = name.map(|n| n.text().to_string()) else {
            continue;
        };
        let scope = match n.parent() {
            _ if !NAMES.contains(&name.as_str()) => continue,
            Some(p) if p.kind() == SyntaxKind::SOURCE_FILE => p.text_range(),
            Some(p) if matches!(p.kind(), SyntaxKind::STMT_LIST | SyntaxKind::ITEM_LIST) => {
                p.text_range()
            }
            _ => continue,
        };
        own.push((name, scope));
    }
    if own.is_empty() {
        return false;
    }
    // Whether `name` at `at` names one of them.
    let owns = |name: &str, at: TextSize| own.iter().any(|(n, s)| n == name && s.contains(at));
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for n in file.syntax().descendants() {
        if let Some(name) = ast::Name::cast(n.clone()) {
            // The declaration itself, and its impls' `impl .. for Vec<..>` (paths, below).
            if n.parent()
                .is_some_and(|p| matches!(p.kind(), SyntaxKind::STRUCT | SyntaxKind::ENUM))
                && owns(name.text().as_ref(), n.text_range().start())
            {
                let r = n.text_range();
                edits.push((r.start().into(), r.end().into(), renamed(name.text().as_ref())));
            }
        } else if let Some(p) = ast::Path::cast(n.clone()) {
            // `Vec<T>`, `Vec::new_in(b)`, `Vec { .. }`, and `crate::Vec` / `self::Vec`; not
            // `std::vec::Vec`, and not inside a `use` (`use std::vec::Vec as StdVec;`).
            if n.ancestors().any(|a| ast::Use::can_cast(a.kind())) {
                continue;
            }
            let Some(name) = p.segment().and_then(|s| s.name_ref()) else {
                continue;
            };
            if !owns(name.text().as_ref(), name.syntax().text_range().start()) {
                continue;
            }
            let rooted = match p.qualifier() {
                None => true,
                Some(q) => matches!(q.syntax().text().to_string().as_str(), "crate" | "self"),
            };
            if rooted {
                let r = name.syntax().text_range();
                edits.push((r.start().into(), r.end().into(), renamed(name.text().as_ref())));
            }
        } else if n.kind() == SyntaxKind::TOKEN_TREE
            && !n.ancestors().any(|a| ast::Attr::can_cast(a.kind()))
        {
            // Macro arguments and `macro_rules!` bodies: `assert_eq!(v, Vec::new_in(&b))`.
            let toks: Vec<SyntaxToken> = n
                .children_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| !t.kind().is_trivia())
                .collect();
            let text_at = |j: Option<usize>| j.and_then(|j| toks.get(j)).map(|t| t.text());
            for (i, t) in toks.iter().enumerate() {
                if t.kind() != SyntaxKind::IDENT || !owns(t.text(), t.text_range().start()) {
                    continue;
                }
                // (`std::vec::Vec`, `$crate::collections::Vec`: another path's.)
                let after_path = text_at(i.checked_sub(1)) == Some("::")
                    || (text_at(i.checked_sub(1)) == Some(":") && text_at(i.checked_sub(2)) == Some(":"));
                if after_path || text_at(i.checked_sub(1)) == Some(".") {
                    continue;
                }
                let r = t.text_range();
                edits.push((r.start().into(), r.end().into(), renamed(t.text())));
            }
        }
    }
    if edits.is_empty() {
        return false;
    }
    for (n, _) in &own {
        names.insert(renamed(n), n.clone());
    }
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    let mut last_start = usize::MAX;
    for (a, b, new) in edits {
        if b > last_start {
            continue;
        }
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
        last_start = a;
    }
    true
}
