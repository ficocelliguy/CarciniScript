//! A struct or enum of a `mod` file named like a type of the file that loads it (`mod bytes;`
//! then `pub struct Shlex(bytes::Shlex)`), or like one an earlier `mod` file exports. Types of
//! every file share one namespace in the checker, so the module's type goes by a mangled name
//! there, `bytes__Shlex` (as an inline module's items do, see `modflat`): this pre-pass writes
//! the loading file's paths to it that way (`bytes::Shlex` → `bytes::bytes__Shlex`), and
//! `typeck_mod` reads the module's exports from its text with the same rename. Its JS class
//! keeps its name (`bytes.Shlex`), through `Adts::js_names`.

use std::collections::HashSet;

use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxToken,
    ast::{self, HasModuleItem, HasName, HasVisibility},
};

use crate::mbe::OffsetMap;

/// The mangled name of `module`'s type `name` (`Name__a__b` for `a.b`): still capitalized, so
/// it reads as a type wherever the compiler goes by the first letter.
pub fn mangle(module: &str, name: &str) -> String {
    format!("{name}__{}", module.replace('.', "__"))
}

/// The kinds and texts of the `n` tokens before `tok`, nearest first, without whitespace and
/// comments; `:` `:` in a macro's arguments counts as one `::`.
fn prev_tokens(tok: &SyntaxToken, n: usize) -> Vec<(SyntaxKind, SyntaxToken)> {
    let mut out = Vec::new();
    let mut cur = tok.prev_token();
    while let Some(t) = cur {
        if out.len() == n {
            break;
        }
        cur = t.prev_token();
        if t.kind() == SyntaxKind::COLON
            && let Some(p) = cur.clone()
            && p.kind() == SyntaxKind::COLON
        {
            cur = p.prev_token();
            out.push((SyntaxKind::COLON2, p));
        } else if !t.kind().is_trivia() {
            out.push((t.kind(), t.clone()));
        }
    }
    out
}

/// Whether `t` starts a path from the crate root: `crate` / `self` with no `::` before it.
fn root_kw(t: &(SyntaxKind, SyntaxToken)) -> bool {
    matches!(t.0, SyntaxKind::CRATE_KW | SyntaxKind::SELF_KW)
        && prev_tokens(&t.1, 1)
            .first()
            .is_none_or(|p| p.0 != SyntaxKind::COLON2)
}

/// Whether `text` may load a `mod` file.
pub fn has_mod_files(text: &str) -> bool {
    text.contains("mod ")
}

/// The names the file's own top-level structs, enums, unions, traits and type aliases take.
pub fn local_type_names(file: &ast::SourceFile) -> HashSet<String> {
    file.items()
        .filter_map(|i| match i {
            ast::Item::Struct(s) => s.name(),
            ast::Item::Enum(e) => e.name(),
            ast::Item::Union(u) => u.name(),
            ast::Item::Trait(t) => t.name(),
            ast::Item::TypeAlias(t) => t.name(),
            _ => None,
        })
        .map(|n| n.text().to_string())
        .collect()
}

/// A module file's public top-level structs and enums.
pub fn pub_adt_names(text: &str) -> Vec<String> {
    let file = crate::macros::parse_tree(text);
    file.items()
        .filter(|i| !crate::adt::test_only(i.syntax()))
        .filter_map(|i| match i {
            ast::Item::Struct(s) if s.visibility().is_some() => s.name(),
            ast::Item::Enum(e) if e.visibility().is_some() => e.name(),
            _ => None,
        })
        .map(|n| n.text().to_string())
        .collect()
}

/// Of a module's types `exported`, the ones that clash: named like one of `taken` (the loading
/// file's own types and earlier modules' types), which it then joins.
pub fn clashing(exported: &[String], taken: &mut HashSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    for n in exported {
        if !taken.insert(n.clone()) {
            out.push(n.clone());
        }
    }
    out
}

/// The `mod` files of `text`, in order, with the types of each that clash.
fn file_clashes(
    file: &ast::SourceFile,
    filename: &str,
    text: &str,
    resolver: crate::typeck::Resolver,
) -> Vec<(String, Vec<String>)> {
    let mut taken = local_type_names(file);
    let mut out = Vec::new();
    for item in file.items() {
        let ast::Item::Module(m) = item else { continue };
        if m.item_list().is_some() || crate::adt::test_only(m.syntax()) {
            continue;
        }
        let Some(n) = m.name() else { continue };
        let n = n.text().to_string();
        let Some((_, src, _)) = crate::typeck::find_module_in(filename, text, &n, resolver) else {
            continue;
        };
        let names = clashing(&pub_adt_names(&src), &mut taken);
        if !names.is_empty() {
            out.push((n, names));
        }
    }
    out
}

