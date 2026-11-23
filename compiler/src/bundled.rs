//! The bundled crates that are source rewrites: `#[derive(thiserror::Error)]` (a `Display`, an
//! `Error` and the `From` impls), `bitflags!` (a struct of bits with consts, set operations and
//! bitflags' `{:?}`), `cfg_if!` (the first branch whose predicate holds), `lazy_static!`
//! (`LazyLock` statics), serde_json's `json!` (the `Value` it makes) and rayon (run
//! sequentially: `par_iter()` is `iter()`), smallvec (`SmallVec<[T; N]>` is a `Vec<T>`). A pre-pass over
//! the text, like the proc macros': each rewrite keeps the `OffsetMap` in step, so positions
//! still point into the original.

use ra_ap_syntax::{
    AstNode, SourceFile, TextRange,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasName},
};

use crate::macros::EDITION;
use crate::mbe::OffsetMap;

/// Whether `text` uses any of these crates (a cheap test before parsing).
pub fn uses_bundled(text: &str) -> bool {
    text.contains("thiserror")
        || text.contains("bitflags!")
        || text.contains("cfg_if!")
        || text.contains("lazy_static!")
        || text.contains("thread_local!")
        || text.contains("rayon")
        || text.contains("serde_json")
        || text.contains("regex")
        || text.contains("clap")
        || text.contains("jsx!")
        || text.contains("SmallVec")
        || text.contains("smallvec!")
        || text.contains("indexmap!")
        || text.contains("indexset!")
        || uses_renamed(text)
}

/// An error in the original text: (start, end, message).
pub type Error = (usize, usize, String);

/// Rewrite every use in `text`; returns whether anything changed.
pub fn expand(text: &mut String, map: &mut OffsetMap) -> Result<bool, Vec<Error>> {
    let file = crate::macros::parse_tree(text);
    let thiserror = text.contains("thiserror");
    let rayon_used = text.contains("rayon");
    let json_used = text.contains("serde_json");
    let tokio_used = text.contains("tokio::");
    // (Not where `regex` is the file's own module.)
    let regex_used =
        (text.contains("regex::") || text.contains("use regex")) && !text.contains("mod regex");
    let json_map_imported = json_used
        && crate::typeck::crates::crate_imports(text)
            .get("Map")
            .is_some_and(|(k, i)| k == "serde_json" && i == "Map");
    let mut edits: Vec<(TextRange, String)> = Vec::new();
    let mut select_arities: std::collections::BTreeSet<usize> = Default::default();
    let mut errors: Vec<(TextRange, String)> = Vec::new();
    rename_crate_items(&file, text, &mut edits);
    if text.contains("petgraph") && !text.contains("mod petgraph") {
        petgraph_edits(&file, &mut edits);
    }
    if text.contains("SmallVec") || text.contains("smallvec!") {
        smallvec_edits(&file, &mut edits);
    }
    if text.contains("indexmap!") || text.contains("indexset!") {
        indexmap_macros(&file, &mut edits);
    }
    if text.contains("chrono_tz") {
        chrono_tz_zones(&file, &mut edits);
    }
    // `#[derive(Parser)]` and the other clap derives.
    let clap_used = text.contains("clap");
    let clap_ctx = crate::clap_derive::Ctx {
        value_enums: if clap_used {
            file.syntax()
                .descendants()
                .filter_map(ast::Adt::cast)
                .filter(|a| {
                    a.attrs().any(|at| {
                        attr_name(&at).as_deref() == Some("derive")
                            && attr_args(&at).is_some_and(|x| x.contains("ValueEnum"))
                    })
                })
                .filter_map(|a| a.name().map(|n| n.text().to_string()))
                .collect()
        } else {
            Vec::new()
        },
    };
    for node in file.syntax().descendants() {
        if clap_used && let Some(adt) = ast::Adt::cast(node.clone()) {
            crate::clap_derive::derive(&adt, &clap_ctx, &mut edits);
        }
        // `clap::value_parser!(u32)`.
        if clap_used
            && let Some(mc) = ast::MacroCall::cast(node.clone())
            && mc
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_some_and(|n| n.text() == "value_parser")
            && let Some(tt) = mc.token_tree()
        {
            let t = tt.syntax().text().to_string();
            let inner = t
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim()
                .to_string();
            edits.push((
                mc.syntax().text_range(),
                format!("ClapValueParser::__of({inner:?})"),
            ));
            continue;
        }
        if thiserror && let Some(adt) = ast::Adt::cast(node.clone()) {
            derive_error(&adt, &mut edits, &mut errors);
            continue;
        }
        // `jsx!` (the bundled `react`): its markup as calls of the react prelude.
        if let Some(mc) = ast::MacroCall::cast(node.clone())
            && crate::jsx::is_jsx_call(&mc)
            && let Some(tt) = mc.token_tree()
            && !mc
                .syntax()
                .ancestors()
                .skip(1)
                .any(|a| ast::MacroCall::cast(a).is_some_and(|m| crate::jsx::is_jsx_call(&m)))
        {
            match crate::jsx::rewrite(&tt, text) {
                // (Around the embedded expressions, which stay where they are written.)
                Ok(code) => edits.extend(crate::jsx::edits(
                    mc.syntax().text_range(),
                    &format!("({code})"),
                    text,
                )),
                // (For the language service: the expressions in it, still analyzed.)
                Err(_) if crate::jsx::is_lenient() => {
                    edits.extend(crate::jsx::edits(
                        mc.syntax().text_range(),
                        &crate::jsx::lenient(&tt, text),
                        text,
                    ));
                }
                Err((r, msg)) => errors.push((r, msg)),
            }
            continue;
        }
        if json_used {
            if let Some(mc) = ast::MacroCall::cast(node.clone())
                && is_json_call(&mc)
                && let Some(tt) = mc.token_tree()
            {
                edits.push((mc.syntax().text_range(), json_of(&tt_items(&tt), text)));
                continue;
            }
            // (Calls in other macros' tokens; a `json!`'s own are its.)
            if let Some(tt) = ast::TokenTree::cast(node.clone())
                && let Some(parent) = tt.syntax().parent()
                && !ast::TokenTree::can_cast(parent.kind())
                && !ast::MacroCall::cast(parent).is_some_and(|mc| is_json_call(&mc))
            {
                json_in_tokens(&tt, text, &mut edits);
            }
        }
        // `serde_json::Map` (and `Map` imported from it) is a `BTreeMap`.
        if json_used
            && let Some(path) = ast::Path::cast(node.clone())
            && path.qualifier().is_none()
            && !path
                .syntax()
                .ancestors()
                .any(|a| ast::Use::can_cast(a.kind()))
            && let Some(parent) = path.parent_path()
            && path.syntax().text() == "serde_json"
            && parent
                .segment()
                .and_then(|s| s.name_ref())
                .is_some_and(|n| n.text() == "Map")
        {
            let r = path.syntax().text_range().cover(
                parent
                    .segment()
                    .and_then(|s| s.name_ref().map(|n| n.syntax().text_range()))
                    .unwrap_or(path.syntax().text_range()),
            );
            edits.push((r, "std::collections::BTreeMap".into()));
        } else if json_map_imported
            && let Some(path) = ast::Path::cast(node.clone())
            && path.qualifier().is_none()
            && !path
                .syntax()
                .ancestors()
                .any(|a| ast::Use::can_cast(a.kind()))
            && path
                .segment()
                .and_then(|s| s.name_ref())
                .is_some_and(|n| n.text() == "Map")
        {
            let r = path
                .segment()
                .and_then(|s| s.name_ref())
                .map(|n| n.syntax().text_range());
            if let Some(r) = r {
                edits.push((r, "std::collections::BTreeMap".into()));
            }
        }
        if tokio_used {
            tokio_rewrite(&node, &mut edits);
        }
        if regex_used {
            regex(&node, &mut edits);
            if let Some(tt) = ast::TokenTree::cast(node.clone())
                && !tt
                    .syntax()
                    .parent()
                    .is_some_and(|p| ast::TokenTree::can_cast(p.kind()))
            {
                regex_tokens(&tt, &mut edits);
            }
        }
        if rayon_used {
            rayon(&node, &mut edits);
            if let Some(tt) = ast::TokenTree::cast(node.clone())
                && !tt
                    .syntax()
                    .parent()
                    .is_some_and(|p| ast::TokenTree::can_cast(p.kind()))
            {
                rayon_tokens(&tt, &mut edits);
            }
        }
        let Some(mc) = ast::MacroCall::cast(node) else {
            continue;
        };
        let Some(name) = mc
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string())
        else {
            continue;
        };
        let select = name == "select"
            && tokio_used
            && mc
                .path()
                .is_some_and(|p| matches!(p.syntax().text().to_string().as_str(), "tokio::select" | "select"));
        if !select && !matches!(name.as_str(), "bitflags" | "cfg_if" | "lazy_static" | "thread_local") {
            continue;
        }
        let Some(tt) = mc.token_tree() else { continue };
        let tt_text = tt.syntax().text().to_string();
        // (Without its delimiters; an unfinished one has no closing one.)
        let Some(body) = tt_text
            .strip_prefix(['{', '(', '['])
            .and_then(|b| b.strip_suffix(['}', ')', ']']))
        else {
            continue;
        };
        // (The edits already made inside, `std::io::stdout()` renamed, go into the rewrite: as
        // edits of their own they'd overlap it.)
        let body_start = usize::from(tt.syntax().text_range().start()) + 1;
        let inside = |r: &TextRange| {
            usize::from(r.start()) >= body_start && usize::from(r.end()) <= body_start + body.len()
        };
        let mut inner: Vec<(TextRange, String)> =
            edits.iter().filter(|(r, _)| inside(r)).cloned().collect();
        inner.sort_by_key(|(r, _)| std::cmp::Reverse((r.start(), r.end())));
        let mut edited = body.to_string();
        let mut last = usize::MAX;
        for (r, new) in &inner {
            let (a, b) = (
                usize::from(r.start()) - body_start,
                usize::from(r.end()) - body_start,
            );
            if b <= last {
                edited.replace_range(a..b, new);
                last = a;
            }
        }
        let out = match name.as_str() {
            "bitflags" => bitflags(&edited),
            "cfg_if" => cfg_if(&edited),
            "lazy_static" => lazy_static(&edited),
            "thread_local" => thread_local(&edited),
            "select" => tokio_select(&edited, &mut select_arities),
            _ => None,
        };
        if out.is_some() {
            edits.retain(|(r, _)| !inside(r));
        }
        if let Some(out) = out {
            // (An item macro's trailing `;` goes too.)
            let mut r = mc.syntax().text_range();
            if let Some(semi) = mc.semicolon_token() {
                r = r.cover(semi.text_range());
            }
            edits.push((r, out));
        }
    }
    // (`select!`'s branch enums, one per number of branches.)
    if !select_arities.is_empty() {
        let end = TextRange::empty(ra_ap_syntax::TextSize::from(text.len() as u32));
        let enums: String = select_arities.iter().map(|&n| select_enum(n)).collect();
        edits.push((end, enums));
    }
    if !errors.is_empty() {
        return Err(errors
            .into_iter()
            .map(|(r, m)| {
                let (a, b) = (usize::from(r.start()), usize::from(r.end()));
                (map.orig(a, false), map.orig(b, true), m)
            })
            .collect());
    }
    if edits.is_empty() {
        return Ok(false);
    }
    edits.sort_by_key(|(r, _)| std::cmp::Reverse((r.start(), r.end())));
    let mut last_start = usize::MAX;
    for (r, new) in edits {
        let (a, b) = (usize::from(r.start()), usize::from(r.end()));
        if b > last_start {
            continue;
        }
        text.replace_range(a..b, &new);
        map.replace(a, b, new.len());
        last_start = a;
    }
    Ok(true)
}

// ---------------------------------------------------------------- thiserror

/// One field: its binding in a pattern (`_0` or its name), its type, and whether it's the
/// error's source (`#[from]`, `#[source]`, or named `source`) and has `#[from]`.
struct Field {
    bind: String,
    ty: String,
    source: bool,
    from: bool,
}

/// A variant (or the struct): its path and pattern, the `#[error(..)]` arguments, its fields
/// (named or not, or none).
struct Case {
    path: String,
    pattern: String,
    error: Option<(String, TextRange)>,
    fields: Vec<Field>,
    named: Option<bool>,
}

pub(crate) fn attr_name(a: &ast::Attr) -> Option<String> {
    Some(a.path()?.syntax().text().to_string())
}

/// What's inside an attribute's parentheses (`#[error("x {0}")]` → `"x {0}"`).
pub(crate) fn attr_args(a: &ast::Attr) -> Option<String> {
    let t = crate::adt::attr_text(a);
    let open = t.find('(')?;
    let close = t.rfind(')')?;
    (close > open).then(|| t[open + 1..close].to_string())
}

/// `#[error("..", args)]`'s inside (and where it is), and the helper attributes to drop (their
/// ranges).
fn helper_attrs(
    node: &impl HasAttrs,
    edits: &mut Vec<(TextRange, String)>,
) -> Option<(String, TextRange)> {
    let mut error = None;
    for a in node.attrs() {
        match attr_name(&a).as_deref() {
            Some("error") => {
                error = Some((attr_args(&a)?, a.syntax().text_range()));
                edits.push((a.syntax().text_range(), String::new()));
            }
            Some("from" | "source" | "backtrace") => {
                edits.push((a.syntax().text_range(), String::new()));
            }
            _ => {}
        }
    }
    error
}

fn fields_of(
    list: Option<ast::FieldList>,
    edits: &mut Vec<(TextRange, String)>,
) -> (Vec<Field>, Option<bool>) {
    let has =
        |n: &dyn HasAttrs, name: &str| n.attrs().any(|a| attr_name(&a).as_deref() == Some(name));
    let mut out = Vec::new();
    let named = match list {
        Some(ast::FieldList::RecordFieldList(l)) => {
            for f in l.fields() {
                let name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
                let from = has(&f, "from");
                out.push(Field {
                    source: from || has(&f, "source") || name == "source",
                    bind: name,
                    ty: f
                        .ty()
                        .map(|t| t.syntax().text().to_string())
                        .unwrap_or_default(),
                    from,
                });
                helper_attrs(&f, edits);
            }
            Some(true)
        }
        Some(ast::FieldList::TupleFieldList(l)) => {
            for (i, f) in l.fields().enumerate() {
                let from = has(&f, "from");
                out.push(Field {
                    source: from || has(&f, "source"),
                    bind: format!("_{i}"),
                    ty: f
                        .ty()
                        .map(|t| t.syntax().text().to_string())
                        .unwrap_or_default(),
                    from,
                });
                helper_attrs(&f, edits);
            }
            Some(false)
        }
        None => None,
    };
    (out, named)
}

fn pattern(path: &str, fields: &[Field], named: Option<bool>) -> String {
    let binds: Vec<&str> = fields.iter().map(|f| f.bind.as_str()).collect();
    match named {
        Some(true) => format!("{path} {{ {} }}", binds.join(", ")),
        Some(false) => format!("{path}({})", binds.join(", ")),
        None => path.to_string(),
    }
}

