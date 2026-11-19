//! Language service for editors: hover (the type under the cursor, with NS docs) and completions
//! after `.` (NS methods and fields, struct fields and methods, std methods). Works on code with
//! syntax errors, since that's what an editor has while you type.

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind, SyntaxNode, TextSize,
    ast::{self, HasAttrs, HasName},
};
use serde::Serialize;

use crate::bindings;
use crate::lineindex::LineIndex;
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
pub(crate) fn offset_of(src: &str, line: u32, column: u32) -> usize {
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

/// The parsed file and its types. With `macro_rules!` macros, those of the expansion, and the map
/// from its offsets back to `src`'s.
pub(crate) struct Analysis {
    pub(crate) file: SourceFile,
    pub(crate) types: Types,
    pub(crate) map: Option<std::rc::Rc<crate::mbe::OffsetMap>>,
}

impl Analysis {
    /// An offset in `src` as one in the analyzed text (None inside a macro invocation).
    pub(crate) fn at(&self, offset: usize) -> Option<usize> {
        match &self.map {
            Some(m) => m.cur(offset),
            None => Some(offset),
        }
    }

    /// Line/column lookups for nodes of the analyzed text, in `src`.
    fn index<'a>(&self, src: &'a str) -> LineIndex<'a> {
        match &self.map {
            Some(m) => LineIndex::mapped(src, m.clone()),
            None => LineIndex::new(src),
        }
    }
}

/// `analyze_uncached`, kept for the next request on the same text (hover, completions and
/// signature help each ask).
pub(crate) fn analyze(src: &str, filename: &str, resolver: Resolver) -> std::rc::Rc<Analysis> {
    thread_local! {
        static ANALYSES: crate::memo::Cache<std::rc::Rc<Analysis>> = Default::default();
    }
    crate::memo::cached(&ANALYSES, src, filename, resolver, |resolver| {
        std::rc::Rc::new(analyze_uncached(src, filename, resolver))
    })
}

fn analyze_uncached(src: &str, filename: &str, resolver: Resolver) -> Analysis {
    // (A proc-macro crate as it compiles; positions are unchanged.)
    let prepared;
    let src = if crate::procmacro::is_proc_crate(src) {
        prepared = crate::procmacro::prepared(src);
        prepared.as_str()
    } else {
        src
    };
    let macros = crate::mbe::imported_macros(src, filename, resolver);
    // (A `jsx!` call being typed doesn't stop the rest of the file being analyzed.)
    let pre = crate::jsx::leniently(|| {
        crate::mbe::preprocess_with(src, &macros, Some((filename, resolver)))
    });
    if let Ok(Some(exp)) = pre {
        // (`std::time`'s prelude after the program, as it compiles.)
        let text = crate::stdtime::with_prelude(&exp.text, filename, resolver, true).unwrap_or(exp.text);
        let file = crate::macros::parse_tree(&text);
        let map = std::rc::Rc::new(exp.map);
        let origin = typeck::Origin {
            map: Some(map.clone()),
            len: src.len(),
        };
        let (types, _) = typeck::check(&file, &text, filename, resolver, &origin);
        return Analysis {
            file,
            types,
            map: Some(map),
        };
    }
    let origin = typeck::Origin {
        map: None,
        len: src.len(),
    };
    let with_time = crate::stdtime::with_prelude(src, filename, resolver, true);
    let src = with_time.as_deref().unwrap_or(src);
    let file = crate::macros::parse_tree(src);
    let (types, _) = typeck::check(&file, src, filename, resolver, &origin);
    Analysis {
        file,
        types,
        map: None,
    }
}

pub(crate) fn type_at(types: &Types, n: &SyntaxNode) -> Option<Ty> {
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
    signature_of(name, &ps, ret, is_async)
}

