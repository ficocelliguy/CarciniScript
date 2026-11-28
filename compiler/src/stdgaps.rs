//! Std gaps filled before type checking: `std::path` (`Path` / `PathBuf` as the path prelude's
//! `PathBuf`, `OsStr` / `OsString` as `String`), `fmt::Arguments` and `format_args!` (a
//! `String` and `format!`), `env!` / `option_env!` (a script has no build environment: placeholders for Cargo's variables) and
//! `include!`, as a pre-pass over the source tokens (so it reaches macro arguments too); and the
//! Rust-source preludes for `PhantomData` and `std::path`. An `OffsetMap` keeps positions pointing
//! at the original text.

use ra_ap_syntax::{
    AstNode, AstToken, SourceFile, SyntaxKind, TextRange,
    ast::{self, HasArgList, HasName},
};

use crate::mbe::OffsetMap;

const MARKER: &str = include_str!("marker_prelude.rs");
const PATH: &str = include_str!("path_prelude.rs");
const WRAPPING: &str = include_str!("wrapping_prelude.rs");
const CSTR: &str = include_str!("cstr_prelude.rs");
const LINKED_LIST: &str = include_str!("linked_list_prelude.rs");
const CONTROL_FLOW: &str = include_str!("control_flow_prelude.rs");
const BOUND: &str = include_str!("bound_prelude.rs");
const UNWIND: &str = include_str!("unwind_prelude.rs");
const RANDOM_STATE: &str = include_str!("random_state_prelude.rs");
const INT_ERROR_KIND: &str = include_str!("int_error_kind_prelude.rs");
const FP_CATEGORY: &str = include_str!("fp_category_prelude.rs");
/// `Cow<Path>` conversions, for a program with both the path and the Cow preludes. (`&Path` is
/// a `PathBuf` too, so `Cow::from(&path)` is `Owned`.)
const COW_PATH: &str = "// (cow path prelude)
impl<'a> From<PathBuf> for Cow<'a, PathBuf> {
    fn from(p: PathBuf) -> Cow<'a, PathBuf> {
        Cow::Owned(p)
    }
}

impl<'a> From<Cow<'a, PathBuf>> for PathBuf {
    fn from(c: Cow<'a, PathBuf>) -> PathBuf {
        c.into_owned()
    }
}
";

/// Whether `ty` is declared in `text` (so a std name of that spelling isn't std's).
/// (`struct Reader` doesn't declare `Read`.)
fn declares(text: &str, name: &str) -> bool {
    ["struct", "enum", "type", "trait", "union"]
        .iter()
        .any(|kw| crate::stdtime::has_word(text, &format!("{kw} {name}")))
}

/// The preludes `text` needs (after `stdtime`'s), when it names their types; `paths`: another
/// prelude it gets names `PathBuf`.
pub(crate) fn preludes(text: &str, paths: bool) -> Vec<&'static str> {
    let mut out = Vec::new();
    let path = (paths
        || text.contains("PathBuf")
        || text.contains("__AsPath")
        || text.contains("__os_push")
        || text.contains("__os_from"))
        && !text.contains("// (path prelude)")
        && !declares(text, "PathBuf");
    // (The path prelude's `FromStr` names `Infallible`.)
    if (text.contains("PhantomData")
        || text.contains("Infallible")
        || text.contains("AllocGlobal")
        || path)
        && !text.contains("// (marker prelude)")
        && !declares(text, "PhantomData")
        && !declares(text, "Infallible")
    {
        out.push(MARKER);
    }
    if path {
        out.push(PATH);
        if crate::stdtime::has_word(text, "Cow") && !text.contains("// (cow path prelude)") {
            out.push(COW_PATH);
        }
    }
    if text.contains("Wrapping")
        && !text.contains("// (wrapping prelude)")
        && !declares(text, "Wrapping")
    {
        out.push(WRAPPING);
    }
    if text.contains("NonZero")
        && !text.contains("// (nonzero prelude)")
        && !declares(text, "NonZeroU8")
        && nonzero_newtypes(&crate::macros::parse_tree(text))
    {
        out.push(nonzero_prelude());
    }
    if (crate::stdtime::has_word(text, "CStr") || crate::stdtime::has_word(text, "CString"))
        && !text.contains("// (cstr prelude)")
        && !declares(text, "CStr")
        && !declares(text, "CString")
    {
        out.push(CSTR);
    }
    if crate::stdtime::has_word(text, "LinkedList")
        && !text.contains("// (linked list prelude)")
        && !declares(text, "LinkedList")
    {
        out.push(LINKED_LIST);
    }
    if crate::stdtime::has_word(text, "ControlFlow")
        && !text.contains("// (control flow prelude)")
        && !declares(text, "ControlFlow")
    {
        out.push(CONTROL_FLOW);
    }
    if (crate::stdtime::has_word(text, "Bound") || crate::stdtime::has_word(text, "RangeBounds"))
        && !text.contains("// (bound prelude)")
        && !declares(text, "Bound")
    {
        out.push(BOUND);
    }
    if crate::stdtime::has_word(text, "AssertUnwindSafe")
        && !text.contains("// (unwind prelude)")
        && !declares(text, "AssertUnwindSafe")
    {
        out.push(UNWIND);
    }
    if crate::stdtime::has_word(text, "RandomState")
        && !text.contains("// (random state prelude)")
        && !declares(text, "RandomState")
    {
        out.push(RANDOM_STATE);
    }
    if crate::stdtime::has_word(text, "IntErrorKind")
        && !text.contains("// (int error kind prelude)")
        && !declares(text, "IntErrorKind")
    {
        out.push(INT_ERROR_KIND);
    }
    if (crate::stdtime::has_word(text, "FpCategory") || text.contains(".classify()")
        || text.contains("::classify("))
        && !text.contains("// (fp category prelude)")
        && !declares(text, "FpCategory")
    {
        out.push(FP_CATEGORY);
    }
    out
}