/// `#[error("{0} at {pos:?}", .1.len())]` as `write!`'s arguments: positional fields become
/// their bindings (`{_0}`), and `.0` / `.field` in the extra arguments too.
fn error_args(spec: &str) -> String {
    let spec = spec.trim();
    let end = string_end(spec).unwrap_or(spec.len());
    let (lit, rest) = spec.split_at(end);
    let mut lit_out = String::new();
    let chars: Vec<char> = lit.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        lit_out.push(c);
        if c == '{' {
            if chars.get(i + 1) == Some(&'{') {
                lit_out.push('{');
                i += 2;
                continue;
            }
            if chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                lit_out.push('_');
            }
        }
        i += 1;
    }
    let mut rest_out = String::new();
    let rc: Vec<char> = rest.chars().collect();
    for (j, &c) in rc.iter().enumerate() {
        if c == '.' {
            let prev = rc[..j].iter().rev().find(|c| !c.is_whitespace());
            let shorthand = match prev {
                None => true,
                Some(p) => matches!(
                    p,
                    ',' | '('
                        | '['
                        | '{'
                        | '&'
                        | '!'
                        | '*'
                        | '+'
                        | '-'
                        | '/'
                        | '='
                        | '<'
                        | '>'
                        | '|'
                ),
            };
            if shorthand {
                if rc.get(j + 1).is_some_and(|d| d.is_ascii_digit()) {
                    rest_out.push('_');
                }
                continue;
            }
        }
        rest_out.push(c);
    }
    format!("{lit_out}{rest_out}")
}

/// The names a message's literal refers to inline (`{key}`, `{key:?}`; not `{0}` or `{}`).
fn inline_names(spec: &str) -> Vec<String> {
    let spec = spec.trim();
    let lit = &spec[..string_end(spec).unwrap_or(spec.len())];
    let mut out = Vec::new();
    let chars: Vec<char> = lit.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if chars.get(i + 1) == Some(&'{') {
                i += 2;
                continue;
            }
            let name: String = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || **c == '_')
                .collect();
            // (`{r#type}` and `{self}`-like names are left to the compile.)
            let keyword = matches!(
                name.as_str(),
                "self" | "Self" | "r" | "fn" | "type" | "struct" | "enum" | "match" | "loop"
            );
            if !name.is_empty() && !keyword && !name.starts_with(|c: char| c.is_ascii_digit()) {
                out.push(name);
            }
        }
        i += 1;
    }
    out
}

/// The end of the string literal `s` starts with (`"..."` or `r#"..."#`).
fn string_end(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if let Some(raw) = s.strip_prefix('r') {
        let hashes = raw.bytes().take_while(|&c| c == b'#').count();
        let close = format!("\"{}", "#".repeat(hashes));
        let start = 2 + hashes;
        return s
            .get(start..)?
            .find(&close)
            .map(|k| start + k + close.len());
    }
    if b.first() != Some(&b'"') {
        return None;
    }
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn derive_error(
    adt: &ast::Adt,
    edits: &mut Vec<(TextRange, String)>,
    errors: &mut Vec<(TextRange, String)>,
) {
    // `#[derive(Error)]` / `#[derive(thiserror::Error)]`, dropped from the derive list.
    let mut found = false;
    for a in adt.attrs() {
        if attr_name(&a).as_deref() != Some("derive") {
            continue;
        }
        let Some(inner) = attr_args(&a) else {
            continue;
        };
        let names: Vec<&str> = inner
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let kept: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| !matches!(*n, "Error" | "thiserror::Error"))
            .collect();
        if kept.len() == names.len() {
            continue;
        }
        found = true;
        let new = if kept.is_empty() {
            String::new()
        } else {
            format!("#[derive({})]", kept.join(", "))
        };
        edits.push((a.syntax().text_range(), new));
    }
    if !found {
        return;
    }
    let Some(name) = adt.name().map(|n| n.text().to_string()) else {
        return;
    };
    let (gp, args) = match adt.generic_param_list() {
        Some(l) => {
            let names: Vec<String> = l
                .generic_params()
                .filter_map(|p| match p {
                    ast::GenericParam::TypeParam(t) => t.name().map(|n| n.text().to_string()),
                    ast::GenericParam::LifetimeParam(l) => {
                        l.lifetime().map(|l| l.text().to_string())
                    }
                    ast::GenericParam::ConstParam(c) => c.name().map(|n| n.text().to_string()),
                })
                .collect();
            (
                l.syntax().text().to_string(),
                format!("<{}>", names.join(", ")),
            )
        }
        None => (String::new(), String::new()),
    };
    let top = helper_attrs(adt, edits);
    let mut cases: Vec<Case> = Vec::new();
    match adt {
        ast::Adt::Enum(e) => {
            for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
                let vname = v.name().map(|n| n.text().to_string()).unwrap_or_default();
                let error = helper_attrs(&v, edits).or_else(|| top.clone());
                let (fields, named) = fields_of(v.field_list(), edits);
                let path = format!("{name}::{vname}");
                cases.push(Case {
                    pattern: pattern(&path, &fields, named),
                    path,
                    error,
                    fields,
                    named,
                });
            }
        }
        ast::Adt::Struct(s) => {
            let (fields, named) = fields_of(s.field_list(), edits);
            cases.push(Case {
                pattern: pattern(&name, &fields, named),
                path: name.clone(),
                error: top,
                fields,
                named,
            });
        }
        ast::Adt::Union(_) => return,
    }
    // A `{name}` in a message must be a field (or a named argument after it).
    for c in &cases {
        let Some((spec, at)) = &c.error else { continue };
        if spec.trim() == "transparent" {
            continue;
        }
        for name in inline_names(spec) {
            let known = c.fields.iter().any(|f| f.bind == name)
                || spec.contains(&format!("{name} ="))
                || spec.contains(&format!("{name}="));
            if !known {
                errors.push((
                    *at,
                    format!("there is no field `{name}` here (an `#[error(..)]` message can name the fields of its variant)"),
                ));
            }
        }
    }
    let mut out = String::new();
    // Display, from `#[error(..)]`.
    out.push_str(&format!(
        "\nimpl{gp} ::std::fmt::Display for {name}{args} {{\n    fn fmt(&self, __f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {{\n        match self {{\n"
    ));
    for c in &cases {
        let body = match c.error.as_ref().map(|(s, _)| s.trim()) {
            Some("transparent") => match c.fields.first() {
                Some(f) => format!("write!(__f, \"{{}}\", {})", f.bind),
                None => "Ok(())".into(),
            },
            Some(spec) => format!("write!(__f, {})", error_args(spec)),
            None => "Ok(())".into(),
        };
        out.push_str(&format!("            {} => {body},\n", c.pattern));
    }
    out.push_str("        }\n    }\n}\n");
    // Error, with `source()` when a field is one.
    // (`#[error(transparent)]` forwards to its field's own `source()`.)
    let transparent = |c: &Case| c.error.as_ref().map(|(s, _)| s.trim()) == Some("transparent");
    let sources: Vec<(&Case, &Field)> = cases
        .iter()
        .filter_map(|c| {
            let f = if transparent(c) {
                c.fields.first()
            } else {
                c.fields.iter().find(|f| f.source)
            };
            f.map(|f| (c, f))
        })
        .collect();
    if sources.is_empty() {
        out.push_str(&format!(
            "impl{gp} ::std::error::Error for {name}{args} {{}}\n"
        ));
    } else {
        out.push_str(&format!(
            "impl{gp} ::std::error::Error for {name}{args} {{\n    fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {{\n        match self {{\n"
        ));
        for (c, f) in &sources {
            let value = if transparent(c) {
                "__s.source()"
            } else {
                "Some(__s)"
            };
            out.push_str(&format!(
                "            {} => {{\n                let __s: &(dyn ::std::error::Error + 'static) = {};\n                {value}\n            }}\n",
                c.pattern, f.bind
            ));
        }
        if sources.len() < cases.len() {
            out.push_str("            _ => None,\n");
        }
        out.push_str("        }\n    }\n}\n");
    }
    // `From` for each `#[from]` field (the variant's only field, or with the others defaulted).
    for c in &cases {
        let Some(f) = c.fields.iter().find(|f| f.from) else {
            continue;
        };
        // (The other fields, a backtrace say, by `Default`.)
        let vals: Vec<String> = c
            .fields
            .iter()
            .map(|o| {
                let v = if o.bind == f.bind {
                    "__source"
                } else {
                    "Default::default()"
                };
                match c.named {
                    Some(true) => format!("{}: {v}", o.bind),
                    _ => v.to_string(),
                }
            })
            .collect();
        let value = match c.named {
            Some(true) => format!("{} {{ {} }}", c.path, vals.join(", ")),
            _ => format!("{}({})", c.path, vals.join(", ")),
        };
        out.push_str(&format!(
            "impl{gp} ::std::convert::From<{}> for {name}{args} {{\n    fn from(__source: {}) -> Self {{\n        {value}\n    }}\n}}\n",
            f.ty, f.ty
        ));
    }
    let end = adt.syntax().text_range().end();
    edits.push((TextRange::new(end, end), out));
}

// ---------------------------------------------------------------- bitflags

/// Comments out, for the hand parsers below.
fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    for line in s.lines() {
        let l = match line.find("//") {
            Some(i) if !line[..i].contains('"') => &line[..i],
            _ => line,
        };
        out.push_str(l);
        out.push('\n');
    }
    out
}

/// Split `s` at top-level occurrences of `sep` (outside brackets and strings).
pub(crate) fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut in_str = false;
    let mut prev = ' ';
    for c in s.chars() {
        if in_str {
            cur.push(c);
            if c == '"' && prev != '\\' {
                in_str = false;
            }
            prev = c;
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if c == sep && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
        prev = c;
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Leading `#[..]` attributes of `s`, and the rest.
fn take_attrs(s: &str) -> (Vec<String>, &str) {
    let mut attrs = Vec::new();
    let mut rest = s.trim_start();
    while rest.starts_with("#[") {
        let mut depth = 0;
        let mut end = rest.len();
        for (i, c) in rest.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        attrs.push(rest[..end].to_string());
        rest = rest[end..].trim_start();
    }
    (attrs, rest)
}

/// `bitflags! { #[derive(..)] pub struct Flags: u32 { const A = 1; .. } .. }`.
fn bitflags(body: &str) -> Option<String> {
    let body = strip_comments(body);
    let mut out = String::new();
    let mut rest = body.as_str();
    while !rest.trim().is_empty() {
        let (attrs, after) = take_attrs(rest);
        let open = after.find('{')?;
        let head = after[..open].trim();
        let (decl, bits) = head.split_once(':')?;
        let decl = decl.trim();
        let name = decl.rsplit(' ').next()?.trim().to_string();
        let vis = decl
            .strip_suffix(&name)?
            .trim()
            .strip_suffix("struct")?
            .trim();
        let bits = bits.trim().to_string();
        // The matching `}`.
        let mut depth = 0;
        let mut close = None;
        for (i, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        let mut flags: Vec<(String, String)> = Vec::new();
        for item in split_top(&after[open + 1..close], ';') {
            let (_, item) = take_attrs(&item);
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let decl = item.strip_prefix("const")?.trim();
            let (fname, value) = decl.split_once('=')?;
            let fname = fname.trim().to_string();
            if fname == "_" {
                continue;
            }
            flags.push((fname, value.trim().to_string()));
        }
        out.push_str(&bitflags_struct(vis, &name, &bits, &attrs, &flags));
        rest = &after[close + 1..];
    }
    Some(out)
}

fn bitflags_struct(
    vis: &str,
    name: &str,
    bits: &str,
    attrs: &[String],
    flags: &[(String, String)],
) -> String {
    let mut debug = false;
    let mut out = String::new();
    for a in attrs {
        let compact: String = a.chars().filter(|c| !c.is_whitespace()).collect();
        if let Some(list) = compact
            .strip_prefix("#[derive(")
            .and_then(|r| r.strip_suffix(")]"))
        {
            let kept: Vec<&str> = list
                .split(',')
                .filter(|d| {
                    let is_debug = matches!(*d, "Debug" | "std::fmt::Debug" | "core::fmt::Debug");
                    debug |= is_debug;
                    !is_debug && !d.is_empty()
                })
                .collect();
            if !kept.is_empty() {
                out.push_str(&format!("#[derive({})]\n", kept.join(", ")));
            }
        } else {
            out.push_str(a);
            out.push('\n');
        }
    }
    let vis_sp = if vis.is_empty() {
        String::new()
    } else {
        format!("{vis} ")
    };
    out.push_str(&format!("{vis_sp}struct {name}({bits});\n"));
    out.push_str(&format!("impl {name} {{\n"));
    for (f, v) in flags {
        out.push_str(&format!("    pub const {f}: Self = Self({v});\n"));
    }
    let all: Vec<String> = flags.iter().map(|(f, _)| format!("Self::{f}.0")).collect();
    let all = if all.is_empty() {
        "0".to_string()
    } else {
        all.join(" | ")
    };
    let names: String = flags
        .iter()
        .map(|(f, _)| format!("            \"{f}\" => Some(Self::{f}),\n"))
        .collect();
    let list: String = flags
        .iter()
        .map(|(f, _)| format!("(\"{f}\", Self::{f})"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!(
        r#"    pub const fn empty() -> Self {{
        Self(0)
    }}
    pub const fn all() -> Self {{
        Self({all})
    }}
    pub const fn bits(&self) -> {bits} {{
        self.0
    }}
    pub const fn from_bits(bits: {bits}) -> Option<Self> {{
        if bits & !Self::all().0 == 0 {{ Some(Self(bits)) }} else {{ None }}
    }}
    pub const fn from_bits_truncate(bits: {bits}) -> Self {{
        Self(bits & Self::all().0)
    }}
    pub const fn from_bits_retain(bits: {bits}) -> Self {{
        Self(bits)
    }}
    pub fn from_name(name: &str) -> Option<Self> {{
        match name {{
{names}            _ => None,
        }}
    }}
    pub const fn is_empty(&self) -> bool {{
        self.0 == 0
    }}
    pub const fn is_all(&self) -> bool {{
        Self::all().0 | self.0 == self.0
    }}
    pub const fn intersects(&self, other: Self) -> bool {{
        self.0 & other.0 != 0
    }}
    pub const fn contains(&self, other: Self) -> bool {{
        self.0 & other.0 == other.0
    }}
    pub fn insert(&mut self, other: Self) {{
        self.0 = self.0 | other.0;
    }}
    pub fn remove(&mut self, other: Self) {{
        self.0 = self.0 & !other.0;
    }}
    pub fn toggle(&mut self, other: Self) {{
        self.0 = self.0 ^ other.0;
    }}
    pub fn set(&mut self, other: Self, value: bool) {{
        if value {{ self.insert(other) }} else {{ self.remove(other) }}
    }}
    pub const fn intersection(self, other: Self) -> Self {{
        Self(self.0 & other.0)
    }}
    pub const fn union(self, other: Self) -> Self {{
        Self(self.0 | other.0)
    }}
    pub const fn difference(self, other: Self) -> Self {{
        Self(self.0 & !other.0)
    }}
    pub const fn symmetric_difference(self, other: Self) -> Self {{
        Self(self.0 ^ other.0)
    }}
    pub const fn complement(self) -> Self {{
        Self(!self.0 & Self::all().0)
    }}
    /// The named flags set (each once, as bitflags' `iter_names` gives them), and what's left.
    fn __names(&self) -> (Vec<(&'static str, Self)>, {bits}) {{
        let mut out = Vec::new();
        let mut rest = self.0;
        for (n, f) in [{list}] {{
            if f.0 != 0 && self.0 & f.0 == f.0 && rest & f.0 != 0 {{
                rest = rest & !f.0;
                out.push((n, f));
            }}
        }}
        (out, rest)
    }}
    pub fn iter_names(&self) -> std::vec::IntoIter<(&'static str, Self)> {{
        self.__names().0.into_iter()
    }}
    pub fn iter(&self) -> std::vec::IntoIter<Self> {{
        let (names, rest) = self.__names();
        let mut out: Vec<Self> = names.into_iter().map(|(_, f)| f).collect();
        if rest != 0 {{
            out.push(Self(rest));
        }}
        out.into_iter()
    }}
}}
impl std::ops::BitOr for {name} {{
    type Output = Self;
    fn bitor(self, o: Self) -> Self {{
        Self(self.0 | o.0)
    }}
}}
impl std::ops::BitAnd for {name} {{
    type Output = Self;
    fn bitand(self, o: Self) -> Self {{
        Self(self.0 & o.0)
    }}
}}
impl std::ops::BitXor for {name} {{
    type Output = Self;
    fn bitxor(self, o: Self) -> Self {{
        Self(self.0 ^ o.0)
    }}
}}
impl std::ops::Sub for {name} {{
    type Output = Self;
    fn sub(self, o: Self) -> Self {{
        Self(self.0 & !o.0)
    }}
}}
impl std::ops::Not for {name} {{
    type Output = Self;
    fn not(self) -> Self {{
        self.complement()
    }}
}}
impl std::ops::BitOrAssign for {name} {{
    fn bitor_assign(&mut self, o: Self) {{
        self.0 = self.0 | o.0;
    }}
}}
impl std::ops::BitAndAssign for {name} {{
    fn bitand_assign(&mut self, o: Self) {{
        self.0 = self.0 & o.0;
    }}
}}
impl std::ops::BitXorAssign for {name} {{
    fn bitxor_assign(&mut self, o: Self) {{
        self.0 = self.0 ^ o.0;
    }}
}}
impl std::ops::SubAssign for {name} {{
    fn sub_assign(&mut self, o: Self) {{
        self.0 = self.0 & !o.0;
    }}
}}
"#
    ));
    if debug {
        out.push_str(&format!(
            r#"impl std::fmt::Debug for {name} {{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {{
        if self.0 == 0 {{
            return write!(f, "{name}({{:#x}})", self.0);
        }}
        let (names, rest) = self.__names();
        let mut text = String::new();
        for (n, _) in names {{
            if !text.is_empty() {{
                text.push_str(" | ");
            }}
            text.push_str(n);
        }}
        if rest != 0 {{
            if !text.is_empty() {{
                text.push_str(" | ");
            }}
            text.push_str(&format!("{{:#x}}", rest));
        }}
        write!(f, "{name}({{}})", text)
    }}
}}
"#
        ));
    }
    out
}