/// A signature from parameters already written out (`host?: &str`).
fn signature_of(name: &str, params: &[String], ret: &Ty, is_async: bool) -> String {
    let ret = if *ret == Ty::Unit {
        String::new()
    } else {
        format!(" -> {}", ret.describe())
    };
    let a = if is_async { "async " } else { "" };
    format!("{a}fn {name}({}){ret}", params.join(", "))
}

/// An NS method's parameters as declared (`connect_path: &[String]`); optional ones (which may be
/// left out) are marked `?`.
fn binding_params(m: &bindings::Method) -> Vec<String> {
    m.params
        .iter()
        .map(|p| {
            let q = if p.optional { "?" } else { "" };
            format!("{}{q}: {}", p.name, p.shown)
        })
        .collect()
}

fn binding_signature(name: &str, m: &bindings::Method) -> String {
    signature_of(name, &binding_params(m), &m.ret, m.is_async)
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
    // `jsx!` markup: a tag or an attribute.
    if src.contains("jsx!")
        && let Some((r, contents)) =
            crate::jsx_service::hover(src, filename, offset_of(src, line, column), resolver)
    {
        let idx = LineIndex::new(src);
        let (l0, c0) = idx.line_col(usize::from(r.start()));
        let (l1, c1) = idx.line_col(usize::from(r.end()));
        return Some(Hover {
            contents,
            line: l0 + 1,
            column: c0 + 1,
            end_line: l1 + 1,
            end_column: c1 + 1,
        });
    }
    let an = analyze(src, filename, resolver);
    let offset = an.at(offset_of(src, line, column))?;
    let (file, types) = (&an.file, &an.types);
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
    let idx = an.index(src);
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
        let rt = m.receiver().and_then(|r| type_at(types, r.syntax()));
        match &rt {
            Some(Ty::Extern(t)) => {
                if let Some(method) = b.types.get(t).and_then(|ty| ty.methods.get(&name)) {
                    let sig = binding_signature(&name, method);
                    return Some(make(
                        format!("{}\n*{t}*\n\n{}", code(&sig), method.doc),
                        &node,
                    ));
                }
            }
            Some(Ty::Adt(n, _)) => {
                if let Some(method) = types.adts.method(n, &name) {
                    // The declaration as written (parameter names, `&self`, `&` in the return
                    // type); a bundled crate's prelude under the crate's own names.
                    let sig = match fn_decl(file, Some(n), &name) {
                        Some(f) => user_names(&fn_label(&f)),
                        None => {
                            let params: Vec<(String, Ty)> = method
                                .params
                                .iter()
                                .enumerate()
                                .map(|(i, t)| (format!("_{i}"), t.clone()))
                                .collect();
                            signature(&name, &params, &method.ret, method.is_async)
                        }
                    };
                    let owner = crate::bundled::user_name(n).unwrap_or(n);
                    let docs = fn_docs(file, Some(n), &name);
                    return Some(make(
                        with_docs(format!("{}\n*{owner}*", code(&sig)), &docs),
                        &node,
                    ));
                }
            }
            _ => {}
        }
        // A std method (or the bundled `itertools`'s): its signature and summary.
        if let Some(t) = &rt
            && let Some((owner, sig, doc)) = std_show(t, &name)
        {
            return Some(make(
                format!("{}\n*{owner}*\n\n{doc}", code(&sig)),
                &node,
            ));
        }
        let ret = type_at(types, m.syntax())?;
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
        if let Some(Ty::Extern(t)) = fe.expr().and_then(|r| type_at(types, r.syntax()))
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
        let t = type_at(types, fe.syntax())?;
        let docs = match fe.expr().and_then(|r| type_at(types, r.syntax())) {
            Some(Ty::Adt(owner, _)) => field_docs(file, &owner, &name),
            _ => String::new(),
        };
        return Some(make(
            with_docs(code(&format!("{name}: {}", t.describe())), &docs),
            &node,
        ));
    }
    // A function's name where it's called or declared: its signature and doc comment.
    let decl = parent
        .clone()
        .and_then(ast::Fn::cast)
        .filter(|_| node.kind() == SyntaxKind::NAME);
    let called = node
        .ancestors()
        .find_map(ast::PathExpr::cast)
        .filter(|p| {
            p.path()
                .and_then(|p| p.as_single_name_ref())
                .is_some_and(|n| n.syntax() == &node)
        })
        .filter(|p| {
            p.syntax()
                .parent()
                .and_then(ast::CallExpr::cast)
                .and_then(|c| c.expr())
                .is_some_and(|e| e.syntax() == p.syntax())
        });
    let target = decl.or_else(|| {
        called.as_ref()?;
        file.syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .find(|f| {
                f.name().is_some_and(|n| n.text() == token.text())
                    && !f
                        .syntax()
                        .ancestors()
                        .any(|a| ast::Impl::can_cast(a.kind()) || ast::Trait::can_cast(a.kind()))
            })
    });
    if let Some(f) = target {
        let sig = match f.body() {
            Some(b) => {
                let start = f.syntax().text_range().start();
                let end = b.syntax().text_range().start();
                let text = f.syntax().text().to_string();
                let cut = usize::from(end - start);
                text[..cut].trim_end().to_string()
            }
            None => f.syntax().text().to_string(),
        };
        // Leave out the doc comment and attributes, which come first.
        let sig = sig
            .lines()
            .skip_while(|l| l.trim_start().starts_with("///") || l.trim_start().starts_with("#["))
            .collect::<Vec<_>>()
            .join("\n");
        return Some(make(with_docs(code(&sig), &docs_of(&f)), &node));
    }
    // A bindings enum (a TS union) or one of its variants: `ScriptArg::Str(String)` with the
    // union it stands for.
    if node.kind() == SyntaxKind::NAME_REF
        && let Some(seg) = node.parent().and_then(ast::PathSegment::cast)
        && let Some(path) = seg.syntax().parent().and_then(ast::Path::cast)
    {
        let enum_hover = |et: &bindings::ExternType, name: &str| {
            let variants: Vec<String> = et
                .variants
                .iter()
                .map(|v| format!("{}({})", v.name, v.shown))
                .collect();
            format!(
                "{}\n*NS bindings*\n\n{}",
                code(&format!("enum {name} {{ {} }}", variants.join(", "))),
                et.doc
            )
        };
        let qualifier = path
            .qualifier()
            .and_then(|q| q.segment()?.name_ref())
            .map(|n| n.text().to_string());
        if let Some(q) = &qualifier
            && let Some(et) = b.enum_of(q)
            && let Some((_, v)) = et.variant(token.text())
        {
            let text = format!("{q}::{}({})", v.name, v.shown);
            return Some(make(
                with_docs(format!("{}\n*{q}*", code(&text)), &v.doc),
                &node,
            ));
        }
        if let Some(q) = &qualifier
            && let Some(et) = b.enum_of(token.text())
            && q.is_empty()
        {
            return Some(make(enum_hover(et, token.text()), &node));
        }
        if qualifier.is_none()
            && let Some(et) = b.enum_of(token.text())
        {
            return Some(make(enum_hover(et, token.text()), &node));
        }
    }
    // `String::from`, `Vec::with_capacity`, `i32::MAX`: a std type's associated function or
    // constant.
    if node.kind() == SyntaxKind::NAME_REF
        && let Some(seg) = node.parent().and_then(ast::PathSegment::cast)
        && let Some(path) = seg.syntax().parent().and_then(ast::Path::cast)
        && let Some(q) = path.qualifier()
        && let Some(qn) = q.segment().and_then(|s| s.name_ref())
        && let Some((owner, sig, doc)) = crate::std_docs::show_assoc(&qn.text(), token.text())
    {
        return Some(make(
            format!("{}\n*{owner}*\n\n{doc}", code(&sig)),
            &node,
        ));
    }
    // A binding or a path: the name and its type.
    for n in node.ancestors().take(5) {
        if matches!(
            n.kind(),
            SyntaxKind::NAME | SyntaxKind::PATH_EXPR | SyntaxKind::LITERAL
        ) && let Some(t) = type_at(types, &n)
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
            // A free function (called or declared here): its doc comment.
            let docs = match &t {
                Ty::Closure { .. } if n.kind() == SyntaxKind::PATH_EXPR => {
                    fn_docs(file, None, token.text())
                }
                _ => String::new(),
            };
            return Some(make(with_docs(code(&text), &docs), &n));
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
    // `jsx!` markup: tag names and attributes.
    if src.contains("jsx!")
        && let Some(out) = crate::jsx_service::completions(src, filename, offset, resolver)
    {
        return out;
    }
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
        let an = analyze(src, filename, resolver);
        let Some(at) = an.at(offset) else {
            return vec![];
        };
        return scope_completions(&an.file, &an.types, at, prefix);
    }
    let an = analyze(src, filename, resolver);
    let Some(dot) = an.at(word_start - 1) else {
        return vec![];
    };
    let (file, types) = (&an.file, &an.types);
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
    let Some(rt) = receiver.and_then(|r| type_at(types, r.syntax())) else {
        return vec![];
    };
    // (Not the operator impls' internal names, `add$0`, or the `std::time` prelude's private
    // `__` fields.)
    members(&rt, types)
        .into_iter()
        .filter(|c| !c.label.contains('$') && !c.label.starts_with("__"))
        .collect()
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
    let mut params = fn_params(f);
    if let Some(sp) = f.param_list().and_then(|pl| pl.self_param()) {
        params.insert(0, sp.syntax().text().to_string());
    }
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
    let an = analyze(src, filename, resolver);
    let offset = an.at(offset_of(src, line, column))?;
    let (file, types) = (&an.file, &an.types);
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
        let rt = m.receiver().and_then(|r| type_at(types, r.syntax()));
        if let Some(Ty::Extern(t)) = &rt {
            let b = bindings::current();
            let method = b.types.get(t)?.methods.get(&name)?;
            let params = binding_params(method);
            let label = binding_signature(&name, method);
            return Some(help(label, params, method.doc.clone()));
        }
        // A method of the receiver's own type (not any `fn` of that name).
        if let Some(Ty::Adt(n, _)) = &rt
            && let Some(f) = fn_decl(file, Some(n), &name)
        {
            let params = fn_params(&f).iter().map(|p| user_names(p)).collect();
            return Some(help(user_names(&fn_label(&f)), params, docs_of(&f)));
        }
        // A std method (or the bundled `itertools`'s).
        if let Some(t) = &rt
            && let Some((_, sig, doc)) = std_show(t, &name)
        {
            let params = crate::std_docs::params(&sig);
            return Some(help(sig, params, doc.to_string()));
        }
        let f = find_fn(&name)?;
        return Some(help(fn_label(&f), fn_params(&f), String::new()));
    }
    let call = ast::CallExpr::cast(parent)?;
    let ast::Expr::PathExpr(p) = call.expr()? else {
        return None;
    };
    let path = p.path()?;
    let name = path.segment()?.name_ref()?.text().to_string();
    // `String::from(..)`, `Vec::with_capacity(..)`: a std type's associated function.
    if let Some(q) = path.qualifier()
        && let Some(qn) = q.segment().and_then(|s| s.name_ref())
        && let Some((_, sig, doc)) = crate::std_docs::show_assoc(&qn.text(), &name)
    {
        let params = crate::std_docs::params(&sig);
        return Some(help(sig, params, doc.to_string()));
    }
    let f = match path.qualifier().and_then(|q| q.segment()?.name_ref()) {
        Some(owner) => fn_decl(file, Some(&owner.text()), &name).or_else(|| find_fn(&name))?,
        None => find_fn(&name)?,
    };
    Some(help(fn_label(&f), fn_params(&f), docs_of(&f)))
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
                    out.push(method(n, binding_signature(n, m), &m.doc));
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
        Ty::Adt(name, _) => {
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
                match crate::std_docs::show(other, name) {
                    Some((_, sig, doc)) => out.push(method(name, sig, doc)),
                    None => out.push(method(name, String::new(), "")),
                }
            }
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out.dedup_by(|a, b| a.label == b.label);
    out
}

