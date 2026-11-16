//! Std gaps filled before type checking: `std::path` (`Path` / `PathBuf` as the path prelude's
//! `PathBuf`, `OsStr` / `OsString` as `String`), `fmt::Arguments` and `format_args!` (a
//! `String` and `format!`), `env!` / `option_env!` (a script has no build environment: placeholders for Cargo's variables) and
//! `include!`, as a pre-pass over the source tokens (so it reaches macro arguments too); and the
//! Rust-source preludes for `PhantomData` and `std::path`. An `OffsetMap` keeps positions pointing
//! at the original text.

use ra_ap_syntax::{
    AstNode, AstToken, SourceFile, SyntaxKind, TextRange,
    ast::{self, HasArgList},
};

use crate::macros::EDITION;
use crate::mbe::OffsetMap;

const MARKER: &str = include_str!("marker_prelude.rs");
const PATH: &str = include_str!("path_prelude.rs");

/// Whether `ty` is declared in `text` (so a std name of that spelling isn't std's).
fn declares(text: &str, name: &str) -> bool {
    ["struct", "enum", "type", "trait", "union"]
        .iter()
        .any(|kw| text.contains(&format!("{kw} {name}")))
}

/// The preludes `text` needs (after `stdtime`'s), when it names their types.
pub(crate) fn preludes(text: &str) -> String {
    let mut out = String::new();
    let path = (text.contains("PathBuf") || text.contains("__AsPath"))
        && !text.contains("// (path prelude)")
        && !declares(text, "PathBuf");
    // (The path prelude's `FromStr` names `Infallible`.)
    if (text.contains("PhantomData") || text.contains("Infallible") || path)
        && !text.contains("// (marker prelude)")
        && !declares(text, "PhantomData")
        && !declares(text, "Infallible")
    {
        out.push('\n');
        out.push_str(MARKER);
    }
    if path {
        out.push('\n');
        out.push_str(PATH);
    }
    out
}

/// Whether the pre-pass has anything to do in `text`.
pub(crate) fn has_rewrites(text: &str) -> bool {
    [
        "Path",
        "OsStr",
        "OsString",
        "Arguments",
        "format_args",
        "env!",
        "include!",
        "fmt::format",
        "write_fmt",
        "NonZero",
        "Ref",
        "Infallible",
        "Read",
        "Cursor",
    ]
    .iter()
    .any(|n| text.contains(n))
}

/// The std module a name this pass rewrites lives in.
fn std_module(name: &str) -> Option<&'static str> {
    Some(match name {
        "Path" | "PathBuf" => "path",
        "OsStr" | "OsString" => "ffi",
        "Arguments" => "fmt",
        "Ref" | "RefMut" => "cell",
        "Infallible" => "convert",
        "Read" | "BufRead" | "Cursor" => "io",
        n if nonzero_int(n).is_some() || n == "NonZero" => "num",
        _ => return None,
    })
}

/// The integer a `NonZero*` type is here: `NonZeroUsize` → `usize`.
fn nonzero_int(name: &str) -> Option<String> {
    let rest = name.strip_prefix("NonZero")?;
    let int = rest.to_ascii_lowercase();
    crate::ty::IntKind::from_name(&int).map(|_| int)
}

/// The std names `use` items bring in: `Path` after `use std::path::{Path, PathBuf};`, all of a
/// module's after `use std::path::*;`.
fn std_imports(file: &SourceFile) -> Vec<String> {
    const ALL: &[&str] = &[
        "Path",
        "PathBuf",
        "OsStr",
        "OsString",
        "Arguments",
        "Ref",
        "RefMut",
        "Infallible",
        "NonZero",
        "NonZeroU8",
        "NonZeroU16",
        "NonZeroU32",
        "NonZeroU64",
        "NonZeroU128",
        "NonZeroUsize",
        "NonZeroI8",
        "NonZeroI16",
        "NonZeroI32",
        "NonZeroI64",
        "NonZeroI128",
        "NonZeroIsize",
        "Read",
        "BufRead",
        "Cursor",
    ];
    let mut out = Vec::new();
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        for (path, rename, star) in crate::typeck::use_leaves(&u) {
            let segs: Vec<&str> = path.iter().map(String::as_str).collect();
            let segs = match segs.as_slice() {
                ["std" | "core" | "alloc", rest @ ..] => rest.to_vec(),
                _ => continue,
            };
            match segs.as_slice() {
                ["io", "prelude"] if star => out.extend(["Read", "BufRead"].map(String::from)),
                [m] if star => out.extend(
                    ALL.iter()
                        .filter(|n| std_module(n) == Some(*m))
                        .map(|n| n.to_string()),
                ),
                [m, n]
                    if !star
                        && std_module(n) == Some(*m)
                        && rename.as_ref().is_none_or(|r| r == n) =>
                {
                    out.push(n.to_string());
                }
                _ => {}
            }
        }
    }
    out
}