// ---------------------------------------------------------------- cfg_if

/// `cfg_if! { if #[cfg(a)] { .. } else if #[cfg(b)] { .. } else { .. } }`: the first branch
/// whose predicate holds in a script (no OS or target, features off, `debug_assertions` off).
fn cfg_if(body: &str) -> Option<String> {
    let mut rest = body.trim();
    loop {
        if let Some(r) = rest.strip_prefix("if") {
            let r = r.trim_start().strip_prefix("#[cfg(")?;
            let close = matching(r, 0)?;
            let pred = &r[..close];
            let r = r[close + 1..].trim_start().strip_prefix(']')?.trim_start();
            let (items, after) = braced(r)?;
            if cfg_holds(pred) {
                return Some(items.to_string());
            }
            rest = after.trim_start();
            match rest.strip_prefix("else") {
                Some(r) => rest = r.trim_start(),
                None => return Some(String::new()),
            }
        } else {
            let (items, _) = braced(rest)?;
            return Some(items.to_string());
        }
    }
}

/// The index of the `)` closing an already-open `(` in `s` (`depth` more are open).
fn matching(s: &str, depth: i32) -> Option<usize> {
    let mut d = depth;
    for (i, c) in s.char_indices() {
        match c {
            '(' => d += 1,
            ')' => {
                if d == 0 {
                    return Some(i);
                }
                d -= 1;
            }
            _ => {}
        }
    }
    None
}

/// `{ .. } rest` → (the inside, rest).
fn braced(s: &str) -> Option<(&str, &str)> {
    let s = s.strip_prefix('{')?;
    let mut d = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => d += 1,
            '}' => {
                if d == 0 {
                    return Some((&s[..i], &s[i + 1..]));
                }
                d -= 1;
            }
            _ => {}
        }
    }
    None
}

fn cfg_holds(pred: &str) -> bool {
    let p = pred.trim();
    for (f, all) in [("all(", true), ("any(", false)] {
        if let Some(inner) = p.strip_prefix(f).and_then(|r| r.strip_suffix(')')) {
            let parts = split_top(inner, ',');
            return if all {
                parts.iter().all(|x| cfg_holds(x))
            } else {
                parts.iter().any(|x| cfg_holds(x))
            };
        }
    }
    if let Some(inner) = p.strip_prefix("not(").and_then(|r| r.strip_suffix(')')) {
        return !cfg_holds(inner);
    }
    // A JS target: `target_arch = "wasm32"` / `target_family = "wasm"` hold, nothing else does.
    let compact: String = p.chars().filter(|c| !c.is_whitespace()).collect();
    matches!(
        compact.as_str(),
        "target_arch=\"wasm32\"" | "target_family=\"wasm\"" | "target_os=\"unknown\""
    )
}

// ---------------------------------------------------------------- lazy_static

/// `lazy_static! { static ref NAME: T = expr; }` as `static NAME: LazyLock<T> = ..`.
fn lazy_static(body: &str) -> Option<String> {
    let mut out = String::new();
    for item in split_top(&strip_comments(body), ';') {
        let (attrs, item) = take_attrs(&item);
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let at = item.find("static")?;
        let vis = item[..at].trim();
        let decl = item[at + "static".len()..]
            .trim()
            .strip_prefix("ref")?
            .trim();
        let (head, value) = decl.split_once('=')?;
        let (n, ty) = head.split_once(':')?;
        for a in attrs {
            out.push_str(&a);
            out.push('\n');
        }
        let vis = if vis.is_empty() {
            String::new()
        } else {
            format!("{vis} ")
        };
        out.push_str(&format!(
            "{vis}static {}: std::sync::LazyLock<{}> = std::sync::LazyLock::new(|| {});\n",
            n.trim(),
            ty.trim(),
            value.trim()
        ));
    }
    Some(out)
}

// ---------------------------------------------------------------- thread_local

/// `thread_local! { static NAME: T = expr; }` as `static NAME: LocalKey<T> = LocalKey::new(expr);`
/// (one thread, so a `static`: the value is made when the script loads). A `const { .. }`
/// initializer is its block.
fn thread_local(body: &str) -> Option<String> {
    let mut out = String::new();
    for item in split_top(&strip_comments(body), ';') {
        let (attrs, item) = take_attrs(&item);
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let at = item.find("static")?;
        let vis = item[..at].trim();
        let decl = item[at + "static".len()..].trim();
        let (head, value) = decl.split_once('=')?;
        let (n, ty) = head.split_once(':')?;
        let value = value.trim();
        let value = value
            .strip_prefix("const")
            .map(str::trim_start)
            .filter(|v| v.starts_with('{'))
            .unwrap_or(value);
        for a in attrs {
            out.push_str(&a);
            out.push('\n');
        }
        let vis = if vis.is_empty() {
            String::new()
        } else {
            format!("{vis} ")
        };
        out.push_str(&format!(
            "{vis}static {}: LocalKey<{}> = LocalKey::new({});\n",
            n.trim(),
            ty.trim(),
            value
        ));
    }
    Some(out)
}

// ---------------------------------------------------------------- rayon

/// rayon's parallel methods and their sequential twins (what one thread does).
const RAYON_METHODS: &[(&str, &str)] = &[
    ("par_iter", "iter"),
    ("par_iter_mut", "iter_mut"),
    ("into_par_iter", "into_iter"),
    ("par_chunks", "chunks"),
    ("par_chunks_mut", "chunks_mut"),
    ("par_chunks_exact", "chunks_exact"),
    ("par_rchunks", "rchunks"),
    ("par_windows", "windows"),
    ("par_sort", "sort"),
    ("par_sort_unstable", "sort_unstable"),
    ("par_sort_by", "sort_by"),
    ("par_sort_by_key", "sort_by_key"),
    ("par_sort_unstable_by", "sort_unstable_by"),
    ("par_sort_unstable_by_key", "sort_unstable_by_key"),
    ("par_sort_by_cached_key", "sort_by_cached_key"),
    ("par_extend", "extend"),
    ("par_drain", "drain"),
    ("par_lines", "lines"),
    ("par_chars", "chars"),
    ("par_char_indices", "char_indices"),
    ("par_bytes", "bytes"),
    ("par_split", "split"),
    ("par_split_whitespace", "split_whitespace"),
    ("par_split_terminator", "split_terminator"),
    ("par_split_mut", "split_mut"),
    ("par_bridge", ""),
    ("flat_map_iter", "flat_map"),
    ("flatten_iter", "flatten"),
    ("reduce_with", "reduce"),
    ("find_any", "find"),
    ("find_first", "find"),
    ("find_map_any", "find_map"),
    ("find_map_first", "find_map"),
    ("position_any", "position"),
    ("position_first", "position"),
    ("with_min_len", ""),
    ("with_max_len", ""),
    ("by_exponential_blocks", ""),
    ("by_uniform_blocks", ""),
];

/// rayon's traits as std's.
const RAYON_TRAITS: &[(&str, &str)] = &[
    ("ParallelIterator", "Iterator"),
    ("IndexedParallelIterator", "Iterator"),
    ("IntoParallelIterator", "IntoIterator"),
    ("ParallelBridge", "Iterator"),
];

/// rayon, sequentially: `v.par_iter()` is `v.iter()` (and the other `par_` methods theirs),
/// `.reduce(|| id, op)` folds from `id`, `.fold(|| init, op)` is one accumulator,
/// `rayon::join(a, b)` calls `a` then `b`, and `with_min_len(..)` / `par_bridge()` go away.
fn rayon(node: &ra_ap_syntax::SyntaxNode, edits: &mut Vec<(TextRange, String)>) {
    if let Some(mc) = ast::MethodCallExpr::cast(node.clone()) {
        let Some(name) = mc.name_ref() else { return };
        let n = name.text().to_string();
        let args: Vec<ast::Expr> = mc
            .arg_list()
            .map(|l| l.args().collect())
            .unwrap_or_default();
        if let Some((_, to)) = RAYON_METHODS.iter().find(|(p, _)| *p == n) {
            if to.is_empty() {
                // `.with_min_len(64)`: the whole call, from its `.`.
                let (Some(dot), Some(end)) = (mc.dot_token(), mc.arg_list()) else {
                    return;
                };
                edits.push((
                    dot.text_range().cover(end.syntax().text_range()),
                    String::new(),
                ));
            } else {
                edits.push((name.syntax().text_range(), to.to_string()));
            }
            return;
        }
        // `.reduce(|| 0, |a, b| a + b)` / `.fold(|| 0, |acc, x| ..)` (std's take one argument
        // or a value): the identity called once.
        let zero_arg_closure = |e: &ast::Expr| {
            matches!(e, ast::Expr::ClosureExpr(c)
                if c.param_list().is_some_and(|p| p.params().next().is_none()))
        };
        if (n == "reduce" && args.len() == 2)
            || (n == "fold" && args.len() == 2 && zero_arg_closure(&args[0]))
        {
            let r = args[0].syntax().text_range();
            edits.push((TextRange::empty(r.start()), "(".into()));
            edits.push((TextRange::empty(r.end()), ")()".into()));
            if n == "reduce" {
                edits.push((name.syntax().text_range(), "fold".into()));
            } else {
                let whole = mc.syntax().text_range();
                edits.push((TextRange::empty(whole.start()), "std::iter::once(".into()));
                edits.push((TextRange::empty(whole.end()), ")".into()));
            }
        }
        return;
    }
    if let Some(call) = ast::CallExpr::cast(node.clone()) {
        let Some(ast::Expr::PathExpr(p)) = call.expr() else {
            return;
        };
        let Some(path) = p.path() else { return };
        let text = path.syntax().text().to_string();
        let args: Vec<ast::Expr> = call
            .arg_list()
            .map(|l| l.args().collect())
            .unwrap_or_default();
        match text.as_str() {
            // `rayon::join(|| a(), || b())`: both, in order.
            // (Inserts around the arguments, which may have their own rewrites.)
            "rayon::join" if args.len() == 2 => {
                edits.push((path.syntax().text_range(), String::new()));
                for a in &args {
                    let r = a.syntax().text_range();
                    edits.push((TextRange::empty(r.start()), "(".into()));
                    edits.push((TextRange::empty(r.end()), ")()".into()));
                }
            }
            "rayon::current_num_threads" => {
                edits.push((call.syntax().text_range(), "1usize".into()));
            }
            _ => {}
        }
        return;
    }
    if let Some(nr) = ast::NameRef::cast(node.clone())
        && let Some((_, to)) = RAYON_TRAITS.iter().find(|(p, _)| nr.text() == *p)
        && !nr
            .syntax()
            .ancestors()
            .any(|a| ast::Use::can_cast(a.kind()))
    {
        edits.push((nr.syntax().text_range(), to.to_string()));
    }
}