/// The std method `name` of a value of type `t` (or the bundled `itertools`'s, on an iterator):
/// the receiver as a hover names it, the signature with `t`'s arguments filled in, and a summary.
fn std_show(t: &Ty, name: &str) -> Option<(String, String, &'static str)> {
    crate::std_docs::show(t, name).or_else(|| {
        if !matches!(t, Ty::Iter { .. } | Ty::Range(..)) {
            return None;
        }
        let (_, args) = crate::std_docs::owners(t)?;
        let d = crate::std_docs::lookup("Itertools", name)?;
        Some((
            crate::std_docs::receiver("Itertools", &args),
            crate::std_docs::substitute(d.sig, &args),
            d.doc,
        ))
    })
}

/// `text` with a bundled crate's prelude names replaced by the crate's own (`PgNodeIndex` is
/// petgraph's `NodeIndex`).
fn user_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(|c: char| c.is_alphanumeric() || c == '_') {
        out.push_str(&rest[..i]);
        let end = rest[i..]
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map_or(rest.len(), |e| i + e);
        let word = &rest[i..end];
        out.push_str(crate::bundled::user_name(word).unwrap_or(word));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The declaration of the function `name`: a free function (`ty` None), or a method of `ty`
/// (from its impls, else from a trait declaring it).
fn fn_decl(file: &SourceFile, ty: Option<&str>, name: &str) -> Option<ast::Fn> {
    let mut in_trait = None;
    for f in file
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|f| f.name().is_some_and(|n| n.text() == name))
    {
        let imp = f.syntax().ancestors().find_map(ast::Impl::cast);
        let owner = imp.and_then(|i| crate::adt::impl_parts(&i).1);
        let trait_decl = f.syntax().ancestors().any(|a| ast::Trait::can_cast(a.kind()));
        match ty {
            None if owner.is_none() && !trait_decl => return Some(f),
            Some(t) if owner.as_deref() == Some(t) => return Some(f),
            Some(_) if trait_decl && in_trait.is_none() => in_trait = Some(f),
            _ => {}
        }
    }
    in_trait
}