/// Whether `text` may have a C string literal (`c"text"`, `cr"text"`, `cr#"text"#`).
fn has_c_strings(text: &str) -> bool {
    text.contains("c\"") || text.contains("cr#")
}

/// `c"text"` as `CStr::__lit(&[116, 101, 120, 116])` (the cstr prelude's), everywhere: in macro
/// arguments too.
fn c_string_literals(file: &SourceFile) -> Vec<(usize, usize, String)> {
    file.syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter_map(ast::CString::cast)
        .map(|s| {
            let bytes = s.value().map(|v| v.into_owned()).unwrap_or_default();
            let items: Vec<String> = bytes.iter().map(|b| format!("{b}u8")).collect();
            let r = s.syntax().text_range();
            (
                usize::from(r.start()),
                usize::from(r.end()),
                format!("CStr::__lit(&[{}])", items.join(", ")),
            )
        })
        .collect()
}

/// Whether the pre-pass has anything to do in `text`.
pub(crate) fn has_rewrites(text: &str) -> bool {
    // (The names `std_module` and `prelude_type_module` know, as whole words: `Ref` isn't in
    // `RefCell`.)
    [
        "Path",
        "PathBuf",
        "OsStr",
        "OsString",
        "Arguments",
        "Ref",
        "RefMut",
        "MutexGuard",
        "RwLockReadGuard",
        "RwLockWriteGuard",
        "Infallible",
        "Read",
        "BufRead",
        "Cursor",
        "Wrapping",
        "AssertUnwindSafe",
    ]
    .iter()
    .any(|n| crate::stdtime::has_word(text, n))
        || has_c_strings(text)
        || [
            "format_args",
            "env!",
            "concat!",
            "file!",
            "line!",
            "column!",
            "include!",
            "fmt::format",
            "fmt::write(",
            "write_fmt",
            "NonZero",
            "dyn FmtWrite",
            "dyn IoWrite",
            "impl FmtWrite",
            "impl IoWrite",
        ]
        .iter()
        .any(|n| text.contains(n))
        || (text.contains("impl") && text.contains("Into<"))
}

/// The std module a name this pass rewrites lives in.
fn std_module(name: &str) -> Option<&'static str> {
    Some(match name {
        "Path" | "PathBuf" => "path",
        "OsStr" | "OsString" => "ffi",
        "Arguments" => "fmt",
        "Ref" | "RefMut" => "cell",
        "MutexGuard" | "RwLockReadGuard" | "RwLockWriteGuard" => "sync",
        "Infallible" => "convert",
        "Read" | "BufRead" | "Cursor" => "io",
        n if nonzero_int(n).is_some() || n == "NonZero" => "num",
        _ => return None,
    })
}

/// The std module of a type a Rust-source prelude defines under its own name, which a written-out
/// path (`std::num::Wrapping`) drops to.
fn prelude_type_module(name: &str) -> Option<&'static str> {
    Some(match name {
        "Wrapping" => "num",
        "AssertUnwindSafe" => "panic",
        _ => return None,
    })
}

/// The integer a `NonZero*` type is here: `NonZeroUsize` → `usize`.
fn nonzero_int(name: &str) -> Option<String> {
    let rest = name.strip_prefix("NonZero")?;
    let int = rest.to_ascii_lowercase();
    crate::ty::IntKind::from_name(&int).map(|_| int)
}

/// `NonZeroU8` for `u8`.
fn nonzero_name(int: &str) -> String {
    let mut cs = int.chars();
    let first = cs.next().map(|c| c.to_ascii_uppercase()).unwrap_or_default();
    format!("NonZero{first}{}", cs.as_str())
}

/// The integer of an impl's self type when it is one (`u8`) or its `NonZero*` (`NonZeroU8`,
/// `NonZero<u8>`, `std::num::NonZeroU8`), with whether it is the `NonZero*`.
fn impl_int(t: &ast::Type) -> Option<(String, bool)> {
    use ra_ap_syntax::ast::HasGenericArgs;
    let ast::Type::PathType(p) = t else {
        return None;
    };
    let seg = p.path()?.segment()?;
    let name = seg.name_ref()?.text().to_string();
    if crate::ty::IntKind::from_name(&name).is_some() {
        return Some((name, false));
    }
    if name == "NonZero" {
        let arg = seg.generic_arg_list()?.generic_args().next()?.syntax().text().to_string();
        crate::ty::IntKind::from_name(&arg)?;
        return Some((arg, true));
    }
    nonzero_int(&name).map(|i| (i, true))
}