/// rayon's method renames in a macro's tokens (`println!("{}", v.par_iter().sum::<u32>())`),
/// where there's no syntax tree: `.par_iter` and the like by name, `.with_min_len(..)` and
/// `.par_bridge()` up to their closing parenthesis, `rayon::current_num_threads()`.
fn rayon_tokens(tt: &ast::TokenTree, edits: &mut Vec<(TextRange, String)>) {
    use ra_ap_syntax::SyntaxKind::{COLON, COLON2, DOT, IDENT, L_PAREN, R_PAREN, WHITESPACE};
    let toks: Vec<_> = tt
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != WHITESPACE)
        .collect();
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if t.kind() == IDENT
            && i > 0
            && toks[i - 1].kind() == DOT
            && let Some((_, to)) = RAYON_METHODS.iter().find(|(p, _)| *p == t.text())
        {
            if !to.is_empty() {
                edits.push((t.text_range(), to.to_string()));
            } else if toks.get(i + 1).is_some_and(|p| p.kind() == L_PAREN) {
                let mut depth = 0;
                let mut j = i + 1;
                while j < toks.len() {
                    match toks[j].kind() {
                        L_PAREN => depth += 1,
                        R_PAREN => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if j < toks.len() {
                    let r = toks[i - 1].text_range().cover(toks[j].text_range());
                    edits.push((r, String::new()));
                    i = j;
                }
            }
        } else if t.kind() == IDENT && t.text() == "rayon" {
            // (`::` is one token or two.)
            let mut j = i + 1;
            while toks
                .get(j)
                .is_some_and(|c| matches!(c.kind(), COLON | COLON2))
            {
                j += 1;
            }
            if j > i + 1
                && toks
                    .get(j)
                    .is_some_and(|f| f.text() == "current_num_threads")
                && toks.get(j + 1).is_some_and(|p| p.kind() == L_PAREN)
                && toks.get(j + 2).is_some_and(|p| p.kind() == R_PAREN)
            {
                edits.push((
                    t.text_range().cover(toks[j + 2].text_range()),
                    "1usize".into(),
                ));
                i = j + 2;
            }
        }
        i += 1;
    }
}

// ---------------------------------------------------------------- serde_json's json!

/// The tokens and token trees of `tt` without its delimiters, whitespace and comments.
pub(crate) fn tt_items(tt: &ast::TokenTree) -> Vec<ra_ap_syntax::SyntaxElement> {
    use ra_ap_syntax::SyntaxKind::{COMMENT, R_BRACK, R_CURLY, R_PAREN, WHITESPACE};
    let mut items: Vec<_> = tt
        .syntax()
        .children_with_tokens()
        .filter(|e| !matches!(e.kind(), WHITESPACE | COMMENT))
        .collect();
    if !items.is_empty() {
        items.remove(0);
    }
    if items
        .last()
        .is_some_and(|e| matches!(e.kind(), R_PAREN | R_BRACK | R_CURLY))
    {
        items.pop();
    }
    items
}

/// The delimiter a token tree opens with.
fn tt_open(tt: &ast::TokenTree) -> Option<ra_ap_syntax::SyntaxKind> {
    tt.syntax().first_token().map(|t| t.kind())
}

/// `items` split at their top-level commas (a trailing one ends the list).
/// A comma between the `<` and `>` of a qualified path (`<R<(), ()> as Clone>::clone(&x)`, at
/// the start of a value or after `::`) is the value's own, as `json!`'s `$value:expr` takes it.
fn split_commas(items: &[ra_ap_syntax::SyntaxElement]) -> Vec<Vec<ra_ap_syntax::SyntaxElement>> {
    use ra_ap_syntax::SyntaxKind::{COLON, COLON2, COMMA, EQ, L_ANGLE, MINUS, R_ANGLE};
    let mut out = vec![Vec::new()];
    let mut angle = 0usize;
    let mut prev: Option<ra_ap_syntax::SyntaxKind> = None;
    for e in items {
        let k = e.kind();
        if k.is_trivia() {
            continue;
        }
        match k {
            L_ANGLE if angle > 0 || matches!(prev, None | Some(COMMA | COLON | COLON2)) => {
                angle += 1
            }
            R_ANGLE if angle > 0 && !matches!(prev, Some(MINUS | EQ)) => angle -= 1,
            _ => {}
        }
        prev = Some(k);
        if k == COMMA && angle == 0 {
            out.push(Vec::new());
        } else if let Some(last) = out.last_mut() {
            last.push(e.clone());
        }
    }
    if out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// The source of `items` (from the first's start to the last's end).
fn items_text(items: &[ra_ap_syntax::SyntaxElement], text: &str) -> String {
    match (items.first(), items.last()) {
        (Some(a), Some(b)) => {
            let (s, e) = (
                usize::from(a.text_range().start()),
                usize::from(b.text_range().end()),
            );
            text.get(s..e).unwrap_or("").to_string()
        }
        _ => String::new(),
    }
}

/// When `items[i..]` start with `json!(..)` (or `serde_json::json!(..)`): how many items it
/// spans, and its token tree.
fn json_call_at(
    items: &[ra_ap_syntax::SyntaxElement],
    i: usize,
) -> Option<(usize, ast::TokenTree)> {
    use ra_ap_syntax::SyntaxKind::{BANG, COLON, COLON2, IDENT};
    let is_ident = |e: &ra_ap_syntax::SyntaxElement, s: &str| {
        e.kind() == IDENT && e.as_token().is_some_and(|t| t.text() == s)
    };
    let mut j = i;
    if items.get(j).is_some_and(|e| is_ident(e, "serde_json")) {
        j += 1;
        let colons = j;
        while items
            .get(j)
            .is_some_and(|e| matches!(e.kind(), COLON | COLON2))
        {
            j += 1;
        }
        if j == colons {
            return None;
        }
    }
    if !items.get(j).is_some_and(|e| is_ident(e, "json")) || items.get(j + 1)?.kind() != BANG {
        return None;
    }
    let tt = items
        .get(j + 2)?
        .as_node()
        .cloned()
        .and_then(ast::TokenTree::cast)?;
    Some((j + 3 - i, tt))
}

/// A `json!` value (`null`, `[..]`, `{ key: value, .. }`, or an expression) as Rust that makes
/// the `serde_json::Value`.
fn json_of(items: &[ra_ap_syntax::SyntaxElement], text: &str) -> String {
    use ra_ap_syntax::SyntaxKind::{COLON, COLON2, FALSE_KW, IDENT, L_BRACK, L_CURLY, TRUE_KW};
    if let [one] = items {
        if one.kind() == IDENT && one.as_token().is_some_and(|t| t.text() == "null") {
            return "serde_json::Value::Null".into();
        }
        if matches!(one.kind(), TRUE_KW | FALSE_KW) {
            return format!("serde_json::Value::Bool({one})");
        }
        if let Some(tt) = one.as_node().cloned().and_then(ast::TokenTree::cast) {
            let inner = tt_items(&tt);
            match tt_open(&tt) {
                Some(L_BRACK) => {
                    let parts: Vec<String> = split_commas(&inner)
                        .iter()
                        .map(|p| json_of(p, text))
                        .collect();
                    return format!("serde_json::Value::Array(vec![{}])", parts.join(", "));
                }
                Some(L_CURLY) => {
                    let mut out = String::from(
                        "serde_json::Value::Object({ let mut __m = std::collections::BTreeMap::new(); ",
                    );
                    for entry in split_commas(&inner) {
                        // The key's `:` (not a path's `::`).
                        let at = (0..entry.len()).find(|&k| {
                            entry[k].kind() == COLON
                                && !entry
                                    .get(k + 1)
                                    .is_some_and(|n| matches!(n.kind(), COLON | COLON2))
                                && !(k > 0 && matches!(entry[k - 1].kind(), COLON | COLON2))
                        });
                        let Some(at) = at else { continue };
                        out.push_str(&format!(
                            "__m.insert(String::from({}), {}); ",
                            items_text(&entry[..at], text),
                            json_of(&entry[at + 1..], text)
                        ));
                    }
                    out.push_str("__m })");
                    return out;
                }
                _ => {}
            }
        }
    }
    if let Some((n, tt)) = json_call_at(items, 0)
        && n == items.len()
    {
        return json_of(&tt_items(&tt), text);
    }
    format!(
        "serde_json::to_value(&({})).unwrap()",
        items_text(items, text)
    )
}

/// The `json!(..)` calls among a macro's tokens (`println!("{}", json!({ "a": 1 }))`).
fn json_in_tokens(tt: &ast::TokenTree, text: &str, edits: &mut Vec<(TextRange, String)>) {
    let items = tt_items(tt);
    let mut i = 0;
    while i < items.len() {
        if let Some((n, body)) = json_call_at(&items, i) {
            let r = items[i].text_range().cover(items[i + n - 1].text_range());
            edits.push((r, json_of(&tt_items(&body), text)));
            i += n;
            continue;
        }
        if let Some(sub) = items[i].as_node().cloned().and_then(ast::TokenTree::cast) {
            json_in_tokens(&sub, text, edits);
        }
        i += 1;
    }
}

/// Whether `mc` is `json!(..)`.
fn is_json_call(mc: &ast::MacroCall) -> bool {
    mc.path()
        .and_then(|p| p.segment())
        .and_then(|s| s.name_ref())
        .is_some_and(|n| n.text() == "json")
}

// ---------------------------------------------------------------- regex

/// The bundled regex's spellings: `regex::Error` is the prelude's `RegexError` and
/// `regex::escape` its `__regex_escape`; a closure replacer (`re.replace_all(s, |caps:
/// &Captures| ..)`) calls `__replace_fn` with how many to replace.
fn regex(node: &ra_ap_syntax::SyntaxNode, edits: &mut Vec<(TextRange, String)>) {
    if let Some(path) = ast::Path::cast(node.clone())
        && path.qualifier().is_none()
        && path.syntax().text() == "regex"
        && !path
            .syntax()
            .ancestors()
            .any(|a| ast::Use::can_cast(a.kind()))
        && let Some(parent) = path.parent_path()
        && let Some(name) = parent.segment().and_then(|s| s.name_ref())
    {
        let to = match name.text() {
            "Error" => "RegexError",
            "escape" => "__regex_escape",
            _ => return,
        };
        edits.push((
            path.syntax().text_range().cover(name.syntax().text_range()),
            to.into(),
        ));
        return;
    }
    let Some(mc) = ast::MethodCallExpr::cast(node.clone()) else {
        return;
    };
    let Some(name) = mc.name_ref() else { return };
    let args: Vec<ast::Expr> = mc
        .arg_list()
        .map(|l| l.args().collect())
        .unwrap_or_default();
    let closure_last = args
        .last()
        .is_some_and(|a| matches!(a, ast::Expr::ClosureExpr(_)));
    let limit = match (name.text(), args.len()) {
        ("replace", 2) if closure_last => "1usize, ",
        ("replace_all", 2) if closure_last => "usize::MAX, ",
        ("replacen", 3) if closure_last => {
            // (0 is all of them.)
            let n = args[1].syntax().text_range();
            edits.push((TextRange::empty(n.start()), "match ".into()));
            edits.push((
                TextRange::empty(n.end()),
                " { 0 => usize::MAX, __n => __n }".into(),
            ));
            ""
        }
        _ => return,
    };
    edits.push((name.syntax().text_range(), "__replace_fn".into()));
    if !limit.is_empty() {
        let f = args[1].syntax().text_range();
        edits.push((TextRange::empty(f.start()), limit.into()));
    }
}

/// `regex::escape` / `regex::Error` among a macro's tokens.
fn regex_tokens(tt: &ast::TokenTree, edits: &mut Vec<(TextRange, String)>) {
    use ra_ap_syntax::SyntaxKind::{COLON, COLON2, IDENT, WHITESPACE};
    let toks: Vec<_> = tt
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != WHITESPACE)
        .collect();
    for i in 0..toks.len() {
        if toks[i].kind() != IDENT || toks[i].text() != "regex" {
            continue;
        }
        let mut j = i + 1;
        while toks
            .get(j)
            .is_some_and(|c| matches!(c.kind(), COLON | COLON2))
        {
            j += 1;
        }
        let to = match toks.get(j).map(|t| t.text().to_string()).as_deref() {
            Some("escape") if j > i + 1 => "__regex_escape",
            Some("Error") if j > i + 1 => "RegexError",
            _ => continue,
        };
        edits.push((toks[i].text_range().cover(toks[j].text_range()), to.into()));
    }
}

// ---------------------------------------------------------------- smallvec

/// `SmallVec<[T; N]>` as `Vec<T>` (the inline buffer is an allocation detail), with the element
/// type rewritten too when it's a `SmallVec` itself.
fn smallvec_type(t: &ast::Type) -> String {
    if let ast::Type::PathType(p) = t
        && let Some(seg) = p.path().and_then(|p| p.segment())
        && seg.name_ref().is_some_and(|n| n.text() == "SmallVec")
        && let Some(elem) = smallvec_elem(&seg)
    {
        return format!("Vec<{}>", smallvec_type(&elem));
    }
    t.syntax().text().to_string()
}

/// The `T` of a `SmallVec<[T; N]>` segment.
fn smallvec_elem(seg: &ast::PathSegment) -> Option<ast::Type> {
    let ast::GenericArg::TypeArg(a) = seg.generic_arg_list()?.generic_args().next()? else {
        return None;
    };
    match a.ty()? {
        ast::Type::ArrayType(arr) => arr.ty(),
        _ => None,
    }
}

/// The bundled `smallvec`: `SmallVec<[T; N]>` / `SmallVec::<[T; N]>::new()` / `smallvec::SmallVec`
/// are `Vec`'s, `SmallVec::from_vec(v)` / `from_slice(s)` / `from_buf(a)` are `Vec::from(..)`, and
/// `smallvec![..]` is `vec![..]`.
fn smallvec_edits(file: &SourceFile, edits: &mut Vec<(TextRange, String)>) {
    for seg in file
        .syntax()
        .descendants()
        .filter_map(ast::PathSegment::cast)
    {
        let Some(name) = seg.name_ref() else { continue };
        if name.text() != "SmallVec"
            || seg.syntax().ancestors().any(|a| ast::Use::can_cast(a.kind()))
            // (Inside another `SmallVec<[..]>`: rewritten with it.)
            || seg.syntax().ancestors().skip(1).any(|a| {
                ast::PathSegment::cast(a)
                    .and_then(|s| s.name_ref())
                    .is_some_and(|n| n.text() == "SmallVec")
            })
        {
            continue;
        }
        let Some(path) = seg.parent_path().into() else {
            continue;
        };
        let path: ast::Path = path;
        // `smallvec::SmallVec`: the crate goes too.
        let start = match path.qualifier() {
            Some(q)
                if matches!(
                    q.syntax().text().to_string().as_str(),
                    "smallvec" | "::smallvec"
                ) =>
            {
                path.syntax().text_range().start()
            }
            _ => seg.syntax().text_range().start(),
        };
        let mut new = String::from("Vec");
        if let Some(gl) = seg.generic_arg_list() {
            let turbofish = gl.coloncolon_token().is_some();
            match smallvec_elem(&seg) {
                Some(elem) => {
                    new.push_str(if turbofish { "::<" } else { "<" });
                    new.push_str(&smallvec_type(&elem));
                    new.push('>');
                }
                None => new.push_str(&gl.syntax().text().to_string()),
            }
        }
        edits.push((TextRange::new(start, seg.syntax().text_range().end()), new));
        // `SmallVec::from_vec(v)` and the other constructors from a buffer.
        if let Some(outer) = path.parent_path()
            && let Some(f) = outer.segment().and_then(|s| s.name_ref())
            && matches!(
                f.text().to_string().as_str(),
                "from_vec" | "from_slice" | "from_buf"
            )
        {
            edits.push((f.syntax().text_range(), "from".into()));
        }
    }
    // `smallvec![1, 2]` (also `smallvec::smallvec!`, and inside other macros' arguments).
    for mc in file.syntax().descendants().filter_map(ast::MacroCall::cast) {
        if let Some(p) = mc.path()
            && p.segment()
                .and_then(|s| s.name_ref())
                .is_some_and(|n| n.text() == "smallvec")
        {
            edits.push((p.syntax().text_range(), "vec".into()));
        }
    }
    for tt in file.syntax().descendants().filter_map(ast::TokenTree::cast) {
        let toks: Vec<_> = tt
            .syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        for (i, t) in toks.iter().enumerate() {
            if t.text() == "smallvec" && toks.get(i + 1).is_some_and(|n| n.text() == "!") {
                edits.push((t.text_range(), "vec".into()));
            }
        }
    }
}