/// The std methods CarciniScript supports on each built-in type.
pub fn std_methods(t: &Ty) -> &'static [&'static str] {
    use crate::ty::Coll;
    match t {
        Ty::Coll(Coll::VecDeque, _) => &[
            "append",
            "back",
            "clear",
            "clone",
            "contains",
            "drain",
            "extend",
            "front",
            "get",
            "insert",
            "into_iter",
            "is_empty",
            "iter",
            "iter_mut",
            "len",
            "make_contiguous",
            "pop_back",
            "pop_front",
            "push_back",
            "push_front",
            "remove",
            "retain",
            "swap",
            "truncate",
        ],
        Ty::Coll(Coll::BinaryHeap, _) => &[
            "append",
            "clear",
            "clone",
            "drain",
            "extend",
            "into_sorted_vec",
            "into_vec",
            "is_empty",
            "iter",
            "len",
            "peek",
            "pop",
            "push",
        ],
        Ty::Coll(Coll::BTreeMap, _) => &[
            "append",
            "clear",
            "clone",
            "contains_key",
            "entry",
            "extend",
            "first_key_value",
            "get",
            "get_mut",
            "insert",
            "into_iter",
            "into_keys",
            "into_values",
            "is_empty",
            "iter",
            "iter_mut",
            "keys",
            "last_key_value",
            "len",
            "pop_first",
            "pop_last",
            "remove",
            "retain",
            "values",
            "values_mut",
        ],
        Ty::Coll(Coll::BTreeSet, _) => &[
            "append",
            "clear",
            "clone",
            "contains",
            "difference",
            "extend",
            "first",
            "insert",
            "intersection",
            "into_iter",
            "is_disjoint",
            "is_empty",
            "is_subset",
            "is_superset",
            "iter",
            "last",
            "len",
            "pop_first",
            "pop_last",
            "remove",
            "retain",
            "symmetric_difference",
            "union",
        ],
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
            "write_str",
            "write_char",
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
            "map_while",
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
            "scan",
            "skip",
            "skip_while",
            "step_by",
            "sum",
            "take",
            "take_while",
            "unzip",
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
            "div_euclid",
            "ilog10",
            "ilog2",
            "is_negative",
            "is_positive",
            "is_power_of_two",
            "isqrt",
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
            "copysign",
            "cos",
            "div_euclid",
            "exp",
            "floor",
            "fract",
            "hypot",
            "is_finite",
            "is_infinite",
            "is_nan",
            "is_sign_negative",
            "ln",
            "log",
            "log10",
            "log2",
            "max",
            "min",
            "mul_add",
            "partial_cmp",
            "powf",
            "powi",
            "recip",
            "rem_euclid",
            "round",
            "signum",
            "sin",
            "sqrt",
            "tan",
            "tanh",
            "to_bits",
            "to_degrees",
            "to_radians",
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

/// The `///` doc comment of an item, as markdown (empty when there is none).
pub(crate) fn docs_of(item: &impl HasAttrs) -> String {
    item.doc_comments()
        .filter_map(|c| {
            let text = c.syntax().text().to_string();
            let line = text.strip_prefix("///")?.to_string();
            Some(line.strip_prefix(' ').map(str::to_string).unwrap_or(line))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The doc comment of the function `name`: a free function (`ty` None), or a method of `ty`
/// (from its impls, else from a trait declaring it, whose docs an impl inherits).
fn fn_docs(file: &SourceFile, ty: Option<&str>, name: &str) -> String {
    let fns: Vec<(Option<String>, bool, ast::Fn)> = file
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|f| f.name().is_some_and(|n| n.text() == name))
        .map(|f| {
            let imp = f.syntax().ancestors().find_map(ast::Impl::cast);
            let in_trait = f.syntax().ancestors().find_map(ast::Trait::cast).is_some();
            (imp.and_then(|i| crate::adt::impl_parts(&i).1), in_trait, f)
        })
        .collect();
    let pick = |want: &dyn Fn(&Option<String>, bool) -> bool| {
        fns.iter()
            .filter(|(owner, in_trait, _)| want(owner, *in_trait))
            .map(|(_, _, f)| docs_of(f))
            .find(|d| !d.is_empty())
    };
    match ty {
        None => pick(&|owner, in_trait| owner.is_none() && !in_trait),
        Some(t) => {
            pick(&|owner, _| owner.as_deref() == Some(t)).or_else(|| pick(&|_, in_trait| in_trait))
        }
    }
    .unwrap_or_default()
}

/// The doc comment of field `name` of the struct `ty`.
fn field_docs(file: &SourceFile, ty: &str, name: &str) -> String {
    for s in file.syntax().descendants().filter_map(ast::Struct::cast) {
        if s.name().is_none_or(|n| n.text() != ty) {
            continue;
        }
        for f in s.syntax().descendants().filter_map(ast::RecordField::cast) {
            if f.name().is_some_and(|n| n.text() == name) {
                return docs_of(&f);
            }
        }
    }
    String::new()
}

/// `text` followed by `docs` when there are any.
fn with_docs(text: String, docs: &str) -> String {
    if docs.is_empty() {
        text
    } else {
        format!("{text}\n\n{docs}")
    }
}