/// Rewrite the std names and macros. `Err` for an `env!` (or an `include!` that can't be read).
pub(crate) fn expand(
    text: &mut String,
    map: &mut OffsetMap,
    resolver: Option<crate::typeck::Resolver>,
) -> Result<bool, Vec<(usize, usize, String)>> {
    let file = SourceFile::parse(text, EDITION).tree();
    let imports = std_imports(&file);
    // `include!("items.in");` among the items: its `;` goes too.
    let item_includes: Vec<usize> = file
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|m| {
            m.syntax().parent().is_some_and(|p| {
                ast::SourceFile::can_cast(p.kind()) || ast::ItemList::can_cast(p.kind())
            })
        })
        .filter_map(|m| m.path())
        .map(|p| usize::from(p.syntax().text_range().start()))
        .collect();
    let uses: Vec<TextRange> = file
        .syntax()
        .descendants()
        .filter_map(ast::Use::cast)
        .map(|u| u.syntax().text_range())
        .collect();
    let toks = tokens(&file, &uses);
    let mut edits: Vec<(usize, usize, String)> = write_fmt_calls(&file);
    let txt = |i: usize| toks.get(i).map(|t| t.text.clone()).unwrap_or_default();
    let mut errors = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if t.kind != SyntaxKind::IDENT || t.in_use {
            i += 1;
            continue;
        }
        let name = t.text.clone();
        let (start, end) = (t.start, t.end);
        // ---- macros: `option_env!("X")`, `env!("X")`, `format_args!(..)`, `include!("f")`
        let after_path = i > 0 && matches!(txt(i - 1).as_str(), "::" | ".");
        if !after_path
            && txt(i + 1) == "!"
            && matches!(
                name.as_str(),
                "option_env" | "env" | "format_args" | "include"
            )
            && let Some(close) = closing(&toks, i + 2)
        {
            let call_end = toks[close].end;
            let arg = toks
                .get(i + 3)
                .filter(|a| a.kind == SyntaxKind::STRING && txt(i + 4) == txt(close))
                .and_then(|a| a.string.clone());
            let cargo = arg.as_deref().and_then(cargo_env);
            match name.as_str() {
                "option_env" => {
                    let v = match cargo {
                        Some(v) => format!("Some::<&'static str>({v:?})"),
                        None => "None::<&'static str>".into(),
                    };
                    edits.push((start, call_end, v));
                }
                "env" if let Some(v) = cargo => {
                    edits.push((start, call_end, format!("{v:?}")));
                }
                "env" => errors.push((
                    map.orig(start, false),
                    map.orig(call_end, true),
                    match arg {
                        Some(v) => format!("environment variable `{v}` not defined at compile time (a script has no build environment; `option_env!` is `None`)"),
                        None => "`env!` takes the variable's name".into(),
                    },
                )),
                "format_args" => {
                    edits.push((start, end, "format".into()));
                }
                _ => match (arg, resolver) {
                    (Some(path), Some(r)) => match r(&path.replace('\\', "/")) {
                        Some(content) => {
                            let end = match toks.get(close + 1) {
                                Some(semi) if semi.text == ";" && item_includes.contains(&start) => {
                                    semi.end
                                }
                                _ => call_end,
                            };
                            edits.push((start, end, content));
                        }
                        None => errors.push((
                            map.orig(start, false),
                            map.orig(call_end, true),
                            format!("couldn't read `{path}`: `include!` reads files next to the script"),
                        )),
                    },
                    _ => {
                        i += 1;
                        continue;
                    }
                },
            }
            i = close + 1;
            continue;
        }
        // ---- `fmt::format(args)`: the string itself.
        if name == "format"
            && i >= 2
            && txt(i - 1) == "::"
            && txt(i - 2) == "fmt"
            && txt(i + 1) == "("
        {
            let q = qualifier_start(&toks, i);
            if q.1 {
                edits.push((toks[q.0].start, end, "String::from".into()));
            }
            i += 1;
            continue;
        }
        if std_module(&name).is_none() || txt(i + 1) == "!" || declares(text, &name) {
            i += 1;
            continue;
        }
        // `std::path::Path`, `path::Path`, or `Path` after its `use`.
        let (q, std_path) = qualifier_start(&toks, i);
        let module = std_module(&name).unwrap_or_default();
        let qualified = q < i;
        let ok = if qualified {
            std_path
                && toks[q..i]
                    .iter()
                    .rev()
                    .nth(1)
                    .is_some_and(|m| m.text == module)
        } else {
            !after_path && imports.contains(&name)
        };
        if !ok {
            i += 1;
            continue;
        }
        if let Some(next) = rewrite_other(&toks, q, i, &mut edits) {
            i = next;
            continue;
        }
        let a = toks[q].start;
        let mut b = end;
        let mut to = match name.as_str() {
            "Path" | "PathBuf" => "PathBuf".to_string(),
            _ => "String".to_string(),
        };
        let mut next = i + 1;
        // `Path::new(..)` / `OsStr::new(..)`.
        if txt(i + 1) == "::" && txt(i + 2) == "new" {
            match name.as_str() {
                "Path" => to = "PathBuf::__new".into(),
                "OsStr" => to = "String::from".into(),
                _ => {}
            }
            if matches!(name.as_str(), "Path" | "OsStr") {
                b = toks[i + 2].end;
                next = i + 3;
            }
        }
        // `Arguments<'_>`: a `String` has no lifetime.
        if name == "Arguments"
            && txt(i + 1) == "<"
            && let Some(k) = (i + 2..toks.len().min(i + 6)).find(|&k| txt(k) == ">")
        {
            b = toks[k].end;
            next = k + 1;
        }
        // `impl AsRef<Path>` / `P: AsRef<Path>`: a string or a path.
        if name == "Path"
            && !qualified
            && i >= 2
            && txt(i - 1) == "<"
            && txt(i - 2) == "AsRef"
            && txt(i + 1) == ">"
        {
            edits.push((toks[i - 2].start, toks[i + 1].end, "__AsPath".into()));
            i += 2;
            continue;
        }
        edits.push((a, b, to));
        i = next;
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // (From the end; at one offset, a replacement before an insertion.)
    edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.1)));
    edits.dedup();
    let changed = !edits.is_empty();
    for (a, b, new) in edits {
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
    }
    Ok(changed)
}