// ---------------------------------------------------------------- indexmap

/// `indexmap! { k => v, .. }` / `indexset! { x, .. }`: `IndexMap::from([(k, v), ..])` /
/// `IndexSet::from([x, ..])` (`IndexMap::new()` when empty).
fn indexmap_macros(file: &SourceFile, edits: &mut Vec<(TextRange, String)>) {
    for mc in file.syntax().descendants().filter_map(ast::MacroCall::cast) {
        let Some(name) = mc.path().and_then(|p| p.segment()?.name_ref()) else {
            continue;
        };
        let ty = match name.text().to_string().as_str() {
            "indexmap" => "IndexMap",
            "indexset" => "IndexSet",
            _ => continue,
        };
        let Some(tt) = mc.token_tree() else { continue };
        // The items, split at top-level commas (a nested `(..)` / `[..]` / `{..}` is one child).
        let children: Vec<_> = tt.syntax().children_with_tokens().collect();
        // (An unclosed `indexmap! {` while typing: left for the parser's error.)
        let closed = children.len() >= 2
            && matches!(
                children[children.len() - 1].kind(),
                ra_ap_syntax::SyntaxKind::R_CURLY
                    | ra_ap_syntax::SyntaxKind::R_BRACK
                    | ra_ap_syntax::SyntaxKind::R_PAREN
            );
        if !closed {
            continue;
        }
        let inner = &children[1..children.len() - 1];
        let mut items: Vec<String> = vec![String::new()];
        for el in inner {
            if el.kind() == ra_ap_syntax::SyntaxKind::COMMA {
                items.push(String::new());
            } else if let Some(last) = items.last_mut() {
                last.push_str(&el.to_string());
            }
        }
        let items: Vec<String> = items
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let new = if items.is_empty() {
            format!("{ty}::new()")
        } else if ty == "IndexMap" {
            let pairs: Vec<String> = items
                .iter()
                .map(|it| match it.split_once("=>") {
                    Some((k, v)) => format!("({}, {})", k.trim(), v.trim()),
                    None => format!("({it})"),
                })
                .collect();
            format!("{ty}::from([{}])", pairs.join(", "))
        } else {
            format!("{ty}::from([{}])", items.join(", "))
        };
        edits.push((mc.syntax().text_range(), new));
    }
}

// ---------------------------------------------------------------- petgraph

/// petgraph's functions and walkers, which the prelude has as methods: `dijkstra(&g, a, ..)`
/// (also `algo::dijkstra` / `petgraph::algo::dijkstra`) is `(&g).__pg_dijkstra(a, ..)`, so it's
/// `Graph`'s or `GraphMap`'s by the graph's type; `bfs.next(&g)` (a walker's: an iterator's
/// `next` takes no argument) is `bfs.__pg_next(|__pg_n| (&g).neighbors(__pg_n).collect::<..>())`;
/// `from_edges(&[(0, 1), ..])` / `extend_with_edges` with pairs are their `__pg_..2` forms (with
/// `E::default()` weights); and `Outgoing` / `Incoming` from `use petgraph::Direction::..`
/// are `PgDirection`'s.
fn petgraph_edits(file: &SourceFile, edits: &mut Vec<(TextRange, String)>) {
    let cx = PgCtx {
        algos: renamed_imports(file, &PETGRAPH_ALGO_IMPORTS),
        root: renamed_imports(file, &PETGRAPH_ROOT_IMPORTS),
        dirs: renamed_imports(file, &PETGRAPH_DIRECTION_IMPORTS),
        types: renamed_imports(file, &PETGRAPH_RENAMES),
        text: file.syntax().text().to_string(),
    };
    petgraph_walk(file.syntax(), &cx, 0, edits);
}

/// What the file's `use`s bring in from petgraph.
struct PgCtx {
    algos: std::collections::HashMap<String, String>,
    root: std::collections::HashMap<String, String>,
    dirs: std::collections::HashMap<String, String>,
    types: std::collections::HashMap<String, String>,
    text: String,
}

/// `petgraph_edits` over `root`, whose positions are `offset` before the file's (a macro's
/// arguments, parsed on their own).
fn petgraph_walk(
    root: &ra_ap_syntax::SyntaxNode,
    cx: &PgCtx,
    offset: i64,
    out: &mut Vec<(TextRange, String)>,
) {
    use ra_ap_syntax::SyntaxKind::{COMMA, R_PAREN};
    let mut edits: Vec<(TextRange, String)> = Vec::new();
    let edits = &mut edits;
    let (algos, root_imports, dirs, types) = (&cx.algos, &cx.root, &cx.dirs, &cx.types);
    let declares_fn = |n: &str| cx.text.contains(&format!("fn {n}"));
    // The algorithm a call's path names, if any.
    let algo_of = |path: &ast::Path| -> Option<String> {
        let name = path.segment()?.name_ref()?.text().to_string();
        match path.qualifier() {
            Some(q) => {
                let qt = q.syntax().text().to_string();
                let algo_mod = qt == "petgraph::algo"
                    || qt == "::petgraph::algo"
                    || (qt == "algo" && root_imports.get("algo").is_some_and(|i| i == "algo"));
                (algo_mod && PETGRAPH_ALGOS.contains(&name.as_str())).then_some(name)
            }
            None => {
                let item = algos.get(&name)?;
                (PETGRAPH_ALGOS.contains(&item.as_str()) && !declares_fn(&name))
                    .then(|| item.clone())
            }
        }
    };
    // `(f)(a, b)` -> `(a).__pg_f(b)`: the path goes, the first comma (or the closing paren) opens
    // the method call.
    let as_method = |path_range: TextRange,
                     algo: &str,
                     args: &ra_ap_syntax::SyntaxNode,
                     edits: &mut Vec<(TextRange, String)>| {
        let toks: Vec<_> = args
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .collect();
        let call = format!(").__pg_{algo}(");
        if let Some(comma) = toks.iter().find(|t| t.kind() == COMMA) {
            edits.push((path_range, String::new()));
            edits.push((comma.text_range(), call));
        } else if let Some(close) = toks.iter().find(|t| t.kind() == R_PAREN) {
            edits.push((path_range, String::new()));
            edits.push((TextRange::empty(close.text_range().start()), call));
        }
    };
    for node in root.descendants() {
        if let Some(c) = ast::CallExpr::cast(node.clone())
            && let Some(ast::Expr::PathExpr(pe)) = c.expr()
            && let Some(path) = pe.path()
            && let Some(args) = c.arg_list()
        {
            if let Some(algo) = algo_of(&path)
                && args.args().next().is_some()
            {
                as_method(path.syntax().text_range(), &algo, args.syntax(), edits);
                continue;
            }
            // `UnGraph::<i32, ()>::from_edges(&[(0, 1), ..])`.
            if let Some(seg) = path.segment()
                && let Some(name) = seg.name_ref()
                && name.text() == "from_edges"
                && path.qualifier().is_some_and(|q| {
                    q.segments().any(|s| {
                        s.name_ref().is_some_and(|n| {
                            PETGRAPH_NAMES
                                .iter()
                                .any(|(p, _)| *p == n.text() && p.contains("Graph"))
                        })
                    })
                })
                && args
                    .args()
                    .next()
                    .is_some_and(|a| edge_tuple_arity(&a) == Some(2))
            {
                edits.push((name.syntax().text_range(), "__pg_from_edges2".into()));
            }
            continue;
        }
        if let Some(m) = ast::MethodCallExpr::cast(node.clone())
            && let Some(name) = m.name_ref()
            && let Some(args) = m.arg_list()
        {
            let list: Vec<ast::Expr> = args.args().collect();
            if name.text() == "next" && list.len() == 1 {
                let r = list[0].syntax().text_range();
                edits.push((name.syntax().text_range(), "__pg_next".into()));
                edits.push((TextRange::empty(r.start()), "|__pg_n| (".into()));
                edits.push((
                    TextRange::empty(r.end()),
                    ").neighbors(__pg_n).collect::<Vec<_>>()".into(),
                ));
            } else if name.text() == "extend_with_edges"
                && list.len() == 1
                && edge_tuple_arity(&list[0]) == Some(2)
            {
                edits.push((name.syntax().text_range(), "__pg_extend_with_edges2".into()));
            }
            continue;
        }
        // The edge type, as an argument: `Graph<N, E>` / `DiGraph<N, E>` are `PgGraph<N, E,
        // PgDirected>`, `UnGraph::new_undirected()` is `PgGraph::<_, _, PgUndirected>::..`.
        if let Some(seg) = ast::PathSegment::cast(node.clone())
            && let Some(name) = seg.name_ref()
            && !seg
                .syntax()
                .ancestors()
                .any(|a| ast::Use::can_cast(a.kind()))
        {
            let item = match seg.parent_path().qualifier() {
                Some(q) => PETGRAPH_RENAMES
                    .roots
                    .contains(&q.syntax().text().to_string().as_str())
                    .then(|| name.text().to_string()),
                None => types.get(&name.text().to_string()).cloned(),
            };
            let ty = match item.as_deref() {
                Some("Graph" | "GraphMap" | "DiGraph" | "DiGraphMap") => "PgDirected",
                Some("UnGraph" | "UnGraphMap") => "PgUndirected",
                _ => "",
            };
            let alias = matches!(
                item.as_deref(),
                Some("DiGraph" | "UnGraph" | "DiGraphMap" | "UnGraphMap")
            );
            match seg.generic_arg_list() {
                Some(gl) if !ty.is_empty() && gl.generic_args().count() == 2 => {
                    if let Some(close) = gl.r_angle_token() {
                        edits.push((
                            TextRange::empty(close.text_range().start()),
                            format!(", {ty}"),
                        ));
                    }
                }
                // (`UnGraph::new_undirected()`, `DiGraphMap::new()`.)
                None if alias && seg.parent_path().parent_path().is_some() => {
                    edits.push((
                        TextRange::empty(name.syntax().text_range().end()),
                        format!("::<_, _, {ty}>"),
                    ));
                }
                _ => {}
            }
        }
        // `Outgoing` after `use petgraph::Direction::Outgoing;`.
        if let Some(path) = ast::Path::cast(node.clone())
            && path.qualifier().is_none()
            && !path
                .syntax()
                .ancestors()
                .any(|a| ast::Use::can_cast(a.kind()))
            && let Some(name) = path.segment().and_then(|s| s.name_ref())
            && let Some(item) = dirs.get(&name.text().to_string())
            && matches!(item.as_str(), "Outgoing" | "Incoming")
        {
            edits.push((name.syntax().text_range(), format!("PgDirection::{item}")));
        }
        // The same in a macro's arguments (`println!("{:?}", toposort(&g, None))`), parsed as
        // a call's.
        if let Some(mc) = ast::MacroCall::cast(node.clone())
            && let Some(tt) = mc.token_tree()
        {
            let t = tt.syntax().text().to_string();
            if t.len() >= 2 && t.is_char_boundary(1) && t.is_char_boundary(t.len() - 1) {
                let prefix = "fn __pg_f() { __pg_f";
                let inner = &t[1..t.len() - 1];
                let sub = format!("{prefix}({inner}); }}");
                let parsed = SourceFile::parse(&sub, EDITION).tree();
                let start = i64::from(u32::from(tt.syntax().text_range().start()));
                petgraph_walk(parsed.syntax(), cx, start - prefix.len() as i64, edits);
            }
        }
    }
    // (Positions in the file.)
    for (r, new) in std::mem::take(edits) {
        let at = |x: ra_ap_syntax::TextSize| {
            ra_ap_syntax::TextSize::from((i64::from(u32::from(x)) + offset).max(0) as u32)
        };
        out.push((TextRange::new(at(r.start()), at(r.end())), new));
    }
}

/// How many parts the first edge of `&[(0, 1), ..]` / `vec![(a, b, w), ..]` has.
fn edge_tuple_arity(e: &ast::Expr) -> Option<usize> {
    let t = e.syntax().text().to_string();
    let t = t.trim_start_matches('&').trim_start();
    let t = t.strip_prefix("vec!").unwrap_or(t).trim_start();
    let t = t.strip_prefix('[')?.trim_start();
    if !t.starts_with('(') {
        return None;
    }
    let (mut depth, mut commas, mut last_comma) = (0, 0, false);
    for ch in t.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(if last_comma { commas } else { commas + 1 });
                }
            }
            ',' if depth == 1 => commas += 1,
            _ => {}
        }
        if !ch.is_whitespace() {
            last_comma = ch == ',';
        }
    }
    None
}

// ---------------------------------------------------------------- crate items as prelude items

/// A crate (or std module) whose items are a prelude's under other names (so they can't collide
/// with yours): the paths that reach it, its names, what `use krate::*` brings in, and the error
/// its own `Result<T>` alias adds.
struct Renames {
    roots: &'static [&'static str],
    names: &'static [(&'static str, &'static str)],
    star: &'static [&'static str],
    result_err: Option<&'static str>,
}

const IO_RENAMES: Renames = Renames {
    roots: &[
        "io",
        "std::io",
        "::std::io",
        "io::prelude",
        "std::io::prelude",
    ],
    names: &[
        ("Error", "IoError"),
        ("ErrorKind", "IoErrorKind"),
        ("Write", "IoWrite"),
        ("stdout", "__io_stdout"),
        ("stderr", "__io_stderr"),
        ("stdin", "__io_stdin"),
        ("Stdout", "IoStdout"),
        ("StdoutLock", "IoStdoutLock"),
        ("Stderr", "IoStderr"),
        ("StderrLock", "IoStderrLock"),
        ("Stdin", "IoStdin"),
        ("StdinLock", "IoStdinLock"),
        ("BufWriter", "IoBufWriter"),
        ("BufReader", "IoBufReader"),
        ("IoSlice", "IoIoSlice"),
    ],
    star: &["Write", "Error", "ErrorKind", "stdout", "stderr", "stdin"],
    result_err: Some("IoError"),
};

// `rustc_hash`'s maps and sets are std's (a JS `Map` / `Set` hashes its own way; the faster hash
// is the point of the crate, and the runtime's is already fast).
const RUSTC_HASH_RENAMES: Renames = Renames {
    roots: &["rustc_hash", "::rustc_hash"],
    names: &[
        ("FxHashMap", "HashMap"),
        ("FxHashSet", "HashSet"),
        ("FxHasher", "DefaultHasher"),
        ("FxBuildHasher", "RandomState"),
    ],
    star: &["FxHashMap", "FxHashSet", "FxHasher", "FxBuildHasher"],
    result_err: None,
};