/// Whether the file implements one trait for both an integer and its `NonZero*` (`impl Tr for
/// u8` and `impl Tr for NonZeroU8`). Elsewhere a `NonZeroU8` is its `u8` (no call could tell the
/// impls apart); in such a file the `NonZero*` types are the nonzero prelude's, each a struct of
/// its own, which calls tell apart by class (directly, through `T: Tr` and through `dyn Tr`).
pub(crate) fn nonzero_newtypes(file: &SourceFile) -> bool {
    let mut seen: Vec<(String, String, bool)> = Vec::new();
    for imp in file.syntax().descendants().filter_map(ast::Impl::cast) {
        let (Some(tr), Some(st)) = (imp.trait_(), imp.self_ty()) else {
            continue;
        };
        let Some((int, nz)) = impl_int(&st) else {
            continue;
        };
        let tr = tr.syntax().text().to_string();
        if seen.iter().any(|(t, i, z)| *t == tr && *i == int && *z != nz) {
            return true;
        }
        seen.push((tr, int, nz));
    }
    false
}

/// In a file with the nonzero prelude's types (`nonzero_newtypes`): `NonZero<u8>` and
/// `NonZero::<u8>::` as `NonZeroU8`, and `std::num::NonZeroU8` as the prelude's `NonZeroU8`.
fn rewrite_nonzero_newtype(
    toks: &[Tok],
    q: usize,
    i: usize,
    edits: &mut Vec<(usize, usize, String)>,
) -> Option<usize> {
    let name = toks[i].text.as_str();
    let txt = |k: usize| toks.get(k).map(|t| t.text.as_str()).unwrap_or_default();
    if name == "NonZero" {
        let (int, close, next) = match (txt(i + 1), txt(i + 2)) {
            ("<", int) if txt(i + 3) == ">" => (int, i + 3, i + 4),
            ("::", "<") if txt(i + 4) == ">" => (txt(i + 3), i + 4, i + 5),
            _ => return Some(i + 1),
        };
        crate::ty::IntKind::from_name(int)?;
        edits.push((toks[q].start, toks[close].end, nonzero_name(int)));
        return Some(next);
    }
    nonzero_int(name)?;
    if q < i {
        edits.push((toks[q].start, toks[i].start, String::new()));
    }
    Some(i + 1)
}

/// The nonzero prelude: `NonZeroU8` and the rest as structs over their integer (see
/// `nonzero_newtypes`), with std's methods a script uses.
fn nonzero_prelude() -> &'static str {
    static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    TEXT.get_or_init(|| {
        let mut out = String::from("// (nonzero prelude)\n");
        for int in [
            "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
        ] {
            let signed = int.starts_with('i');
            let mut t = NONZERO.to_string();
            t.push_str(if signed { NONZERO_SIGNED } else { NONZERO_UNSIGNED });
            let min = if signed { "INT::MIN" } else { "1" };
            let bits = match int.trim_start_matches(['i', 'u']) {
                "size" => "64",
                b => b,
            };
            out.push_str(
                &t.replace("NZ", &nonzero_name(int))
                    .replace("MIN_VALUE", min)
                    .replace("BITS_VALUE", bits)
                    .replace("INT", int),
            );
        }
        out
    })
}

/// One `NonZero*` of the nonzero prelude: `NZ` over `INT`.
const NONZERO: &str = r#"
/// `std::num::NZ`: an `INT` that is never zero.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NZ(INT);

impl NZ {
    pub const MIN: NZ = NZ(MIN_VALUE);
    pub const MAX: NZ = NZ(INT::MAX);
    pub const BITS: u32 = BITS_VALUE;
    pub const fn new(n: INT) -> Option<NZ> {
        if n == 0 { None } else { Some(NZ(n)) }
    }
    pub const unsafe fn new_unchecked(n: INT) -> NZ {
        NZ(n)
    }
    pub const fn get(self) -> INT {
        self.0
    }
    pub fn leading_zeros(self) -> u32 {
        self.0.leading_zeros()
    }
    pub fn trailing_zeros(self) -> u32 {
        self.0.trailing_zeros()
    }
    pub fn count_ones(self) -> u32 {
        self.0.count_ones()
    }
    pub fn checked_mul(self, o: NZ) -> Option<NZ> {
        self.0.checked_mul(o.0).map(NZ)
    }
    pub fn saturating_mul(self, o: NZ) -> NZ {
        NZ(self.0.saturating_mul(o.0))
    }
    pub fn checked_pow(self, e: u32) -> Option<NZ> {
        self.0.checked_pow(e).map(NZ)
    }
    pub fn saturating_pow(self, e: u32) -> NZ {
        NZ(self.0.saturating_pow(e))
    }
}