/// `NonZero*`, `Ref` / `RefMut` and `Infallible` (the std name at `toks[i]`, its path starting at
/// `q`): the edits, and the token to go on from; `None` for the other names.
fn rewrite_other(
    toks: &[Tok],
    q: usize,
    i: usize,
    edits: &mut Vec<(usize, usize, String)>,
) -> Option<usize> {
    let name = toks[i].text.as_str();
    let txt = |k: usize| toks.get(k).map(|t| t.text.as_str()).unwrap_or_default();
    let (a, end) = (toks[q].start, toks[i].end);
    // `NonZeroU32` is a `u32`; `NonZeroU32::new(x)` is `u32::__nz_new(x)`, an `Option<u32>`.
    let int = match name {
        "NonZero" if txt(i + 1) == "<" && txt(i + 3) == ">" => {
            let int = txt(i + 2).to_string();
            crate::ty::IntKind::from_name(&int)?;
            edits.push((a, toks[i + 3].end, int.clone()));
            return Some(i + 4);
        }
        "NonZero"
            if txt(i + 1) == "::"
                && txt(i + 2) == "<"
                && txt(i + 4) == ">"
                && txt(i + 5) == "::" =>
        {
            let int = txt(i + 3).to_string();
            crate::ty::IntKind::from_name(&int)?;
            edits.push((a, toks[i + 4].end, int));
            return Some(i + 5);
        }
        "NonZero" => return Some(i + 1),
        n => nonzero_int(n),
    };
    if let Some(int) = int {
        let (to, b, next) = match (txt(i + 1), txt(i + 2)) {
            ("::", f @ ("new" | "new_unchecked")) if txt(i + 3) == "(" => {
                (format!("{int}::__nz_{f}"), toks[i + 2].end, i + 3)
            }
            // (As a value: `.map(NonZeroUsize::new)`.)
            ("::", f @ ("new" | "new_unchecked")) => (
                format!("(|__nz: {int}| {int}::__nz_{f}(__nz))"),
                toks[i + 2].end,
                i + 3,
            ),
            _ => (int, end, i + 1),
        };
        edits.push((a, b, to));
        return Some(next);
    }
    match name {
        // `Ref<'_, T>` / `RefMut<'a, T>`: what `borrow()` / `borrow_mut()` give, the `T`.
        "Ref" | "RefMut" => {
            if txt(i + 1) != "<" {
                return Some(i + 1);
            }
            // The `>` closing it (maybe half of a `>>`).
            let mut depth = 0;
            let mut close = None;
            for (k, t) in toks.iter().enumerate().skip(i + 1) {
                let before = depth;
                match t.text.as_str() {
                    "<" => depth += 1,
                    ">" => depth -= 1,
                    ">>" => depth -= 2,
                    ">>=" | ">=" | "->" | ";" | "{" => break,
                    _ => {}
                }
                if depth <= 0 {
                    let at = match (t.text.as_str(), before) {
                        (">>", 2) => t.end - 1,
                        _ => t.start,
                    };
                    close = Some((k, at));
                    break;
                }
            }
            let (close, at) = close?;
            let mut inner = i + 2;
            if toks
                .get(inner)
                .is_some_and(|t| t.kind == SyntaxKind::LIFETIME_IDENT)
            {
                inner += 1;
                if txt(inner) == "," {
                    inner += 1;
                }
            }
            if inner >= close {
                return Some(i + 1);
            }
            edits.push((a, toks[inner].start, String::new()));
            edits.push((at, at + 1, String::new()));
            Some(inner)
        }
        // `io::Read` / `BufRead` / `Cursor`: the io prelude's. (`Cursor::new(Vec::new())` is a
        // `Cursor<Vec<u8>>`, as its `Write` impl would say.)
        "Read" | "BufRead" | "Cursor" => {
            edits.push((a, end, format!("Io{name}")));
            if name == "Cursor"
                && (1..=8)
                    .map(txt_at(toks, i))
                    .eq(["::", "new", "(", "Vec", "::", "new", "(", ")"])
            {
                edits.push((toks[i + 4].end, toks[i + 4].end, "::<u8>".into()));
            }
            Some(i + 1)
        }
        // `std::convert::Infallible`: the prelude's.
        "Infallible" => {
            if q < i {
                edits.push((a, end, "Infallible".into()));
            }
            Some(i + 1)
        }
        _ => None,
    }
}