// `fmt::Write` (`use std::fmt::Write;`): the io prelude's `FmtWrite`.
const FMT_RENAMES: Renames = Renames {
    roots: &["fmt", "std::fmt", "::std::fmt", "core::fmt", "::core::fmt"],
    names: &[("Write", "FmtWrite")],
    star: &["Write"],
    result_err: None,
};

// `std::task` / `std::future`: the task prelude's (hand-written futures).
const TASK_RENAMES: Renames = Renames {
    roots: &[
        "task",
        "std::task",
        "::std::task",
        "core::task",
        "::core::task",
        "futures::task",
    ],
    names: &[
        ("Poll", "TaskPoll"),
        ("Context", "TaskContext"),
        ("Waker", "TaskWaker"),
    ],
    star: &["Poll", "Context", "Waker"],
    result_err: None,
};

// `std::alloc::Global` (and `System`): the marker prelude's `AllocGlobal`; an `A: Allocator`
// parameter is accepted and ignored.
const ALLOC_RENAMES: Renames = Renames {
    roots: &[
        "alloc",
        "std::alloc",
        "::std::alloc",
        "core::alloc",
        "alloc::alloc",
    ],
    names: &[("Global", "AllocGlobal"), ("System", "AllocGlobal")],
    star: &["Global", "System"],
    result_err: None,
};

// `std::fs` and `std::process::Command`: the os prelude's (every operation fails).
const FS_RENAMES: Renames = Renames {
    roots: &["fs", "std::fs", "::std::fs"],
    names: &[
        ("File", "FsFile"),
        ("OpenOptions", "FsOpenOptions"),
        ("Metadata", "FsMetadata"),
        ("DirEntry", "FsDirEntry"),
        ("read_to_string", "__fs_read_to_string"),
        ("read", "__fs_read"),
        ("metadata", "__fs_metadata"),
        ("read_dir", "__fs_read_dir"),
        ("exists", "__fs_exists"),
        ("write", "__fs_write"),
        ("create_dir", "__fs_create_dir"),
        ("create_dir_all", "__fs_create_dir_all"),
        ("remove_file", "__fs_remove_file"),
        ("remove_dir", "__fs_remove_dir"),
        ("remove_dir_all", "__fs_remove_dir_all"),
        ("copy", "__fs_copy"),
        ("rename", "__fs_rename"),
    ],
    star: &["File", "OpenOptions", "Metadata", "DirEntry"],
    result_err: None,
};

// `std::net`'s addresses: the net prelude's (parsing and formatting).
const NET_RENAMES: Renames = Renames {
    roots: &["net", "std::net", "::std::net", "core::net", "::core::net"],
    names: &[
        ("IpAddr", "NetIpAddr"),
        ("Ipv4Addr", "NetIpv4Addr"),
        ("Ipv6Addr", "NetIpv6Addr"),
        ("SocketAddr", "NetSocketAddr"),
        ("SocketAddrV4", "NetSocketAddrV4"),
        ("SocketAddrV6", "NetSocketAddrV6"),
        ("AddrParseError", "NetAddrParseError"),
    ],
    star: &[
        "IpAddr",
        "Ipv4Addr",
        "Ipv6Addr",
        "SocketAddr",
        "SocketAddrV4",
        "SocketAddrV6",
        "AddrParseError",
    ],
    result_err: None,
};

// `std::sync::atomic::Ordering`: the lock prelude's `AtomicOrdering` (`std::cmp::Ordering` keeps
// its name).
const ATOMIC_RENAMES: Renames = Renames {
    roots: &[
        "atomic",
        "std::sync::atomic",
        "::std::sync::atomic",
        "core::sync::atomic",
        "::core::sync::atomic",
        "sync::atomic",
    ],
    names: &[("Ordering", "AtomicOrdering"), ("fence", "__atomic_fence"), ("compiler_fence", "__atomic_fence")],
    star: &[],
    result_err: None,
};

// `std::sync::mpsc`: the lock prelude's channel (tokio's `mpsc` is the tokio prelude's, below).
const MPSC_RENAMES: Renames = Renames {
    roots: &["mpsc", "std::sync::mpsc", "::std::sync::mpsc", "sync::mpsc"],
    names: &[
        ("Sender", "MpscSender"),
        ("SyncSender", "MpscSender"),
        ("Receiver", "MpscReceiver"),
        ("channel", "__mpsc_channel"),
        ("sync_channel", "__mpsc_sync_channel"),
        ("SendError", "MpscSendError"),
        ("TrySendError", "MpscTrySendError"),
        ("RecvError", "MpscRecvError"),
        ("TryRecvError", "MpscTryRecvError"),
        ("RecvTimeoutError", "MpscRecvTimeoutError"),
    ],
    star: &[],
    result_err: None,
};

// `tokio::sync::mpsc`: the tokio prelude's channel. (A file that names `tokio::sync::mpsc` gets
// this one for `mpsc::..`, else std's; see `renamed_crates`.)
const TOKIO_MPSC_RENAMES: Renames = Renames {
    roots: &[
        "mpsc",
        "tokio::sync::mpsc",
        "mpsc::error",
        "tokio::sync::mpsc::error",
    ],
    names: &[
        ("Sender", "TokioSender"),
        ("UnboundedSender", "TokioUnboundedSender"),
        ("Receiver", "TokioReceiver"),
        ("UnboundedReceiver", "TokioReceiver"),
        ("channel", "__tokio_channel"),
        ("unbounded_channel", "__tokio_unbounded_channel"),
        ("SendError", "TokioSendError"),
        ("TrySendError", "TokioTrySendError"),
        ("TryRecvError", "TokioTryRecvError"),
    ],
    star: &[],
    result_err: None,
};

// `std::thread::LocalKey` (what `thread_local!` declares).
const THREAD_RENAMES: Renames = Renames {
    roots: &["thread", "std::thread", "::std::thread"],
    names: &[("LocalKey", "LocalKey"), ("AccessError", "AccessError")],
    star: &[],
    result_err: None,
};

const PROCESS_RENAMES: Renames = Renames {
    roots: &["process", "std::process", "::std::process"],
    names: &[
        ("Command", "ProcCommand"),
        ("Stdio", "ProcStdio"),
        ("ExitStatus", "ProcExitStatus"),
        ("Output", "ProcOutput"),
        ("Child", "ProcChild"),
        ("ChildStdin", "ProcChildStdin"),
        ("ChildStdout", "ProcChildStdout"),
        ("ChildStderr", "ProcChildStderr"),
    ],
    star: &[
        "Command",
        "Stdio",
        "ExitStatus",
        "Output",
        "Child",
        "ChildStdin",
        "ChildStdout",
        "ChildStderr",
    ],
    result_err: None,
};

const FUTURE_RENAMES: Renames = Renames {
    roots: &[
        "future",
        "std::future",
        "::std::future",
        "core::future",
        "::core::future",
        "futures::future",
    ],
    names: &[
        ("ready", "__future_ready"),
        ("pending", "__future_pending"),
        ("poll_fn", "__future_poll_fn"),
        ("Ready", "FutureReady"),
        ("Pending", "FuturePending"),
        ("PollFn", "FuturePollFn"),
    ],
    star: &[],
    result_err: None,
};

const LOG_RENAMES: Renames = Renames {
    roots: &["log"],
    names: &[
        ("Level", "LogLevel"),
        ("LevelFilter", "LogLevelFilter"),
        ("set_max_level", "__log_set_max_level"),
        ("max_level", "__log_max_level"),
    ],
    star: &["Level", "LevelFilter"],
    result_err: None,
};

const ENV_LOGGER_RENAMES: Renames = Renames {
    roots: &["env_logger", "simple_logger"],
    names: &[
        ("init", "__log_env_init"),
        ("try_init", "__log_env_try_init"),
        ("Builder", "LogBuilder"),
        ("builder", "__log_builder"),
        ("Env", "LogEnv"),
        ("init_with_level", "__log_init_with_level"),
        ("SimpleLogger", "LogBuilder"),
    ],
    star: &[],
    result_err: None,
};

const NUM_RENAMES: Renames = Renames {
    roots: &[
        "num",
        "num::integer",
        "num::traits",
        "num_integer",
        "num_traits",
        "num_traits::identities",
        "num_traits::cast",
        "num_traits::sign",
        "num_traits::bounds",
    ],
    names: &[
        ("Integer", "NumInteger"),
        ("gcd", "__num_gcd"),
        ("lcm", "__num_lcm"),
        ("Zero", "NumZero"),
        ("One", "NumOne"),
        ("zero", "__num_zero"),
        ("one", "__num_one"),
        ("Signed", "NumSigned"),
        ("abs", "__num_abs"),
        ("signum", "__num_signum"),
        ("pow", "__num_pow"),
        ("clamp", "__num_clamp"),
        ("ToPrimitive", "NumToPrimitive"),
        ("FromPrimitive", "NumFromPrimitive"),
        ("Bounded", "NumBounded"),
    ],
    star: &[
        "Integer",
        "Zero",
        "One",
        "Signed",
        "ToPrimitive",
        "FromPrimitive",
        "Bounded",
    ],
    result_err: None,
};

const TOKIO_RENAMES: Renames = Renames {
    roots: &[
        "tokio",
        "tokio::time",
        "tokio::task",
        "tokio::sync",
        "tokio::time::error",
        "futures",
        "futures::future",
        "futures_util::future",
    ],
    names: &[
        ("sleep", "__tokio_sleep"),
        ("yield_now", "__tokio_yield_now"),
        ("JoinHandle", "TokioJoinHandle"),
        ("JoinError", "TokioJoinError"),
        ("join_all", "js::Promise::all"),
        ("Duration", "Duration"),
        ("Instant", "Instant"),
        ("Mutex", "TokioMutex"),
        ("RwLock", "TokioRwLock"),
        ("TryLockError", "TokioTryLockError"),
        ("timeout", "__tokio_timeout"),
        ("Elapsed", "TokioElapsed"),
    ],
    star: &[],
    result_err: None,
};

/// `rand::distributions` (rand 0.8) and `rand_distr`.
const RAND_DISTR_RENAMES: Renames = Renames {
    roots: &[
        "rand::distributions",
        "rand::distributions::uniform",
        "rand::distributions::weighted",
        "rand_distr",
        "rand_distr::uniform",
    ],
    names: &[
        ("Distribution", "RandDistribution"),
        ("DistString", "RandDistribution"),
        ("Standard", "RandStandard"),
        ("StandardNormal", "RandStandardNormal"),
        ("Exp1", "RandExp1"),
        ("Open01", "RandOpen01"),
        ("OpenClosed01", "RandOpenClosed01"),
        ("Uniform", "RandUniform"),
        ("SampleUniform", "RandSampleUniform"),
        ("WeightedIndex", "RandWeightedIndex"),
        ("WeightedError", "RandWeightedError"),
        ("Bernoulli", "RandBernoulli"),
        ("BernoulliError", "RandBernoulliError"),
        ("Alphanumeric", "RandAlphanumeric"),
        ("Normal", "RandNormal"),
        ("NormalError", "RandNormalError"),
        ("LogNormal", "RandLogNormal"),
        ("Exp", "RandExp"),
        ("ExpError", "RandExpError"),
        ("Poisson", "RandPoisson"),
        ("PoissonError", "RandPoissonError"),
        ("Binomial", "RandBinomial"),
        ("BinomialError", "RandBinomialError"),
        ("Gamma", "RandGamma"),
        ("GammaError", "RandGammaError"),
        ("Beta", "RandBeta"),
        ("BetaError", "RandBetaError"),
        ("ChiSquared", "RandChiSquared"),
        ("ChiSquaredError", "RandChiSquaredError"),
        ("StudentT", "RandStudentT"),
        ("Triangular", "RandTriangular"),
        ("TriangularError", "RandTriangularError"),
        ("Pareto", "RandPareto"),
        ("ParetoError", "RandParetoError"),
        ("Cauchy", "RandCauchy"),
        ("CauchyError", "RandCauchyError"),
        ("Weibull", "RandWeibull"),
        ("WeibullError", "RandWeibullError"),
        ("Geometric", "RandGeometric"),
        ("GeoError", "RandGeoError"),
        ("Float", "RandFloat"),
    ],
    star: &[
        "Distribution",
        "Standard",
        "StandardNormal",
        "Exp1",
        "Open01",
        "OpenClosed01",
        "Uniform",
        "WeightedIndex",
        "WeightedError",
        "Bernoulli",
        "BernoulliError",
        "Alphanumeric",
        "Normal",
        "NormalError",
        "LogNormal",
        "Exp",
        "ExpError",
        "Poisson",
        "PoissonError",
        "Binomial",
        "BinomialError",
        "Gamma",
        "GammaError",
        "Beta",
        "BetaError",
        "ChiSquared",
        "ChiSquaredError",
        "StudentT",
        "Triangular",
        "TriangularError",
        "Pareto",
        "ParetoError",
        "Cauchy",
        "CauchyError",
        "Weibull",
        "WeibullError",
        "Geometric",
        "GeoError",
    ],
    result_err: None,
};

const CHRONO_RENAMES: Renames = Renames {
    roots: &[
        "chrono",
        "::chrono",
        "chrono::prelude",
        "chrono::offset",
        "chrono::naive",
        "chrono::format",
    ],
    names: &[
        ("NaiveDate", "ChronoNaiveDate"),
        ("NaiveTime", "ChronoNaiveTime"),
        ("NaiveDateTime", "ChronoNaiveDateTime"),
        ("NaiveWeek", "ChronoNaiveWeek"),
        ("DateTime", "ChronoDateTime"),
        ("Utc", "ChronoUtc"),
        ("Local", "ChronoLocal"),
        ("FixedOffset", "ChronoFixedOffset"),
        ("TimeZone", "ChronoTimeZone"),
        ("Duration", "ChronoTimeDelta"),
        ("TimeDelta", "ChronoTimeDelta"),
        ("Days", "ChronoDays"),
        ("Months", "ChronoMonths"),
        ("Weekday", "ChronoWeekday"),
        ("Month", "ChronoMonth"),
        ("Datelike", "ChronoDatelike"),
        ("Timelike", "ChronoTimelike"),
        ("ParseError", "ChronoParseError"),
        ("ParseErrorKind", "ChronoParseErrorKind"),
        ("ParseWeekdayError", "ChronoParseWeekdayError"),
        ("OutOfRange", "ChronoOutOfRange"),
        ("OutOfRangeError", "ChronoOutOfRangeError"),
        ("LocalResult", "ChronoLocalResult"),
        ("MappedLocalTime", "ChronoLocalResult"),
        ("SecondsFormat", "ChronoSecondsFormat"),
        ("IsoWeek", "ChronoIsoWeek"),
    ],
    star: &[
        "NaiveDate",
        "NaiveTime",
        "NaiveDateTime",
        "DateTime",
        "Utc",
        "Local",
        "FixedOffset",
        "TimeZone",
        "Weekday",
        "Month",
        "Datelike",
        "Timelike",
        "LocalResult",
        "MappedLocalTime",
        "SecondsFormat",
        "Days",
        "Months",
        "IsoWeek",
    ],
    result_err: None,
};

