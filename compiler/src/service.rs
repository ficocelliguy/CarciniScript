//! Language service for editors: hover (the type under the cursor, with NS docs) and completions
//! after `.` (NS methods and fields, struct fields and methods, std methods). Works on code with
//! syntax errors, since that's what an editor has while you type.

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, TextSize,
    ast::{self, HasName},
};
use serde::Serialize;

use crate::bindings;
use crate::lineindex::LineIndex;
use crate::macros::EDITION;
use crate::ty::Ty;
use crate::typeck::{self, Resolver, Types};

/// Markdown for the hover popup, and the range it describes (1-based, UTF-16 columns).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub contents: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize)]
pub struct Completion {
    pub label: String,
    /// `method`, `field`, `function`, `variable`, `struct`, `enum`, `trait`, `constant` or `module`.
    pub kind: &'static str,
    /// A signature or type.
    pub detail: String,
    pub documentation: String,
}

/// Byte offset of a 1-based line and UTF-16 column.
fn offset_of(src: &str, line: u32, column: u32) -> usize {
    let mut offset = 0;
    for (i, l) in src.split_inclusive('\n').enumerate() {
        if i + 1 == line as usize {
            let mut units = 0;
            for (b, c) in l.char_indices() {
                if units + 1 >= column as usize {
                    return offset + b;
                }
                units += c.len_utf16();
            }
            return offset + l.trim_end_matches('\n').len();
        }
        offset += l.len();
    }
    src.len()
}

fn analyze(src: &str, filename: &str, resolver: Resolver) -> (SourceFile, Types) {
    let file = SourceFile::parse(src, EDITION).tree();
    let (types, _) = typeck::check(&file, src, filename, resolver);
    (file, types)
}

fn type_at(types: &Types, n: &SyntaxNode) -> Option<Ty> {
    let r = n.text_range();
    types
        .get(usize::from(r.start()), usize::from(r.end()))
        .cloned()
        .filter(|t| !matches!(t, Ty::Unknown | Ty::Var(_)))
}

fn signature(name: &str, params: &[(String, Ty)], ret: &Ty, is_async: bool) -> String {
    let ps: Vec<String> = params
        .iter()
        .map(|(n, t)| format!("{n}: {}", t.describe()))
        .collect();
    let ret = if *ret == Ty::Unit {
        String::new()
    } else {
        format!(" -> {}", ret.describe())
    };
    let a = if is_async { "async " } else { "" };
    format!("{a}fn {name}({}){ret}", ps.join(", "))
}

/// An NS method's parameters; optional ones (which may be left out) are marked `?`.
fn binding_params(m: &bindings::Method) -> Vec<(String, Ty)> {
    m.params
        .iter()
        .map(|p| {
            (
                if p.optional {
                    format!("{}?", p.name)
                } else {
                    p.name.clone()
                },
                p.ty.clone(),
            )
        })
        .collect()
}

fn code(s: &str) -> String {
    format!("```rust\n{s}\n```")
}