/// The text of the token `k` after `i` (empty past the end).
fn txt_at<'a>(toks: &'a [Tok], i: usize) -> impl Fn(usize) -> &'a str {
    move |k| toks.get(i + k).map(|t| t.text.as_str()).unwrap_or_default()
}

/// A token, with `::` as one even inside a macro's token tree (where it's two `:`s).
struct Tok {
    text: String,
    kind: SyntaxKind,
    start: usize,
    end: usize,
    in_use: bool,
    /// A string literal's value.
    string: Option<String>,
}

fn tokens(file: &SourceFile, uses: &[TextRange]) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    for t in file
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
    {
        let r = t.text_range();
        let (start, end) = (usize::from(r.start()), usize::from(r.end()));
        if t.kind() == SyntaxKind::COLON
            && let Some(prev) = out.last_mut()
            && prev.text == ":"
            && prev.end == start
        {
            prev.text = "::".into();
            prev.end = end;
            continue;
        }
        let string =
            ast::String::cast(t.clone()).and_then(|s| s.value().ok().map(|v| v.to_string()));
        out.push(Tok {
            text: t.text().to_string(),
            kind: t.kind(),
            start,
            end,
            in_use: uses.iter().any(|u| u.contains_range(r)),
            string,
        });
    }
    out
}

/// `w.write_fmt(args)` as `write!(w, "{}", args)` (a `String`, a `Formatter` or an `io::Write`).
fn write_fmt_calls(file: &SourceFile) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for m in file
        .syntax()
        .descendants()
        .filter_map(ast::MethodCallExpr::cast)
    {
        let (Some(name), Some(recv), Some(list)) = (m.name_ref(), m.receiver(), m.arg_list())
        else {
            continue;
        };
        let args: Vec<ast::Expr> = list.args().collect();
        if name.text() != "write_fmt" || args.len() != 1 {
            continue;
        }
        let r = recv.syntax().text_range();
        let a = args[0].syntax().text_range();
        out.push((
            usize::from(r.start()),
            usize::from(r.start()),
            "write!(".into(),
        ));
        out.push((
            usize::from(r.end()),
            usize::from(a.start()),
            ", \"{}\", ".into(),
        ));
    }
    out
}