const CLAP_RENAMES: Renames = Renames {
    roots: &["clap", "clap::builder", "clap::error", "clap::parser"],
    names: &[
        ("Command", "ClapCommand"),
        ("Arg", "ClapArg"),
        ("ArgAction", "ClapAction"),
        ("ArgMatches", "ClapMatches"),
        ("Error", "ClapError"),
        ("ErrorKind", "ClapErrorKind"),
        ("PossibleValue", "ClapPossibleValue"),
    ],
    star: &["Command", "Arg", "ArgAction", "ArgMatches", "Error"],
    result_err: None,
};

/// `std::collections::hash_map::Entry` (and `btree_map`'s): the map entry prelude's.
/// The bundled `react` crate (the react prelude; `jsx!` is rewritten on its own).
const REACT_RENAMES: Renames = Renames {
    roots: &["react"],
    names: &[
        ("Node", "ReactNode"),
        ("SetState", "ReactSetState"),
        ("Ref", "ReactRef"),
        ("h", "__rx_h"),
        ("use_state", "__rx_use_state"),
        ("use_effect", "__rx_use_effect"),
        ("use_ref", "__rx_use_ref"),
        ("use_memo", "__rx_use_memo"),
        ("use_callback", "__rx_use_callback"),
        ("use_reducer", "__rx_use_reducer"),
        ("create_context", "__rx_create_context"),
        ("use_context", "__rx_use_context"),
        ("Context", "ReactContext"),
        ("Dispatch", "ReactDispatch"),
        ("IntoNode", "ReactIntoNode"),
    ],
    star: &[
        "Node",
        "SetState",
        "Ref",
        "h",
        "use_state",
        "use_effect",
        "use_ref",
        "use_memo",
        "use_callback",
        "use_reducer",
        "create_context",
        "use_context",
        "Context",
        "Dispatch",
        "IntoNode",
    ],
    result_err: None,
};

const ENTRY_RENAMES: Renames = Renames {
    roots: &[
        "std::collections::hash_map",
        "std::collections::btree_map",
        "collections::hash_map",
        "collections::btree_map",
        "hash_map",
        "btree_map",
    ],
    names: &[
        ("Entry", "StdMapEntry"),
        ("OccupiedEntry", "StdOccupiedEntry"),
        ("VacantEntry", "StdVacantEntry"),
    ],
    star: &["Entry", "OccupiedEntry", "VacantEntry"],
    result_err: None,
};

/// `chrono_tz::Tz` and its error (its zones are `chrono_tz_zones`').
const CHRONO_TZ_RENAMES: Renames = Renames {
    roots: &["chrono_tz"],
    names: &[
        ("Tz", "ChronoTz"),
        ("ParseError", "ChronoTzParseError"),
        ("UTC", "ChronoTz::UTC"),
    ],
    star: &["Tz"],
    result_err: None,
};

/// `Distribution` from `use rand::prelude::*;`.
const RAND_PRELUDE_RENAMES: Renames = Renames {
    roots: &["rand::prelude"],
    names: &[("Distribution", "RandDistribution")],
    star: &["Distribution"],
    result_err: None,
};

/// rand 0.9's `rand::distr`: `Uniform::new` gives a `Result`, `Standard` is `StandardUniform`.
const RAND_DISTR09_RENAMES: Renames = Renames {
    roots: &[
        "rand::distr",
        "rand::distr::uniform",
        "rand::distr::weighted",
    ],
    names: &[
        ("Distribution", "RandDistribution"),
        ("SampleString", "RandDistribution"),
        ("StandardUniform", "RandStandard"),
        ("Open01", "RandOpen01"),
        ("OpenClosed01", "RandOpenClosed01"),
        ("Uniform", "RandUniform09"),
        ("SampleUniform", "RandSampleUniform"),
        ("WeightedIndex", "RandWeightedIndex"),
        ("Bernoulli", "RandBernoulli"),
        ("BernoulliError", "RandBernoulliError"),
        ("Alphanumeric", "RandAlphanumeric"),
    ],
    star: &[
        "Distribution",
        "StandardUniform",
        "Open01",
        "OpenClosed01",
        "Uniform",
        "WeightedIndex",
        "Bernoulli",
        "BernoulliError",
        "Alphanumeric",
    ],
    result_err: None,
};

/// `ordered_float`'s names are the prelude's own (this resolves `ordered_float::OrderedFloat` and
/// `use ordered_float::OrderedFloat as OF;`).
const ORDERED_FLOAT_RENAMES: Renames = Renames {
    roots: &["ordered_float", "::ordered_float"],
    names: &[
        ("OrderedFloat", "OrderedFloat"),
        ("NotNan", "NotNan"),
        ("FloatIsNan", "FloatIsNan"),
    ],
    star: &["OrderedFloat", "NotNan", "FloatIsNan"],
    result_err: None,
};

/// The types `itertools` methods return, as the itertools prelude's (`itertools::MinMaxResult`,
/// `use itertools::FoldWhile::{Continue, Done};`).
const ITERTOOLS_RENAMES: Renames = Renames {
    roots: &["itertools", "::itertools"],
    names: &[
        ("MinMaxResult", "MinMaxResult"),
        ("FoldWhile", "FoldWhile"),
        ("ExactlyOneError", "ExactlyOneError"),
    ],
    star: &["MinMaxResult", "FoldWhile", "ExactlyOneError"],
    result_err: None,
};

/// `petgraph`'s types as the petgraph prelude's (its functions are `petgraph_edits`').
const PETGRAPH_RENAMES: Renames = Renames {
    roots: &[
        "petgraph",
        "::petgraph",
        "petgraph::graph",
        "petgraph::graphmap",
        "petgraph::visit",
        "petgraph::prelude",
        "petgraph::algo",
        "petgraph::algo::bellman_ford",
        "petgraph::data",
    ],
    names: PETGRAPH_NAMES,
    star: &[
        "Graph",
        "DiGraph",
        "UnGraph",
        "GraphMap",
        "DiGraphMap",
        "UnGraphMap",
        "NodeIndex",
        "EdgeIndex",
        "EdgeReference",
        "Directed",
        "Undirected",
        "Direction",
        "Outgoing",
        "Incoming",
        "Bfs",
        "Dfs",
        "DfsPostOrder",
    ],
    result_err: None,
};

/// petgraph's names and the prelude's.
pub(crate) const PETGRAPH_NAMES: &[(&str, &str)] = &[
    ("Graph", "PgGraph"),
    ("DiGraph", "PgGraph"),
    ("UnGraph", "PgGraph"),
    ("GraphMap", "PgGraphMap"),
    ("DiGraphMap", "PgGraphMap"),
    ("UnGraphMap", "PgGraphMap"),
    ("NodeIndex", "PgNodeIndex"),
    ("EdgeIndex", "PgEdgeIndex"),
    ("EdgeReference", "PgEdgeReference"),
    ("Directed", "PgDirected"),
    ("Undirected", "PgUndirected"),
    ("Direction", "PgDirection"),
    ("Outgoing", "PgDirection::Outgoing"),
    ("Incoming", "PgDirection::Incoming"),
    ("Bfs", "PgBfs"),
    ("Dfs", "PgDfs"),
    ("DfsPostOrder", "PgDfsPostOrder"),
    ("Cycle", "PgCycle"),
    ("NegativeCycle", "PgNegativeCycle"),
    ("Paths", "PgPaths"),
    ("DfsSpace", "PgDfsSpace"),
    ("Element", "PgElement"),
    ("node_index", "__pg_node_index"),
    ("edge_index", "__pg_edge_index"),
];

/// `petgraph::algo`'s functions: methods of the graph here (`(&g).__pg_dijkstra(..)`).
const PETGRAPH_ALGOS: &[&str] = &[
    "dijkstra",
    "astar",
    "bellman_ford",
    "toposort",
    "is_cyclic_directed",
    "is_cyclic_undirected",
    "connected_components",
    "kosaraju_scc",
    "tarjan_scc",
    "has_path_connecting",
    "min_spanning_tree",
];

const PETGRAPH_ALGO_IMPORTS: Renames = Renames {
    roots: &["petgraph::algo", "::petgraph::algo"],
    names: &[],
    star: PETGRAPH_ALGOS,
    result_err: None,
};

const PETGRAPH_ROOT_IMPORTS: Renames = Renames {
    roots: &["petgraph", "::petgraph"],
    names: &[],
    star: &[],
    result_err: None,
};

const PETGRAPH_DIRECTION_IMPORTS: Renames = Renames {
    roots: &["petgraph::Direction", "::petgraph::Direction"],
    names: &[],
    star: &["Outgoing", "Incoming"],
    result_err: None,
};

/// Every renamed crate and std module, in the order their `use`s are resolved.
const ALL_RENAMES: &[&Renames] = &[
        &IO_RENAMES,
        &FMT_RENAMES,
        &TASK_RENAMES,
        &FUTURE_RENAMES,
        &ALLOC_RENAMES,
        &FS_RENAMES,
        &PROCESS_RENAMES,
        &ATOMIC_RENAMES,
        &MPSC_RENAMES,
        &TOKIO_MPSC_RENAMES,
        &THREAD_RENAMES,
        &NET_RENAMES,
        &RUSTC_HASH_RENAMES,
        &LOG_RENAMES,
        &ENV_LOGGER_RENAMES,
        &NUM_RENAMES,
        &TOKIO_RENAMES,
        &RAND_DISTR_RENAMES,
        &RAND_DISTR09_RENAMES,
        &RAND_PRELUDE_RENAMES,
        &CHRONO_RENAMES,
        &CLAP_RENAMES,
        &ENTRY_RENAMES,
        &CHRONO_TZ_RENAMES,
        &REACT_RENAMES,
        &ORDERED_FLOAT_RENAMES,
        &PETGRAPH_RENAMES,
        &ITERTOOLS_RENAMES,
];

/// The crate's own name for a prelude's internal one (`PgGraph` is petgraph's `Graph`), as the
/// language service shows it.
pub(crate) fn user_name(internal: &str) -> Option<&'static str> {
    // (Only the prelude's own spellings: `__pg_node_index`, `PgGraph`, `ChronoNaiveDate`. Some
    // tables map a crate's name onto a std one, `par_iter` to `iter`, which stays as it is.)
    let prefixed = internal.starts_with("__")
        || internal
            .strip_prefix(|c: char| c.is_ascii_uppercase())
            .map(|rest| rest.trim_start_matches(|c: char| c.is_ascii_lowercase()))
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_uppercase()));
    if !prefixed {
        return None;
    }
    ALL_RENAMES
        .iter()
        .flat_map(|r| r.names.iter())
        .find(|(_, to)| *to == internal)
        .map(|(from, _)| *from)
}

/// The crates of `text` that are renamed, and the names its `use`s bring in from each.
fn renamed_crates(text: &str) -> Vec<&'static Renames> {
    let mut out: Vec<&'static Renames> = Vec::new();
    // (A use of `std::io`'s items names `io::` somewhere, `std::io::Error`, `use std::{io::Write}`,
    // or is `use std::io;` itself; `std::` alone says nothing.)
    let has = |root: &str| {
        let root = root.trim_start_matches("::");
        let first = root.split("::").next().unwrap_or(root);
        let last = root.rsplit("::").next().unwrap_or(root);
        (text.contains(&format!("{last}::")) || text.contains(&format!("use {root}")))
            && !text.contains(&format!("mod {first}"))
    };
    // (`mpsc::channel` is tokio's in a file that imports tokio's `mpsc`.)
    let tokio_mpsc = text.contains("tokio::sync::mpsc")
        || (text.contains("tokio::sync::{") && has_mpsc_in_tokio_use(text));
    for r in ALL_RENAMES.iter().copied() {
        // (Consts have no one address: told apart by a root.)
        if r.roots.contains(&"std::sync::mpsc") && tokio_mpsc
            || r.roots.contains(&"tokio::sync::mpsc") && !tokio_mpsc
        {
            continue;
        }
        if r.roots.iter().any(|root| has(root)) {
            out.push(r);
        }
    }
    out
}

/// Whether a `use tokio::sync::{..}` brings in `mpsc`.
fn has_mpsc_in_tokio_use(text: &str) -> bool {
    text.match_indices("tokio::sync::{").any(|(at, _)| {
        let rest = &text[at..];
        let end = rest.find('}').unwrap_or(rest.len());
        crate::stdtime::has_word(&rest[..end], "mpsc")
    })
}

/// Whether `text` uses a renamed crate.
pub(crate) fn uses_renamed(text: &str) -> bool {
    !renamed_crates(text).is_empty()
}

/// The std name a crate's alias is rewritten to (`FxHashMap` → `HashMap`), for the std-import
/// check: `use rustc_hash::FxHashMap;` imports `HashMap`.
pub(crate) fn renamed_target(alias: &str) -> Option<&'static str> {
    ALL_RENAMES
        .iter()
        .flat_map(|r| r.names.iter())
        .find(|(from, _)| *from == alias)
        .map(|(_, to)| *to)
}

/// The names `use krate::{..}` brings in: alias -> the crate's name.
fn renamed_imports(file: &SourceFile, r: &Renames) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        for (path, rename, star) in crate::typeck::use_leaves(&u) {
            let joined = path.join("::");
            for root in r.roots {
                let root = root.trim_start_matches("::");
                if star && joined == root {
                    // (A glob import is shadowed by the file's own `enum Month`.)
                    let text = file.syntax().text().to_string();
                    let declared = |n: &str| {
                        ["struct", "enum", "type", "trait", "fn"]
                            .iter()
                            .any(|kw| text.contains(&format!("{kw} {n}")))
                    };
                    for n in r.star {
                        if !declared(n) {
                            out.entry(n.to_string()).or_insert_with(|| n.to_string());
                        }
                    }
                } else if !star
                    && let Some(item) = joined.strip_prefix(root).and_then(|x| x.strip_prefix("::"))
                    && !item.contains("::")
                {
                    let alias = rename.clone().unwrap_or_else(|| item.to_string());
                    out.insert(alias, item.to_string());
                }
            }
        }
    }
    out
}