impl std::fmt::Display for NZ {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for NZ {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}

impl From<NZ> for INT {
    fn from(n: NZ) -> INT {
        n.0
    }
}

impl std::ops::BitOr for NZ {
    type Output = NZ;
    fn bitor(self, o: NZ) -> NZ {
        NZ(self.0 | o.0)
    }
}

impl std::ops::BitOr<INT> for NZ {
    type Output = NZ;
    fn bitor(self, o: INT) -> NZ {
        NZ(self.0 | o)
    }
}
"#;

/// The unsigned `NonZero*`'s own methods.
const NONZERO_UNSIGNED: &str = r#"
impl NZ {
    pub fn checked_add(self, o: INT) -> Option<NZ> {
        self.0.checked_add(o).map(NZ)
    }
    pub fn saturating_add(self, o: INT) -> NZ {
        NZ(self.0.saturating_add(o))
    }
    pub fn is_power_of_two(self) -> bool {
        self.0.is_power_of_two()
    }
    pub fn ilog2(self) -> u32 {
        self.0.ilog2()
    }
    pub fn ilog10(self) -> u32 {
        self.0.ilog10()
    }
}
"#;

/// The signed `NonZero*`'s own methods.
const NONZERO_SIGNED: &str = r#"
impl NZ {
    pub fn is_positive(self) -> bool {
        self.0 > 0
    }
    pub fn is_negative(self) -> bool {
        self.0 < 0
    }
    pub fn abs(self) -> NZ {
        NZ(self.0.abs())
    }
    pub fn checked_abs(self) -> Option<NZ> {
        self.0.checked_abs().map(NZ)
    }
    pub fn checked_neg(self) -> Option<NZ> {
        self.0.checked_neg().map(NZ)
    }
}

impl std::ops::Neg for NZ {
    type Output = NZ;
    fn neg(self) -> NZ {
        NZ(-self.0)
    }
}
"#;

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
        "MutexGuard",
        "RwLockReadGuard",
        "RwLockWriteGuard",
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
    site: Option<(&str, &str)>,
) -> Result<bool, Vec<(usize, usize, String)>> {
    let file = crate::macros::parse_tree(text);
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
    let newtypes = text.contains("NonZero") && nonzero_newtypes(&file);
    let mut edits: Vec<(usize, usize, String)> = write_fmt_calls(&file);
    edits.extend(writer_params(&file));
    edits.extend(c_string_literals(&file));
    edits.extend(into_impls(&file, text));
    edits.extend(os_string_pushes(&file, &imports));
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
        // ---- `file!()`, `line!()`, `column!()`: where the call is in the source (the outermost
        // macro call's place inside a `macro_rules!` expansion, as rustc has it).
        if !after_path
            && txt(i + 1) == "!"
            && let Some(v) = location_macro(&toks, i, map, site)
        {
            edits.push((start, toks[i + 3].end, v));
            i += 4;
            continue;
        }
        // ---- `concat!("{}: ", "x", 1)` of literals: one string literal, so it can be a format
        // string (`println!(concat!("[{}] ", $msg), ..)`).
        if !after_path
            && name == "concat"
            && txt(i + 1) == "!"
            && let Some(close) = closing(&toks, i + 2)
            && let Some(v) = concat_literals(&toks[i + 3..close], map, site)
        {
            edits.push((start, toks[close].end, format!("{v:?}")));
            i = close + 1;
            continue;
        }
        // ---- `fmt::write(w, args)`: the prelude's `__fmt_write`, its `write_str` of the text.
        if name == "write"
            && i >= 2
            && txt(i - 1) == "::"
            && txt(i - 2) == "fmt"
            && txt(i + 1) == "("
        {
            let q = qualifier_start(&toks, i);
            if q.1 {
                edits.push((toks[q.0].start, end, "__fmt_write".into()));
            }
            i += 1;
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
        // `std::panic::AssertUnwindSafe(..)` / `std::num::Wrapping<u8>` written out: the
        // prelude's type, by its name.
        if let Some(module) = prelude_type_module(&name)
            && !declares(text, &name)
        {
            let (q, std_path) = qualifier_start(&toks, i);
            if q < i
                && std_path
                && toks[q..i].iter().rev().nth(1).is_some_and(|m| m.text == module)
            {
                edits.push((toks[q].start, toks[i].start, String::new()));
            }
            i += 1;
            continue;
        }
        if std_module(&name).is_none() || txt(i + 1) == "!" {
            i += 1;
            continue;
        }
        // `std::path::Path`, `path::Path`, or `Path` after its `use`.
        let (q, std_path) = qualifier_start(&toks, i);
        let module = std_module(&name).unwrap_or_default();
        let qualified = q < i;
        // (The file's own `struct OsStr` is what a bare `OsStr` names; `std::ffi::OsStr` is
        // still std's.)
        if !(qualified && std_path) && declares(text, &name) {
            i += 1;
            continue;
        }
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
        // (`impl Tr for NonZeroU8` next to `impl Tr for u8`: the nonzero prelude's types.)
        if newtypes
            && let Some(next) = rewrite_nonzero_newtype(&toks, q, i, &mut edits)
        {
            i = next;
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
        // `OsStr::from_bytes(..)` / `OsString::from_vec(..)` (`std::os::unix::ffi`): the path
        // prelude's (lossy, as an `OsStr` is a `String` here).
        if txt(i + 1) == "::"
            && let Some(f) = match (name.as_str(), txt(i + 2).as_str()) {
                ("OsStr", "from_bytes") => Some("__os_from_bytes"),
                ("OsString", "from_vec") => Some("__os_from_vec"),
                _ => None,
            }
        {
            to = f.into();
            b = toks[i + 2].end;
            next = i + 3;
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
    // `impl Tr for PathBuf` next to `impl Tr for Path` (both the path prelude's `PathBuf` here,
    // as `OsString` and `OsStr` are both `String`): the borrowed type's impl serves, the owned
    // one (which usually hands on to it) goes.
    let dropped = owned_twin_impls(&file);
    edits.retain(|(a, b, _)| !dropped.iter().any(|(x, y)| x <= a && b <= y));
    edits.extend(dropped.iter().map(|(x, y)| (*x, *y, String::new())));
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

/// The ranges of `impl Tr for PathBuf` / `impl Tr for OsString` blocks whose trait the file also
/// implements for `Path` / `OsStr`.
fn owned_twin_impls(file: &SourceFile) -> Vec<(usize, usize)> {
    let self_name = |imp: &ast::Impl| {
        let mut t = imp.self_ty()?;
        while let ast::Type::RefType(r) = t {
            t = r.ty()?;
        }
        match t {
            ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
            _ => None,
        }
    };
    let impls: Vec<(ast::Impl, String, String)> = file
        .syntax()
        .descendants()
        .filter_map(ast::Impl::cast)
        .filter_map(|imp| {
            let tr = imp.trait_()?.syntax().text().to_string();
            let s = self_name(&imp)?;
            Some((imp, tr, s))
        })
        .collect();
    impls
        .iter()
        .filter(|(_, tr, s)| {
            let borrowed = match s.as_str() {
                "PathBuf" => "Path",
                "OsString" => "OsStr",
                _ => return false,
            };
            impls.iter().any(|(_, t2, s2)| t2 == tr && s2 == borrowed)
        })
        .map(|(imp, _, _)| {
            let r = imp.syntax().text_range();
            (r.start().into(), r.end().into())
        })
        .collect()
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
        // (`NonZero<char>` too, a `char`.)
        "NonZero" if txt(i + 1) == "<" && txt(i + 3) == ">" => {
            let int = txt(i + 2).to_string();
            if int != "char" {
                crate::ty::IntKind::from_name(&int)?;
            }
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
            if int != "char" {
                crate::ty::IntKind::from_name(&int)?;
            }
            // (`NonZero::<u8>::new(x)`: `u8::__nz_new(x)`.)
            // (As a value, `.find_map(NonZero::<char>::new)`: a closure.)
            if let f @ ("new" | "new_unchecked") = txt(i + 6) {
                let to = if txt(i + 7) == "(" {
                    format!("{int}::__nz_{f}")
                } else {
                    format!("(|__nz: {int}| {int}::__nz_{f}(__nz))")
                };
                edits.push((a, toks[i + 6].end, to));
                return Some(i + 7);
            }
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
        // `Ref<'_, T>` / `RefMut<'a, T>` (and the lock prelude's `MutexGuard<'_, T>`, ..): what
        // `borrow()` / `borrow_mut()` (`lock().unwrap()`) give, the `T`.
        "Ref" | "RefMut" | "MutexGuard" | "RwLockReadGuard" | "RwLockWriteGuard" => {
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
            ast::String::cast(t.clone()).and_then(|s| crate::lineindex::string_value(&s));
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

/// `impl Into<Y> for X { fn into(self) -> Y { .. } }` as `impl From<X> for Y { fn from(self__: X)
/// -> Y { .. } }`, `self` in the body as `self__` and `Self` as `X`: std's blanket `Into` from
/// `From` then gives `x.into()` too. Not for a trait of the file's own named `Into`.
fn into_impls(file: &SourceFile, text: &str) -> Vec<(usize, usize, String)> {
    use ast::{HasGenericArgs, HasName};
    let mut out = Vec::new();
    if declares(text, "Into") {
        return out;
    }
    let range = |n: &ra_ap_syntax::SyntaxNode| {
        let r = n.text_range();
        (usize::from(r.start()), usize::from(r.end()))
    };
    for imp in file.syntax().descendants().filter_map(ast::Impl::cast) {
        let (Some(ast::Type::PathType(tr)), Some(x)) = (imp.trait_(), imp.self_ty()) else {
            continue;
        };
        let Some(path) = tr.path() else { continue };
        let Some(seg) = path.segment() else { continue };
        let qual = path.qualifier().map(|q| q.syntax().text().to_string());
        if seg.name_ref().is_none_or(|n| n.text() != "Into")
            || qual.is_some_and(|q| !matches!(q.as_str(), "std::convert" | "core::convert" | "convert"))
        {
            continue;
        }
        let Some(y) = seg
            .generic_arg_list()
            .and_then(|l| l.generic_args().next())
            .and_then(|g| match g {
                ast::GenericArg::TypeArg(t) => t.ty(),
                _ => None,
            })
        else {
            continue;
        };
        let Some(items) = imp.assoc_item_list() else { continue };
        let fns: Vec<ast::Fn> = items
            .assoc_items()
            .filter_map(|i| match i {
                ast::AssocItem::Fn(f) => Some(f),
                _ => None,
            })
            .collect();
        let [f] = fns.as_slice() else { continue };
        let (Some(name), Some(sp)) = (f.name(), f.param_list().and_then(|p| p.self_param()))
        else {
            continue;
        };
        if name.text() != "into" || sp.amp_token().is_some() || sp.ty().is_some() {
            continue;
        }
        let (x, y) = (x.syntax().text().to_string(), y.syntax().text().to_string());
        let (a, b) = range(tr.syntax());
        out.push((a, b, format!("From<{x}>")));
        let (a, b) = range(imp.self_ty().unwrap().syntax());
        out.push((a, b, y));
        let (a, b) = range(name.syntax());
        out.push((a, b, "from".into()));
        let (a, b) = range(sp.syntax());
        let m = if sp.mut_token().is_some() { "mut " } else { "" };
        out.push((a, b, format!("{m}self__: {x}")));
        for t in f
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !sp.syntax().text_range().contains_range(t.text_range()))
        {
            let r = t.text_range();
            let (a, b) = (usize::from(r.start()), usize::from(r.end()));
            match (t.kind(), t.text()) {
                (SyntaxKind::SELF_KW, _) | (SyntaxKind::IDENT, "self") => {
                    out.push((a, b, "self__".into()))
                }
                (SyntaxKind::SELF_TYPE_KW, _) | (SyntaxKind::IDENT, "Self") => {
                    out.push((a, b, x.clone()))
                }
                _ => {}
            }
        }
    }
    out
}

/// `fn put(w: &mut dyn fmt::Write)` (after the rename, `dyn FmtWrite`; also `io::Write` and
/// `impl ..`) as `fn put<__W0: FmtWrite>(w: &mut __W0)`: the writer is passed as a generic
/// one is (a `String` or a `Formatter` through its cell, your own writer, a `Vec<u8>`).
fn writer_params(file: &SourceFile) -> Vec<(usize, usize, String)> {
    use ast::{HasGenericParams, HasName};
    let mut out = Vec::new();
    for f in file.syntax().descendants().filter_map(ast::Fn::cast) {
        let Some(pl) = f.param_list() else { continue };
        let mut added: Vec<String> = Vec::new();
        for p in pl.params() {
            let Some(ast::Type::RefType(r)) = p.ty() else {
                continue;
            };
            let Some(mut inner) = r.ty().filter(|_| r.mut_token().is_some()) else {
                continue;
            };
            if let ast::Type::ParenType(pt) = &inner
                && let Some(t) = pt.ty()
            {
                inner = t;
            }
            let bounds = match &inner {
                ast::Type::DynTraitType(d) => d.type_bound_list(),
                ast::Type::ImplTraitType(i) => i.type_bound_list(),
                _ => None,
            };
            let Some(tr) = bounds.into_iter().flat_map(|l| l.bounds()).find_map(|b| {
                let n = match b.ty()? {
                    ast::Type::PathType(pt) => pt.path()?.segment()?.name_ref()?.text().to_string(),
                    _ => return None,
                };
                matches!(n.as_str(), "FmtWrite" | "IoWrite").then_some(n)
            }) else {
                continue;
            };
            let name = format!("__W{}", added.len());
            let range = r
                .ty()
                .map(|t| t.syntax().text_range())
                .unwrap_or(inner.syntax().text_range());
            out.push((
                usize::from(range.start()),
                usize::from(range.end()),
                name.clone(),
            ));
            added.push(format!("{name}: {tr}"));
        }
        if added.is_empty() {
            continue;
        }
        let list = added.join(", ");
        match f.generic_param_list() {
            Some(gl) => {
                let Some(close) = gl.r_angle_token() else {
                    continue;
                };
                let at = usize::from(close.text_range().start());
                let sep = if gl.generic_params().next().is_some() {
                    ", "
                } else {
                    ""
                };
                out.push((at, at, format!("{sep}{list}")));
            }
            None => {
                let Some(n) = f.name() else { continue };
                let at = usize::from(n.syntax().text_range().end());
                out.push((at, at, format!("<{list}>")));
            }
        }
    }
    out
}

/// `w.write_fmt(args)` as `write!(w, "{}", args)` (a `String`, a `Formatter` or an `io::Write`).
fn write_fmt_calls(file: &SourceFile) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    // (Not with a trait of the file's own that declares `write_fmt`: its calls are its.)
    if file
        .syntax()
        .descendants()
        .filter_map(ast::Trait::cast)
        .flat_map(|t| t.assoc_item_list())
        .flat_map(|l| l.assoc_items())
        .any(|i| matches!(i, ast::AssocItem::Fn(f) if f.name().is_some_and(|n| n.text() == "write_fmt")))
    {
        return out;
    }
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

/// `s.push(x)` on an `OsString` (`x` a string or a path) is `s.__os_push(x)` (the path prelude's):
/// `OsString` is a `String` here, whose `push` takes a `char`. The bindings are the ones written
/// `OsString` or made by `OsString::..` / `to_os_string()` / `into_os_string()` / an `OsStr`'s
/// `to_owned()`, the struct fields declared `OsString`, by name, and the elements of a
/// `Vec<OsString>`.
fn os_string_pushes(file: &SourceFile, imports: &[String]) -> Vec<(usize, usize, String)> {
    let os = |t: &str| {
        (imports.iter().any(|i| i == "OsString") && crate::stdtime::has_word(t, "OsString"))
            || t.contains("ffi::OsString")
    };
    let mut names = std::collections::HashSet::new();
    for n in file.syntax().descendants() {
        let (pat, ty, init) = if let Some(l) = ast::LetStmt::cast(n.clone()) {
            (l.pat(), l.ty(), l.initializer())
        } else if let Some(p) = ast::Param::cast(n) {
            (p.pat(), p.ty(), None)
        } else {
            continue;
        };
        let Some(ast::Pat::IdentPat(ip)) = pat else { continue };
        let init = init.map(|e| e.syntax().text().to_string()).unwrap_or_default();
        if ty.is_some_and(|t| os(&t.syntax().text().to_string()))
            || (os(&init) && init.contains("OsString::"))
            || init.ends_with(".to_os_string()")
            || init.ends_with(".into_os_string()")
            // (`p.as_os_str().to_owned()`, `OsStr::new(..).to_owned()`: an `OsString`.)
            || (init.ends_with(".to_owned()")
                && (init.contains(".as_os_str()") || init.contains("OsStr::")))
        {
            if let Some(n) = ip.name() {
                names.insert(n.text().to_string());
            }
        }
    }
    // Struct fields declared `OsString` (`p.s.push(..)`, `self.s.push(..)`), by name: only names
    // no other struct field in the file declares with another type.
    let mut fields = std::collections::HashSet::new();
    let mut other_fields = std::collections::HashSet::new();
    for f in file.syntax().descendants().filter_map(ast::RecordField::cast) {
        let Some(n) = f.name() else { continue };
        if f.ty().is_some_and(|t| os(&t.syntax().text().to_string())) {
            fields.insert(n.text().to_string());
        } else {
            other_fields.insert(n.text().to_string());
        }
    }
    fields.retain(|n| !other_fields.contains(n));
    // `Vec<OsString>` / `[OsString]` bindings and fields: their elements are `OsString`s too
    // (`v[0].push(..)`, `v.last_mut().unwrap().push(..)`, and the bindings of `for s in
    // v.iter_mut()` / `&mut v` and `if let Some(s) = v.last_mut()`).
    let elems = |t: &str| {
        let t: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        os(&t) && (t.contains("Vec<") || t.contains('['))
    };
    let mut vecs = std::collections::HashSet::new();
    for n in file.syntax().descendants() {
        let (pat, ty) = if let Some(l) = ast::LetStmt::cast(n.clone()) {
            (l.pat(), l.ty())
        } else if let Some(p) = ast::Param::cast(n) {
            (p.pat(), p.ty())
        } else {
            continue;
        };
        if let Some(ast::Pat::IdentPat(ip)) = pat
            && ty.is_some_and(|t| elems(&t.syntax().text().to_string()))
            && let Some(n) = ip.name()
        {
            vecs.insert(n.text().to_string());
        }
    }
    let mut vec_fields = std::collections::HashSet::new();
    let mut other_vec_fields = std::collections::HashSet::new();
    for f in file.syntax().descendants().filter_map(ast::RecordField::cast) {
        let Some(n) = f.name() else { continue };
        if f.ty().is_some_and(|t| elems(&t.syntax().text().to_string())) {
            vec_fields.insert(n.text().to_string());
        } else {
            other_vec_fields.insert(n.text().to_string());
        }
    }
    vec_fields.retain(|n| !other_vec_fields.contains(n));
    let named = |e: Option<ast::Expr>| match e {
        Some(ast::Expr::PathExpr(p)) => p
            .path()
            .and_then(|p| p.as_single_name_ref())
            .is_some_and(|n| vecs.contains(&n.text().to_string())),
        Some(ast::Expr::FieldExpr(f)) => f
            .name_ref()
            .is_some_and(|n| vec_fields.contains(&n.text().to_string())),
        _ => false,
    };
    // (Also `&mut v`.)
    let is_vec = |e: Option<ast::Expr>| match e {
        Some(ast::Expr::RefExpr(r)) if r.mut_token().is_some() => named(r.expr()),
        e => named(e),
    };
    // `v.last_mut()` / `first_mut()` / `get_mut(i)` / `iter_mut()` on such a `Vec`.
    let elem_call = |e: Option<ast::Expr>, ms: &[&str]| match e {
        Some(ast::Expr::MethodCallExpr(m)) => {
            m.name_ref().is_some_and(|n| ms.contains(&n.text())) && is_vec(m.receiver())
        }
        _ => false,
    };
    const ELEM: [&str; 3] = ["last_mut", "first_mut", "get_mut"];
    for n in file.syntax().descendants() {
        let pat = if let Some(f) = ast::ForExpr::cast(n.clone()) {
            let it = f.iterable();
            if !(elem_call(it.clone(), &["iter_mut"]) || is_vec(it)) {
                continue;
            }
            f.pat()
        } else if let Some(l) = ast::LetExpr::cast(n) {
            if !elem_call(l.expr(), &ELEM) {
                continue;
            }
            match l.pat() {
                Some(ast::Pat::TupleStructPat(t))
                    if t.path().is_some_and(|p| p.syntax().text() == "Some") =>
                {
                    t.fields().next()
                }
                _ => None,
            }
        } else {
            continue;
        };
        if let Some(ast::Pat::IdentPat(ip)) = pat
            && let Some(n) = ip.name()
        {
            names.insert(n.text().to_string());
        }
    }
    let mut out = Vec::new();
    for m in file
        .syntax()
        .descendants()
        .filter_map(ast::MethodCallExpr::cast)
    {
        let Some(name) = m.name_ref() else { continue };
        let os_recv = match m.receiver() {
            Some(ast::Expr::PathExpr(p)) => p
                .path()
                .and_then(|p| p.as_single_name_ref())
                .is_some_and(|n| names.contains(&n.text().to_string())),
            Some(ast::Expr::FieldExpr(f)) => f
                .name_ref()
                .is_some_and(|n| fields.contains(&n.text().to_string())),
            Some(ast::Expr::IndexExpr(i)) => is_vec(i.base()),
            Some(ast::Expr::MethodCallExpr(u)) => {
                u.name_ref()
                    .is_some_and(|n| matches!(n.text(), "unwrap" | "expect"))
                    && elem_call(u.receiver(), &ELEM)
            }
            _ => false,
        };
        if name.text() == "push" && os_recv {
            let r = name.syntax().text_range();
            out.push((usize::from(r.start()), usize::from(r.end()), "__os_push".into()));
        }
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

/// The text `concat!` makes of its arguments when they're all literals (strings, chars, unsuffixed
/// numbers, bools) or `file!()` / `line!()` / `column!()`, separated by commas.
fn concat_literals(
    args: &[Tok],
    map: &OffsetMap,
    site: Option<(&str, &str)>,
) -> Option<String> {
    let mut out = String::new();
    let mut k = 0;
    while k < args.len() {
        // (An argument ends at the next `,`; a macro's `$x:expr` comes wrapped in parens.)
        let mut arg = &args[k..];
        if let Some(n) = arg.iter().position(|t| t.text == ",") {
            arg = &arg[..n];
        }
        k += arg.len() + 1;
        while arg.len() > 2 && arg[0].text == "(" && arg[arg.len() - 1].text == ")" {
            arg = &arg[1..arg.len() - 1];
        }
        if arg.len() == 4
            && let Some((v, _)) = location_value(arg, 0, map, site)
        {
            out.push_str(&v);
            continue;
        }
        // (`stringify!(name)`: its tokens' text, one space where the source has some.)
        if arg.len() >= 4
            && arg[0].text == "stringify"
            && arg[1].text == "!"
            && arg[2].text == "("
            && arg[arg.len() - 1].text == ")"
        {
            let inner = &arg[3..arg.len() - 1];
            for (j, t) in inner.iter().enumerate() {
                if j > 0 && t.start > inner[j - 1].end {
                    out.push(' ');
                }
                out.push_str(&t.text);
            }
            continue;
        }
        let [t] = arg else { return None };
        match t.kind {
            SyntaxKind::STRING => out.push_str(t.string.as_deref()?),
            SyntaxKind::CHAR => out.push(char_value(&t.text)?),
            SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER
                if t.text.chars().all(|c| c.is_ascii_digit() || c == '.') =>
            {
                out.push_str(&t.text)
            }
            SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW => out.push_str(&t.text),
            _ => return None,
        }
    }
    Some(out)
}

/// `file!()` / `line!()` / `column!()` at `i`, as a literal: the file's name, or the 1-based line or
/// column (in chars) of the call's place in `src`.
fn location_macro(
    toks: &[Tok],
    i: usize,
    map: &OffsetMap,
    site: Option<(&str, &str)>,
) -> Option<String> {
    let (v, is_file) = location_value(toks, i, map, site)?;
    Some(if is_file { format!("{v:?}") } else { format!("{v}u32") })
}

/// The value of `location_macro`, and whether it is `file!()`'s.
fn location_value(
    toks: &[Tok],
    i: usize,
    map: &OffsetMap,
    site: Option<(&str, &str)>,
) -> Option<(String, bool)> {
    let (src, filename) = site?;
    let txt = txt_at(toks, i);
    if txt(1) != "!" || txt(2) != "(" || txt(3) != ")" {
        return None;
    }
    let at = map.orig(toks[i].start, false).min(src.len());
    let line_start = src[..at].rfind('\n').map_or(0, |n| n + 1);
    Some(match txt(0) {
        "file" => (filename.to_string(), true),
        "line" => ((src[..at].matches('\n').count() + 1).to_string(), false),
        "column" => ((src[line_start..at].chars().count() + 1).to_string(), false),
        _ => return None,
    })
}

/// A char literal's value: one character or a simple escape.
fn char_value(lit: &str) -> Option<char> {
    let inner = lit.strip_prefix('\'')?.strip_suffix('\'')?;
    let mut cs = inner.chars();
    match (cs.next()?, cs.next(), cs.next()) {
        (c, None, _) if c != '\\' => Some(c),
        ('\\', Some(e), None) => Some(match e {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            '0' => '\0',
            '\\' | '\'' | '"' => e,
            _ => return None,
        }),
        _ => None,
    }
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
                "path" | "ffi" | "fmt" | "cell" | "num" | "convert" | "io" | "panic"
            )
            && q + 2 == i);
    (q, std)
}