/// Write the loading file's paths to clashing module types by their mangled names:
/// `bytes::Shlex`, `crate::bytes::Shlex::new`, `use bytes::{Shlex as Inner}`.
pub fn expand(
    text: &mut String,
    map: &mut OffsetMap,
    files: Option<(&str, crate::typeck::Resolver)>,
) -> bool {
    let Some((filename, resolver)) = files else {
        return false;
    };
    let file = crate::macros::parse_tree(text);
    let clashes = file_clashes(&file, filename, text, resolver);
    if clashes.is_empty() {
        return false;
    }
    let renamed = |m: &str, n: &str| {
        clashes
            .iter()
            .any(|(cm, names)| cm == m && names.iter().any(|x| x == n))
    };
    let mut edits = Vec::new();
    for seg in file
        .syntax()
        .descendants()
        .filter_map(ast::PathSegment::cast)
    {
        let Some(nr) = seg.name_ref() else { continue };
        let Some(path) = seg.syntax().parent().and_then(ast::Path::cast) else {
            continue;
        };
        // The module the segment is in: the path's qualifier (`bytes::`, `crate::bytes::`), or
        // a `use` tree's prefix (`use bytes::{Shlex}`).
        let module = match path.qualifier() {
            Some(q) => module_of_path(&q),
            None => path
                .syntax()
                .parent()
                .and_then(ast::UseTree::cast)
                .and_then(|t| t.syntax().parent())
                .and_then(ast::UseTreeList::cast)
                .and_then(|l| l.syntax().parent())
                .and_then(ast::UseTree::cast)
                .and_then(|t| t.path())
                .and_then(|p| module_of_path(&p)),
        };
        let Some(module) = module else { continue };
        let n = nr.text().to_string();
        if renamed(&module, &n) {
            let r = nr.syntax().text_range();
            edits.push((
                usize::from(r.start()),
                usize::from(r.end()),
                mangle(&module, &n),
            ));
        }
    }
    // Inside macro arguments (`println!("{}", bytes::Shlex::new(..))`, `vec![..]`): tokens.
    for tok in file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if tok.kind() != SyntaxKind::IDENT
            || tok
                .parent()
                .is_none_or(|p| p.kind() != SyntaxKind::TOKEN_TREE)
        {
            continue;
        }
        let prev = prev_tokens(&tok, 4);
        let [colons, m, rest @ ..] = prev.as_slice() else {
            continue;
        };
        let rooted = match rest {
            [] => true,
            [c, ..] if c.0 != SyntaxKind::COLON2 => true,
            [_, kw, ..] => root_kw(kw),
            _ => false,
        };
        if colons.0 == SyntaxKind::COLON2
            && m.0 == SyntaxKind::IDENT
            && rooted
            && renamed(m.1.text(), tok.text())
        {
            let r = tok.text_range();
            edits.push((
                usize::from(r.start()),
                usize::from(r.end()),
                mangle(m.1.text(), tok.text()),
            ));
        }
    }
    if edits.is_empty() {
        return false;
    }
    crate::alias::apply(text, map, &mut edits);
    true
}

/// `bytes` / `crate::bytes` / `self::bytes`: the module it names.
fn module_of_path(p: &ast::Path) -> Option<String> {
    let mut segs: Vec<String> = Vec::new();
    let mut cur = Some(p.clone());
    while let Some(c) = cur {
        segs.push(c.segment()?.syntax().text().to_string());
        cur = c.qualifier();
    }
    segs.reverse();
    match segs.as_slice() {
        [m] => Some(m.clone()),
        [root, m] if root == "crate" || root == "self" => Some(m.clone()),
        _ => None,
    }
}

/// A module file's text with its types `names` renamed to their mangled names: their
/// definitions and every path to them from the module's own scope (`Shlex`, `Self` stays,
/// `crate::Shlex`), not a variant or field that happens to share the name.
pub fn rename_in_module(text: &str, module: &str, names: &[String]) -> String {
    let file = crate::macros::parse_tree(text);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for tok in file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if tok.kind() != SyntaxKind::IDENT || !names.iter().any(|n| n == tok.text()) {
            continue;
        }
        let Some(parent) = tok.parent() else { continue };
        let rename = match parent.kind() {
            // `pub struct Shlex` / `pub enum Shlex`
            SyntaxKind::NAME => parent
                .parent()
                .is_some_and(|p| matches!(p.kind(), SyntaxKind::STRUCT | SyntaxKind::ENUM)),
            // In macro arguments: not after `.` (a field) or another path segment.
            SyntaxKind::TOKEN_TREE => match prev_tokens(&tok, 2).as_slice() {
                [p, ..] if p.0 == SyntaxKind::DOT => false,
                [p, kw] if p.0 == SyntaxKind::COLON2 => root_kw(kw),
                [p] if p.0 == SyntaxKind::COLON2 => false,
                _ => true,
            },
            SyntaxKind::NAME_REF => parent
                .parent()
                .and_then(ast::PathSegment::cast)
                .and_then(|s| s.syntax().parent())
                .and_then(ast::Path::cast)
                .is_some_and(|p| match p.qualifier() {
                    None => true,
                    Some(q) => matches!(q.syntax().text().to_string().as_str(), "crate" | "self"),
                }),
            _ => false,
        };
        if rename {
            let r = tok.text_range();
            edits.push((
                usize::from(r.start()),
                usize::from(r.end()),
                mangle(module, tok.text()),
            ));
        }
    }
    let mut out = text.to_string();
    let mut map = OffsetMap::identity(text.len());
    crate::alias::apply(&mut out, &mut map, &mut edits);
    out
}