/// The crates' names as the preludes': `io::Error` / `std::io::Error` (and `Error` after `use
/// std::io::Error;`) is `IoError`, `impl Write for W` is `impl IoWrite for W`, `io::Result<T>` is
/// `Result<T, IoError>`, `log::Level` is `LogLevel`, ..
fn rename_crate_items(file: &SourceFile, text: &str, edits: &mut Vec<(TextRange, String)>) {
    for r in renamed_crates(text) {
        let imports = renamed_imports(file, r);
        // (In other macros' tokens too: `println!("{:?}", ErrorKind::Other)`.)
        for tt in file.syntax().descendants().filter_map(ast::TokenTree::cast) {
            if tt
                .syntax()
                .parent()
                .is_some_and(|p| ast::TokenTree::can_cast(p.kind()))
            {
                continue;
            }
            rename_tokens(&tt, r, &imports, edits);
        }
        for path in file.syntax().descendants().filter_map(ast::Path::cast) {
            if path
                .syntax()
                .ancestors()
                .any(|a| ast::Use::can_cast(a.kind()))
            {
                continue;
            }
            let Some(seg) = path.segment() else { continue };
            let Some(name) = seg.name_ref() else { continue };
            let item = match path.qualifier() {
                Some(q) => {
                    let qt = q.syntax().text().to_string();
                    if !r.roots.contains(&qt.as_str()) {
                        continue;
                    }
                    name.text().to_string()
                }
                None => match imports.get(name.text()) {
                    Some(item) => item.clone(),
                    None => continue,
                },
            };
            // (From the path's start to its name: generic arguments stay.)
            let r2 = TextRange::new(
                path.syntax().text_range().start(),
                name.syntax().text_range().end(),
            );
            if item == "Result"
                && let Some(err) = r.result_err
            {
                edits.push((r2, "Result".into()));
                if let Some(gl) = seg.generic_arg_list()
                    && let Some(close) = gl.r_angle_token()
                {
                    edits.push((
                        TextRange::empty(close.text_range().start()),
                        format!(", {err}"),
                    ));
                }
                continue;
            }
            if let Some((_, to)) = r.names.iter().find(|(n, _)| *n == item) {
                edits.push((r2, to.to_string()));
            }
        }
    }
}

/// `krate::X` and the names `use krate::{..}` brought in, among a macro's tokens.
fn rename_tokens(
    tt: &ast::TokenTree,
    r: &Renames,
    imports: &std::collections::HashMap<String, String>,
    edits: &mut Vec<(TextRange, String)>,
) {
    use ra_ap_syntax::SyntaxKind::{COLON, COLON2, DOT, IDENT, WHITESPACE};
    let toks: Vec<_> = tt
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != WHITESPACE)
        .collect();
    let is_sep = |k| matches!(k, COLON | COLON2);
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        let after_path = i > 0 && (is_sep(toks[i - 1].kind()) || toks[i - 1].kind() == DOT);
        if t.kind() != IDENT || after_path {
            i += 1;
            continue;
        }
        // The longest `a::b::X` whose `a::b` is a root.
        let mut segs = vec![(t.text().to_string(), i)];
        let mut k = i + 1;
        loop {
            let mut s = k;
            while toks.get(s).is_some_and(|c| is_sep(c.kind())) {
                s += 1;
            }
            if s == k || !toks.get(s).is_some_and(|c| c.kind() == IDENT) {
                break;
            }
            segs.push((toks[s].text().to_string(), s));
            k = s + 1;
        }
        let mut done = false;
        for n in (2..=segs.len()).rev() {
            let root: Vec<&str> = segs[..n - 1].iter().map(|(s, _)| s.as_str()).collect();
            let (item, at) = &segs[n - 1];
            if r.roots.contains(&root.join("::").as_str())
                && let Some((_, to)) = r.names.iter().find(|(x, _)| x == item)
            {
                edits.push((t.text_range().cover(toks[*at].text_range()), to.to_string()));
                i = at + 1;
                done = true;
                break;
            }
        }
        if done {
            continue;
        }
        if let Some(item) = imports.get(&t.text().to_string())
            && let Some((_, to)) = r.names.iter().find(|(n, _)| n == item)
        {
            edits.push((t.text_range(), to.to_string()));
        }
        i += 1;
    }
}

/// `tokio::spawn(fut)` (after the rename, `__tokio_spawn(fut)`): an `async` block that awaits it,
/// so the task starts at once and its handle gives `Ok(value)`. `#[tokio::main]` goes away.
fn tokio_rewrite(node: &ra_ap_syntax::SyntaxNode, edits: &mut Vec<(TextRange, String)>) {
    if let Some(a) = ast::Attr::cast(node.clone())
        && a.path().is_some_and(|p| {
            let t = p.syntax().text().to_string();
            t == "tokio::main" || t == "tokio::test"
        })
    {
        edits.push((a.syntax().text_range(), String::new()));
        return;
    }
    let Some(call) = ast::CallExpr::cast(node.clone()) else {
        return;
    };
    let Some(ast::Expr::PathExpr(p)) = call.expr() else {
        return;
    };
    let pt = p.syntax().text().to_string();
    if !matches!(pt.as_str(), "tokio::spawn" | "tokio::task::spawn") {
        return;
    }
    let Some(arg) = call.arg_list().and_then(|l| l.args().next()) else {
        return;
    };
    // (Its argument may hold its own rewrites: inserts around it keep them.)
    let r = arg.syntax().text_range();
    edits.push((
        TextRange::new(call.syntax().text_range().start(), r.start()),
        "async { Ok::<_, TokioJoinError>((".into(),
    ));
    edits.push((
        TextRange::new(r.end(), call.syntax().text_range().end()),
        ").await) }".into(),
    ));
}

/// One branch of a `select!`: `pattern = future, if precondition => handler`.
struct SelectBranch {
    pat: String,
    fut: String,
    cond: Option<String>,
    handler: String,
}

/// Where the top-level `=` of a `select!` branch is in `s` (not `==`, `=>`, `<=`, `..=`, ..),
/// and the end of the handler that starts at `from`: these scan outside brackets and strings.
fn select_scan(s: &str, from: usize, want: &str) -> Option<usize> {
    let b = s.as_bytes();
    let (mut depth, mut i) = (0i32, from);
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                let next = b.get(i + 1).copied().unwrap_or(b' ');
                let hit = match want {
                    "=" => b[i] == b'=' && !b"=!<>.".contains(&prev) && !b"=>".contains(&next),
                    "=>" => b[i] == b'=' && next == b'>',
                    _ => b[i] == b',',
                };
                if hit {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The branches of a `select!` body, its `else` handler, and whether it's `biased;`.
fn select_branches(body: &str) -> Option<(Vec<SelectBranch>, Option<String>)> {
    let mut rest = body.trim_start();
    if let Some(r) = rest.strip_prefix("biased") {
        rest = r.trim_start().strip_prefix(';')?.trim_start();
    }
    let mut branches = Vec::new();
    let mut otherwise = None;
    // The handler after `=>`, and what follows it.
    let handler = |s: &str| -> Option<(String, String)> {
        let s = s.trim_start();
        if s.starts_with('{') {
            let (inside, after) = braced(s)?;
            let after = after.trim_start();
            Some((
                format!("{{{inside}}}"),
                after.strip_prefix(',').unwrap_or(after).to_string(),
            ))
        } else {
            match select_scan(s, 0, ",") {
                Some(at) => Some((s[..at].to_string(), s[at + 1..].to_string())),
                None => Some((s.to_string(), String::new())),
            }
        }
    };
    let mut owned;
    while !rest.trim().is_empty() {
        if let Some(r) = rest.strip_prefix("else") {
            let r = r.trim_start().strip_prefix("=>")?;
            let (h, after) = handler(r)?;
            otherwise = Some(h);
            owned = after;
            rest = owned.trim_start();
            continue;
        }
        let eq = select_scan(rest, 0, "=")?;
        let arrow = select_scan(rest, eq + 1, "=>")?;
        let pat = rest[..eq].trim().to_string();
        let mid = rest[eq + 1..arrow].trim();
        let (fut, cond) = match select_scan(mid, 0, ",") {
            Some(c) => {
                let cond = mid[c + 1..].trim().strip_prefix("if")?.trim().to_string();
                (mid[..c].trim().to_string(), Some(cond))
            }
            None => (mid.to_string(), None),
        };
        let (h, after) = handler(&rest[arrow + 2..])?;
        branches.push(SelectBranch {
            pat,
            fut,
            cond,
            handler: h,
        });
        owned = after;
        rest = owned.trim_start();
    }
    (!branches.is_empty()).then_some((branches, otherwise))
}

/// `tokio::select! { pat = fut => handler, .. }`: a future polling each branch's future in turn
/// (as `biased;`; tokio picks a random order) until one is ready with a value its pattern takes.
/// A ready value the pattern doesn't take disables its branch, as does a false precondition;
/// with all disabled, the `else` handler runs (or it panics, as tokio does). The futures that
/// lose are dropped unfinished, so a channel's `recv()` (a hand-written future) loses nothing;
/// an `async fn`'s promise keeps running in JS.
fn tokio_select(body: &str, arities: &mut std::collections::BTreeSet<usize>) -> Option<String> {
    let (branches, otherwise) = select_branches(body)?;
    let n = branches.len();
    arities.insert(n);
    let mut out = String::from("{\n");
    for (i, b) in branches.iter().enumerate() {
        out.push_str(&format!("let mut __sel_f{i} = Box::pin({});\n", b.fut));
    }
    let offs: Vec<String> = branches
        .iter()
        .map(|b| match &b.cond {
            Some(c) => format!("!({c})"),
            None => "false".into(),
        })
        .collect();
    out.push_str(&format!("let mut __sel_off = [{}];\n", offs.join(", ")));
    out.push_str("match __future_poll_fn(move |__sel_cx| {\n");
    for (i, b) in branches.iter().enumerate() {
        out.push_str(&format!(
            "if !__sel_off[{i}] {{ if let TaskPoll::Ready(__sel_v) = __sel_f{i}.as_mut().poll(__sel_cx) {{ if matches!(__sel_v, {pat}) {{ return TaskPoll::Ready(Some(__Select{n}::B{i}(__sel_v))); }} __sel_off[{i}] = true; }} }}\n",
            pat = b.pat
        ));
    }
    out.push_str(
        "if __sel_off.iter().all(|o| *o) { return TaskPoll::Ready(None); }\nTaskPoll::Pending\n}).await {\n",
    );
    for (i, b) in branches.iter().enumerate() {
        out.push_str(&format!(
            "Some(__Select{n}::B{i}({})) => {},\n",
            b.pat, b.handler
        ));
    }
    let none = otherwise
        .unwrap_or_else(|| "panic!(\"all branches are disabled and there is no else branch\")".into());
    out.push_str(&format!("None => {none},\n"));
    if branches.iter().any(|b| b.pat.trim() != "_" && !b.pat.trim().chars().all(|c| c.is_alphanumeric() || c == '_')) {
        out.push_str("_ => unreachable!(),\n");
    }
    out.push('}');
    out.push_str("\n}");
    Some(out)
}

/// `enum __Select2<T0, T1> { B0(T0), B1(T1) }`: which branch of a 2-branch `select!` won.
fn select_enum(n: usize) -> String {
    let params: Vec<String> = (0..n).map(|i| format!("T{i}")).collect();
    let variants: Vec<String> = (0..n).map(|i| format!("B{i}(T{i})")).collect();
    format!(
        "\nenum __Select{n}<{}> {{ {} }}\n",
        params.join(", "),
        variants.join(", ")
    )
}

// ---------------------------------------------------------------- chrono-tz

/// chrono-tz's zones: `chrono_tz::Europe::London`, `London` after `use chrono_tz::Europe::London;`,
/// `Europe::London` after `use chrono_tz::Europe;` and `Tz::Europe__London` are
/// `ChronoTz::__of_name("Europe/London")` (also among macros' tokens).
fn chrono_tz_zones(file: &SourceFile, edits: &mut Vec<(TextRange, String)>) {
    const NOT_ZONES: &[&str] = &[
        "Tz",
        "ParseError",
        "UTC",
        "OffsetName",
        "OffsetComponents",
        "TZ_VARIANTS",
        "IANA_TZDB_VERSION",
    ];
    // alias -> a zone ("Europe/London") or a region ("Europe")
    let mut zones: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut tz_alias = None;
    for u in file.syntax().descendants().filter_map(ast::Use::cast) {
        for (path, rename, star) in crate::typeck::use_leaves(&u) {
            if path.first().map(String::as_str) != Some("chrono_tz") || star || path.len() < 2 {
                continue;
            }
            let alias = rename
                .clone()
                .unwrap_or_else(|| path.last().cloned().unwrap_or_default());
            if path[1] == "Tz" {
                tz_alias = Some(alias);
            } else if !NOT_ZONES.contains(&path[1].as_str()) {
                zones.insert(alias, path[1..].join("/"));
            }
        }
    }
    let tz_alias = tz_alias.unwrap_or_else(|| "Tz".to_string());
    let zone_of = |segs: &[String]| -> Option<String> {
        match segs {
            [first, rest @ ..]
                if first == "chrono_tz"
                    && rest.len() >= 2
                    && !NOT_ZONES.contains(&rest[0].as_str()) =>
            {
                Some(rest.join("/"))
            }
            [t, v] if *t == tz_alias && v.contains("__") => Some(v.replace("__", "/")),
            [one] => zones.get(one).filter(|z| z.contains('/')).cloned(),
            [first, rest @ ..] => zones
                .get(first)
                .filter(|z| !z.contains('/'))
                .map(|region| format!("{region}/{}", rest.join("/"))),
            [] => None,
        }
    };
    let mk = |zone: &str| format!("ChronoTz::__of_name({zone:?})");
    for path in file.syntax().descendants().filter_map(ast::Path::cast) {
        if path
            .syntax()
            .parent()
            .is_some_and(|p| ast::Path::can_cast(p.kind()))
            || path
                .syntax()
                .ancestors()
                .any(|a| ast::Use::can_cast(a.kind()))
        {
            continue;
        }
        let segs: Vec<String> = path
            .segments()
            .filter_map(|s| s.name_ref().map(|n| n.text().to_string()))
            .collect();
        if let Some(zone) = zone_of(&segs) {
            edits.push((path.syntax().text_range(), mk(&zone)));
        }
    }
    // (In macros' tokens: `println!("{}", t.with_timezone(&London))`.)
    use ra_ap_syntax::SyntaxKind::{COLON, COLON2, DOT, IDENT, WHITESPACE};
    for tt in file.syntax().descendants().filter_map(ast::TokenTree::cast) {
        if tt
            .syntax()
            .parent()
            .is_some_and(|p| ast::TokenTree::can_cast(p.kind()))
        {
            continue;
        }
        let toks: Vec<_> = tt
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() != WHITESPACE)
            .collect();
        let mut i = 0;
        while i < toks.len() {
            let after = i > 0 && matches!(toks[i - 1].kind(), COLON | COLON2 | DOT);
            if toks[i].kind() != IDENT || after {
                i += 1;
                continue;
            }
            let mut segs = vec![toks[i].text().to_string()];
            let mut k = i + 1;
            let mut last = i;
            loop {
                let mut s = k;
                while toks
                    .get(s)
                    .is_some_and(|c| matches!(c.kind(), COLON | COLON2))
                {
                    s += 1;
                }
                if s == k || !toks.get(s).is_some_and(|c| c.kind() == IDENT) {
                    break;
                }
                segs.push(toks[s].text().to_string());
                last = s;
                k = s + 1;
            }
            if let Some(zone) = zone_of(&segs) {
                edits.push((
                    toks[i].text_range().cover(toks[last].text_range()),
                    mk(&zone),
                ));
            }
            i = last + 1;
        }
    }
}