/// What to show for hovering at (line, column).
pub fn hover(
    src: &str,
    filename: &str,
    line: u32,
    column: u32,
    resolver: Resolver,
) -> Option<Hover> {
    let (file, types) = analyze(src, filename, resolver);
    let offset = offset_of(src, line, column);
    let token = file
        .syntax()
        .token_at_offset(TextSize::from(offset as u32))
        .right_biased()?;
    if !matches!(
        token.kind(),
        SyntaxKind::IDENT | SyntaxKind::SELF_KW | SyntaxKind::INT_NUMBER
    ) {
        return None;
    }
    let node = token.parent()?;
    let idx = LineIndex::new(src);
    let make = |contents: String, n: &SyntaxNode| {
        let r = n.text_range();
        let (l0, c0) = idx.line_col(usize::from(r.start()));
        let (l1, c1) = idx.line_col(usize::from(r.end()));
        Hover {
            contents,
            line: l0 + 1,
            column: c0 + 1,
            end_line: l1 + 1,
            end_column: c1 + 1,
        }
    };
    let b = bindings::current();
    let parent = node.parent();
    // A method name: the method's signature (with NS docs).
    if let Some(m) = parent.clone().and_then(ast::MethodCallExpr::cast)
        && m.name_ref().is_some_and(|n| n.syntax() == &node)
    {
        let name = token.text().to_string();
        let rt = m.receiver().and_then(|r| type_at(&types, r.syntax()));
        match &rt {
            Some(Ty::Extern(t)) => {
                if let Some(method) = b.types.get(t).and_then(|ty| ty.methods.get(&name)) {
                    let sig =
                        signature(&name, &binding_params(method), &method.ret, method.is_async);
                    return Some(make(
                        format!("{}\n*{t}*\n\n{}", code(&sig), method.doc),
                        &node,
                    ));
                }
            }
            Some(Ty::Adt(n)) => {
                if let Some(method) = types.adts.method(n, &name) {
                    let params: Vec<(String, Ty)> = method
                        .params
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (format!("_{i}"), t.clone()))
                        .collect();
                    let sig = signature(&name, &params, &method.ret, method.is_async);
                    return Some(make(format!("{}\n*{n}*", code(&sig)), &node));
                }
            }
            _ => {}
        }
        let ret = type_at(&types, m.syntax())?;
        return Some(make(
            code(&format!("{name}(..) -> {}", ret.describe())),
            &node,
        ));
    }
    // A field: its type (with NS docs).
    if let Some(fe) = parent.clone().and_then(ast::FieldExpr::cast)
        && fe.name_ref().is_some_and(|n| n.syntax() == &node)
    {
        let name = token.text().to_string();
        if let Some(Ty::Extern(t)) = fe.expr().and_then(|r| type_at(&types, r.syntax()))
            && let Some(field) = b.types.get(&t).and_then(|ty| ty.fields.get(&name))
        {
            return Some(make(
                format!(
                    "{}\n\n{}",
                    code(&format!("{t}.{name}: {}", field.ty.describe())),
                    field.doc
                ),
                &node,
            ));
        }
        let t = type_at(&types, fe.syntax())?;
        return Some(make(code(&format!("{name}: {}", t.describe())), &node));
    }
    // A binding or a path: the name and its type.
    for n in node.ancestors().take(5) {
        if matches!(
            n.kind(),
            SyntaxKind::NAME | SyntaxKind::PATH_EXPR | SyntaxKind::LITERAL
        ) && let Some(t) = type_at(&types, &n)
        {
            let text = match &t {
                Ty::Closure { params, ret } if n.kind() == SyntaxKind::PATH_EXPR => {
                    let ps: Vec<(String, Ty)> = params
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (format!("_{i}"), t.clone()))
                        .collect();
                    signature(token.text(), &ps, ret, false)
                }
                _ => format!("{}: {}", token.text(), t.describe()),
            };
            return Some(make(code(&text), &n));
        }
    }
    None
}

/// Completions at (line, column): the members of the value before a `.`.
pub fn completions(
    src: &str,
    filename: &str,
    line: u32,
    column: u32,
    resolver: Resolver,
) -> Vec<Completion> {
    let offset = offset_of(src, line, column);
    // Back over the partial name being typed to the `.`.
    let before = &src[..offset];
    let word_start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map(|(i, _)| i)
        .unwrap_or(offset);
    if !before[..word_start].ends_with('.') {
        // A plain identifier being typed: what's in scope.
        let prefix = &before[word_start..];
        if prefix.is_empty() || before[..word_start].ends_with("::") {
            return vec![];
        }
        let (file, types) = analyze(src, filename, resolver);
        return scope_completions(&file, &types, offset, prefix);
    }
    let dot = word_start - 1;
    let (file, types) = analyze(src, filename, resolver);
    let Some(token) = file
        .syntax()
        .token_at_offset(TextSize::from(dot as u32))
        .right_biased()
    else {
        return vec![];
    };
    let receiver = token.parent().and_then(|p| {
        ast::FieldExpr::cast(p.clone())
            .and_then(|f| f.expr())
            .or_else(|| ast::MethodCallExpr::cast(p).and_then(|m| m.receiver()))
    });
    let Some(rt) = receiver.and_then(|r| type_at(&types, r.syntax())) else {
        return vec![];
    };
    members(&rt, &types)
}