/// The index of the token closing the delimiter at `open`.
/// Placeholders for the variables Cargo sets when it builds a crate (a script is a crate named
/// `script`, version 0.1.0); `None` for any other variable.
fn cargo_env(name: &str) -> Option<&'static str> {
    Some(match name {
        "CARGO_PKG_VERSION" => "0.1.0",
        "CARGO_PKG_VERSION_MAJOR" | "CARGO_PKG_VERSION_PATCH" => "0",
        "CARGO_PKG_VERSION_MINOR" => "1",
        "CARGO_PKG_NAME" | "CARGO_CRATE_NAME" | "CARGO_BIN_NAME" => "script",
        "CARGO_MANIFEST_DIR" => ".",
        "CARGO_PKG_VERSION_PRE"
        | "CARGO_PKG_AUTHORS"
        | "CARGO_PKG_DESCRIPTION"
        | "CARGO_PKG_HOMEPAGE"
        | "CARGO_PKG_REPOSITORY"
        | "CARGO_PKG_LICENSE"
        | "CARGO_PKG_LICENSE_FILE"
        | "CARGO_PKG_README"
        | "CARGO_PKG_RUST_VERSION" => "",
        _ => return None,
    })
}

fn closing(toks: &[Tok], open: usize) -> Option<usize> {
    let o = toks.get(open)?.text.clone();
    let c = match o.as_str() {
        "(" => ")",
        "[" => "]",
        "{" => "}",
        _ => return None,
    };
    let mut depth = 0;
    for (k, t) in toks.iter().enumerate().skip(open) {
        if t.text == o {
            depth += 1;
        } else if t.text == c {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

/// Where the path ending at `toks[i]` starts (`std` of `std::path::Path`), and whether it's a
/// std path (`std::`, `core::`, `::std::`, or a bare module like `path::`).
fn qualifier_start(toks: &[Tok], i: usize) -> (usize, bool) {
    let mut q = i;
    while q >= 2 && toks[q - 1].text == "::" && toks[q - 2].kind == SyntaxKind::IDENT {
        q -= 2;
    }
    let first = toks[q].text.clone();
    let leading = q >= 1 && toks[q - 1].text == "::";
    if leading {
        q -= 1;
    }
    let std = q == i
        || matches!(first.as_str(), "std" | "core" | "alloc")
        || (!leading
            && matches!(
                first.as_str(),
                "path" | "ffi" | "fmt" | "cell" | "num" | "convert" | "io"
            )
            && q + 2 == i);
    (q, std)
}