/// Locals declared before `offset` in the enclosing function, then the file's items.
fn scope_completions(
    file: &SourceFile,
    types: &Types,
    offset: usize,
    prefix: &str,
) -> Vec<Completion> {
    let mut out = Vec::new();
    let at = TextSize::from(offset as u32);
    let enclosing = file
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .find(|f| f.syntax().text_range().contains_inclusive(at));
    if let Some(f) = enclosing {
        for n in f.syntax().descendants().filter_map(ast::Name::cast) {
            if n.syntax().text_range().end() > at
                || n.syntax()
                    .parent()
                    .is_some_and(|p| p.kind() == SyntaxKind::FN)
            {
                continue;
            }
            let detail = type_at(types, n.syntax())
                .map(|t| t.describe())
                .unwrap_or_default();
            out.push(Completion {
                label: n.text().to_string(),
                kind: "variable",
                detail,
                documentation: String::new(),
            });
        }
        // `self` in a method.
        if f.param_list().and_then(|p| p.self_param()).is_some() {
            out.push(Completion {
                label: "self".into(),
                kind: "variable",
                detail: String::new(),
                documentation: String::new(),
            });
        }
    }
    for item in file.syntax().children() {
        let (name, kind, detail) = if let Some(f) = ast::Fn::cast(item.clone()) {
            (f.name(), "function", fn_label(&f))
        } else if let Some(s) = ast::Struct::cast(item.clone()) {
            (s.name(), "struct", String::new())
        } else if let Some(e) = ast::Enum::cast(item.clone()) {
            (e.name(), "enum", String::new())
        } else if let Some(t) = ast::Trait::cast(item.clone()) {
            (t.name(), "trait", String::new())
        } else if let Some(k) = ast::Const::cast(item.clone()) {
            (
                k.name(),
                "constant",
                k.ty()
                    .map(|t| t.syntax().text().to_string())
                    .unwrap_or_default(),
            )
        } else if let Some(m) = ast::Module::cast(item.clone()) {
            (m.name(), "module", String::new())
        } else {
            continue;
        };
        if let Some(n) = name {
            out.push(Completion {
                label: n.text().to_string(),
                kind,
                detail,
                documentation: String::new(),
            });
        }
    }
    out.retain(|c| c.label.starts_with(prefix));
    // Later declarations shadow earlier ones; keep one of each name.
    out.reverse();
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c.label.clone()));
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// `fn name(a: i32, b: &str) -> u32`, from the source text of the declaration.
fn fn_label(f: &ast::Fn) -> String {
    let name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
    let params = fn_params(f);
    let ret = f
        .ret_type()
        .map(|r| format!(" {}", r.syntax().text()))
        .unwrap_or_default();
    let a = if f.async_token().is_some() {
        "async "
    } else {
        ""
    };
    format!("{a}fn {name}({}){ret}", params.join(", "))
}

/// The parameters of a declaration as written (without `self`).
fn fn_params(f: &ast::Fn) -> Vec<String> {
    f.param_list()
        .map(|pl| pl.params().map(|p| p.syntax().text().to_string()).collect())
        .unwrap_or_default()
}

/// Parameter hints for the call around the cursor.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelp {
    /// The whole signature.
    pub label: String,
    /// Each parameter as it appears in `label`.
    pub params: Vec<String>,
    /// The parameter the cursor is in.
    pub active: usize,
    pub documentation: String,
}

/// The signature of the function or method whose argument list holds the cursor.
pub fn signature_help(
    src: &str,
    filename: &str,
    line: u32,
    column: u32,
    resolver: Resolver,
) -> Option<SignatureHelp> {
    let offset = offset_of(src, line, column);
    let (file, types) = analyze(src, filename, resolver);
    let at = TextSize::from(offset as u32);
    let token = file.syntax().token_at_offset(at).left_biased()?;
    let args = token
        .parent_ancestors()
        .filter_map(ast::ArgList::cast)
        .find(|l| {
            let r = l.syntax().text_range();
            r.start() < at && (at < r.end() || l.r_paren_token().is_none())
        })?;
    // Commas directly in this argument list, before the cursor.
    let active = args
        .syntax()
        .children_with_tokens()
        .filter(|e| e.kind() == SyntaxKind::COMMA && e.text_range().end() <= at)
        .count();
    let parent = args.syntax().parent()?;
    let help = |label: String, params: Vec<String>, documentation: String| SignatureHelp {
        label,
        params,
        active,
        documentation,
    };
    let find_fn = |name: &str| {
        file.syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .find(|f| f.name().is_some_and(|n| n.text() == name))
    };
    if let Some(m) = ast::MethodCallExpr::cast(parent.clone()) {
        let name = m.name_ref()?.text().to_string();
        let rt = m.receiver().and_then(|r| type_at(&types, r.syntax()));
        if let Some(Ty::Extern(t)) = &rt {
            let b = bindings::current();
            let method = b.types.get(t)?.methods.get(&name)?;
            let params: Vec<String> = binding_params(method)
                .into_iter()
                .map(|(n, t)| format!("{n}: {}", t.describe()))
                .collect();
            let label = signature(&name, &binding_params(method), &method.ret, method.is_async);
            return Some(help(label, params, method.doc.clone()));
        }
        let f = find_fn(&name)?;
        return Some(help(fn_label(&f), fn_params(&f), String::new()));
    }
    let call = ast::CallExpr::cast(parent)?;
    let ast::Expr::PathExpr(p) = call.expr()? else {
        return None;
    };
    let name = p.path()?.segment()?.name_ref()?.text().to_string();
    let f = find_fn(&name)?;
    Some(help(fn_label(&f), fn_params(&f), String::new()))
}

fn method(label: &str, detail: String, doc: &str) -> Completion {
    Completion {
        label: label.to_string(),
        kind: "method",
        detail,
        documentation: doc.to_string(),
    }
}

/// The fields and methods of a value of type `t`.
fn members(t: &Ty, types: &Types) -> Vec<Completion> {
    let b = bindings::current();
    let mut out = Vec::new();
    match t {
        Ty::Extern(name) => {
            if let Some(ty) = b.types.get(name) {
                for (n, m) in &ty.methods {
                    out.push(method(
                        n,
                        signature(n, &binding_params(m), &m.ret, m.is_async),
                        &m.doc,
                    ));
                }
                for (n, f) in &ty.fields {
                    out.push(Completion {
                        label: n.clone(),
                        kind: "field",
                        detail: f.ty.describe(),
                        documentation: f.doc.clone(),
                    });
                }
            }
        }
        Ty::Adt(name) => {
            if let Some(def) = types.adts.defs.get(name)
                && !def.is_enum
                && let Some(v) = def.variants.first()
                && v.shape == crate::adt::Shape::Named
            {
                for f in &v.fields {
                    out.push(Completion {
                        label: f.name.clone(),
                        kind: "field",
                        detail: f.ty.describe(),
                        documentation: String::new(),
                    });
                }
            }
            if let Some(ms) = types.adts.methods.get(name) {
                for (n, m) in ms.iter().filter(|(_, m)| m.receiver.is_some()) {
                    let params: Vec<(String, Ty)> = m
                        .params
                        .iter()
                        .enumerate()
                        .map(|(i, t)| (format!("_{i}"), t.clone()))
                        .collect();
                    out.push(method(n, signature(n, &params, &m.ret, m.is_async), ""));
                }
            }
            if let Some(def) = types.adts.defs.get(name) {
                for (d, m) in [
                    ("Clone", "clone"),
                    ("PartialEq", "eq"),
                    ("Ord", "cmp"),
                    ("PartialOrd", "partial_cmp"),
                ] {
                    if def.derives(d) {
                        out.push(method(m, format!("derived from {d}"), ""));
                    }
                }
                if def.display {
                    out.push(method("to_string", "fn to_string() -> String".into(), ""));
                }
            }
        }
        Ty::Dyn(tr) => {
            let mut todo = vec![tr.clone()];
            let mut seen = Vec::new();
            while let Some(t) = todo.pop() {
                if seen.contains(&t) {
                    continue;
                }
                seen.push(t.clone());
                if let Some(def) = types.adts.traits.get(&t) {
                    for m in &def.methods {
                        let params: Vec<(String, Ty)> = m
                            .sig
                            .params
                            .iter()
                            .enumerate()
                            .map(|(i, t)| (format!("_{i}"), t.clone()))
                            .collect();
                        out.push(method(
                            &m.name,
                            signature(&m.name, &params, &m.sig.ret, m.sig.is_async),
                            "",
                        ));
                    }
                    todo.extend(def.supers.iter().cloned());
                }
            }
        }
        other => {
            for name in std_methods(other) {
                out.push(method(name, String::new(), ""));
            }
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out.dedup_by(|a, b| a.label == b.label);
    out
}

/// The std methods CarciniScript supports on each built-in type.
fn std_methods(t: &Ty) -> &'static [&'static str] {
    match t {
        Ty::Str => &[
            "as_str",
            "bytes",
            "char_indices",
            "chars",
            "clear",
            "contains",
            "ends_with",
            "eq_ignore_ascii_case",
            "find",
            "insert",
            "insert_str",
            "is_char_boundary",
            "is_empty",
            "len",
            "lines",
            "matches",
            "parse",
            "pop",
            "push",
            "push_str",
            "remove",
            "repeat",
            "replace",
            "replacen",
            "rfind",
            "rsplit",
            "rsplit_once",
            "rsplitn",
            "split",
            "split_once",
            "split_whitespace",
            "splitn",
            "starts_with",
            "strip_prefix",
            "strip_suffix",
            "to_ascii_lowercase",
            "to_ascii_uppercase",
            "to_lowercase",
            "to_owned",
            "to_string",
            "to_uppercase",
            "trim",
            "trim_end",
            "trim_end_matches",
            "trim_matches",
            "trim_start",
            "trim_start_matches",
            "truncate",
        ],
        Ty::Vec(_) => &[
            "append",
            "as_slice",
            "binary_search",
            "binary_search_by",
            "binary_search_by_key",
            "chunks",
            "clear",
            "clone",
            "concat",
            "contains",
            "dedup",
            "dedup_by_key",
            "drain",
            "ends_with",
            "extend",
            "extend_from_slice",
            "fill",
            "first",
            "get",
            "insert",
            "into_iter",
            "is_empty",
            "is_sorted",
            "iter",
            "iter_mut",
            "join",
            "last",
            "len",
            "pop",
            "push",
            "remove",
            "repeat",
            "retain",
            "reverse",
            "rotate_left",
            "rotate_right",
            "sort",
            "sort_by",
            "sort_by_key",
            "sort_unstable",
            "sort_unstable_by",
            "split_at",
            "split_off",
            "starts_with",
            "swap",
            "swap_remove",
            "to_vec",
            "truncate",
            "windows",
        ],
        Ty::Iter { .. } => &[
            "all",
            "any",
            "chain",
            "cloned",
            "collect",
            "copied",
            "count",
            "enumerate",
            "filter",
            "filter_map",
            "find",
            "find_map",
            "flat_map",
            "flatten",
            "fold",
            "for_each",
            "inspect",
            "last",
            "map",
            "max",
            "max_by",
            "max_by_key",
            "min",
            "min_by",
            "min_by_key",
            "next",
            "nth",
            "position",
            "product",
            "reduce",
            "rev",
            "skip",
            "skip_while",
            "step_by",
            "sum",
            "take",
            "take_while",
            "zip",
        ],
        Ty::Option(_) => &[
            "and_then",
            "as_mut",
            "as_ref",
            "cloned",
            "copied",
            "expect",
            "filter",
            "is_none",
            "is_none_or",
            "is_some",
            "is_some_and",
            "map",
            "map_or",
            "map_or_else",
            "ok_or",
            "ok_or_else",
            "or",
            "or_else",
            "replace",
            "take",
            "unwrap",
            "unwrap_or",
            "unwrap_or_default",
            "unwrap_or_else",
            "zip",
        ],
        Ty::Result(..) => &[
            "and_then",
            "err",
            "expect",
            "expect_err",
            "is_err",
            "is_err_and",
            "is_ok",
            "is_ok_and",
            "map",
            "map_err",
            "map_or",
            "ok",
            "or_else",
            "unwrap",
            "unwrap_err",
            "unwrap_or",
            "unwrap_or_default",
            "unwrap_or_else",
        ],
        Ty::Set(_) => &[
            "clear",
            "clone",
            "contains",
            "difference",
            "extend",
            "insert",
            "intersection",
            "is_disjoint",
            "is_empty",
            "is_subset",
            "is_superset",
            "iter",
            "len",
            "remove",
            "retain",
            "symmetric_difference",
            "union",
        ],
        Ty::Map(..) => &[
            "clear",
            "clone",
            "contains_key",
            "entry",
            "extend",
            "get",
            "get_mut",
            "insert",
            "is_empty",
            "iter",
            "iter_mut",
            "keys",
            "len",
            "remove",
            "retain",
            "values",
            "values_mut",
        ],
        Ty::Int(_) | Ty::IntLit => &[
            "abs",
            "abs_diff",
            "checked_add",
            "checked_div",
            "checked_mul",
            "checked_sub",
            "clamp",
            "cmp",
            "count_ones",
            "is_power_of_two",
            "leading_zeros",
            "max",
            "min",
            "pow",
            "rem_euclid",
            "saturating_add",
            "saturating_mul",
            "saturating_sub",
            "signum",
            "to_string",
            "trailing_zeros",
            "wrapping_add",
            "wrapping_mul",
            "wrapping_sub",
        ],
        Ty::Float { .. } => &[
            "abs",
            "atan2",
            "ceil",
            "clamp",
            "cos",
            "exp",
            "floor",
            "is_finite",
            "is_nan",
            "ln",
            "log10",
            "max",
            "min",
            "partial_cmp",
            "powf",
            "powi",
            "round",
            "signum",
            "sin",
            "sqrt",
            "tan",
            "to_string",
            "total_cmp",
            "trunc",
        ],
        Ty::Char => &[
            "is_alphabetic",
            "is_alphanumeric",
            "is_ascii",
            "is_ascii_digit",
            "is_ascii_lowercase",
            "is_ascii_uppercase",
            "is_digit",
            "is_lowercase",
            "is_numeric",
            "is_uppercase",
            "is_whitespace",
            "to_ascii_lowercase",
            "to_ascii_uppercase",
            "to_digit",
            "to_string",
        ],
        _ => &[],
    }
}
