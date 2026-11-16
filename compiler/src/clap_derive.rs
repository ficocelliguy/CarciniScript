//! The bundled `clap`'s derives, written out as Rust over the clap prelude: `#[derive(Parser)]`
//! (`parse`, `try_parse_from`, `command`, ..), `#[derive(Args)]`, `#[derive(Subcommand)]` and
//! `#[derive(ValueEnum)]`, from the fields' types, their `#[arg(..)]` / `#[command(..)]`
//! attributes and doc comments, as clap_derive reads them. A pass of the `bundled` pre-pass.

use ra_ap_syntax::{
    AstNode, TextRange,
    ast::{self, HasAttrs, HasName},
};

use crate::bundled::{attr_args, attr_name, split_top};

const DERIVES: [&str; 4] = ["Parser", "Args", "Subcommand", "ValueEnum"];

/// The clap derives `adt` lists (dropped from its `#[derive(..)]`), and its helper attributes
/// (dropped too).
fn clap_derives(adt: &ast::Adt, edits: &mut Vec<(TextRange, String)>) -> Vec<String> {
    let mut found = Vec::new();
    for a in adt.attrs() {
        if attr_name(&a).as_deref() != Some("derive") {
            continue;
        }
        let Some(inner) = attr_args(&a) else { continue };
        let names: Vec<String> = inner
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let short = |n: &str| n.trim_start_matches("clap::").to_string();
        let (ours, kept): (Vec<String>, Vec<String>) = names.into_iter().partition(|n| {
            DERIVES.contains(&short(n).as_str()) && (!n.contains("::") || n.starts_with("clap::"))
        });
        if ours.is_empty() {
            continue;
        }
        found.extend(ours.iter().map(|n| short(n)));
        let new = if kept.is_empty() {
            String::new()
        } else {
            format!("#[derive({})]", kept.join(", "))
        };
        edits.push((a.syntax().text_range(), new));
    }
    found
}

/// `#[arg(..)]` / `#[clap(..)]` / `#[command(..)]` / `#[value(..)]` items, the attributes dropped:
/// `(key, value)`, the value `""` for a bare key.
fn helper_items(
    node: &impl HasAttrs,
    edits: &mut Vec<(TextRange, String)>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for a in node.attrs() {
        let Some(name) = attr_name(&a) else { continue };
        if !matches!(name.as_str(), "arg" | "clap" | "command" | "value") {
            continue;
        }
        edits.push((a.syntax().text_range(), String::new()));
        let Some(inner) = attr_args(&a) else { continue };
        for item in split_top(&inner, ',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            match item.split_once('=') {
                Some((k, v)) if !k.contains('(') => {
                    out.push((k.trim().to_string(), v.trim().to_string()))
                }
                _ => out.push((item.to_string(), String::new())),
            }
        }
    }
    out
}

/// Doc comment lines (`///` and one space taken off), blank lines at either end dropped.
fn doc_lines(node: &impl HasAttrs) -> Vec<String> {
    let mut lines: Vec<String> = node
        .doc_comments()
        .map(|c| {
            let t = c.syntax().text().to_string();
            let t = t.trim_start_matches('/').trim_start_matches('!');
            t.strip_prefix(' ').unwrap_or(t).trim_end().to_string()
        })
        .skip_while(|l| l.trim().is_empty())
        .collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
}

/// clap_derive's help from doc comments: the first paragraph (its lines joined, a final `.`
/// dropped), and all paragraphs when there are several.
fn doc_help(lines: &[String]) -> (Option<String>, Option<String>) {
    if lines.is_empty() {
        return (None, None);
    }
    let mut paras: Vec<String> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for l in lines {
        if l.trim().is_empty() {
            if !cur.is_empty() {
                paras.push(cur.join(" "));
                cur.clear();
            }
        } else {
            cur.push(l.trim().to_string());
        }
    }
    if !cur.is_empty() {
        paras.push(cur.join(" "));
    }
    let mut short = paras.first().cloned().unwrap_or_default();
    if short.ends_with('.') && !short.ends_with("..") {
        short.pop();
    }
    let long = (paras.len() > 1).then(|| paras.join("\n\n"));
    (Some(short), long)
}

fn lit(s: &str) -> String {
    format!("{s:?}")
}

/// `ReallySlow` → `really-slow`, `max_count` → `max-count`.
fn kebab(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            out.push('-');
        } else if c.is_uppercase() {
            if i > 0 && chars[i - 1] != '_' && !chars[i - 1].is_uppercase() {
                out.push('-');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(*c);
        }
    }
    out
}

/// A string attribute value (`"x"`), else the expression as written.
fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

/// The type inside `Option<..>` / `Vec<..>`.
fn inner_of<'a>(ty: &'a str, wrapper: &str) -> Option<&'a str> {
    let t = ty.trim();
    let rest = t.strip_prefix(wrapper)?.trim_start();
    let rest = rest.strip_prefix('<')?;
    let rest = rest.strip_suffix('>')?;
    Some(rest.trim())
}

/// The number types (and `char`, `bool`) clap checks as it parses.
fn value_kind(ty: &str) -> &str {
    match ty.trim() {
        t @ ("u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
        | "f32" | "f64" | "char") => t,
        _ => "",
    }
}

/// What the derives know about the file: the `ValueEnum` types.
pub struct Ctx {
    pub value_enums: Vec<String>,
}

/// The `__clap_args` / `__clap_from` bodies for named fields: (`.arg(..)` statements, field
/// initializers).
fn fields_code(
    fields: &ast::RecordFieldList,
    ctx: &Ctx,
    edits: &mut Vec<(TextRange, String)>,
) -> (String, String) {
    let mut args = String::new();
    let mut inits = String::new();
    for f in fields.fields() {
        let Some(name) = f.name().map(|n| n.text().to_string()) else {
            continue;
        };
        let ty = f
            .ty()
            .map(|t| t.syntax().text().to_string())
            .unwrap_or_default();
        let items = helper_items(&f, edits);
        let has = |k: &str| items.iter().any(|(x, _)| x == k);
        let get = |k: &str| items.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        // `#[command(subcommand)]` / `#[command(flatten)]`.
        if has("subcommand") {
            let (inner, optional) = match inner_of(&ty, "Option") {
                Some(t) => (t.to_string(), true),
                None => (ty.clone(), false),
            };
            args.push_str(&format!(
                "        let c = c.subcommands({inner}::__clap_subs()).subcommand_required({});\n",
                !optional
            ));
            if optional {
                inits.push_str(&format!(
                    "            {name}: match m.subcommand() {{\n                Some((n, sm)) => Some({inner}::__clap_from_sub(n, sm)?),\n                None => None,\n            }},\n"
                ));
            } else {
                inits.push_str(&format!(
                    "            {name}: match m.subcommand() {{\n                Some((n, sm)) => {inner}::__clap_from_sub(n, sm)?,\n                None => panic!(\"a subcommand is required\"),\n            }},\n"
                ));
            }
            continue;
        }
        if has("flatten") {
            args.push_str(&format!("        let c = {ty}::__clap_args(c);\n"));
            inits.push_str(&format!("            {name}: {ty}::__clap_from(m)?,\n"));
            continue;
        }
        if has("skip") {
            inits.push_str(&format!("            {name}: Default::default(),\n"));
            continue;
        }
        let id = get("id")
            .map(|v| unquote(&v))
            .unwrap_or_else(|| name.clone());
        let doc = doc_lines(&f);
        let (help, long_help) = doc_help(&doc);
        let help = get("help").map(|v| unquote(&v)).or(help);
        let long_help = get("long_help").map(|v| unquote(&v)).or(long_help);
        let action = get("action").map(|v| v.rsplit("::").next().unwrap_or(&v).trim().to_string());
        let opt_inner = inner_of(&ty, "Option").map(str::to_string);
        let vec_inner = inner_of(&ty, "Vec").map(str::to_string).or_else(|| {
            opt_inner
                .as_deref()
                .and_then(|t| inner_of(t, "Vec"))
                .map(str::to_string)
        });
        let is_bool = ty.trim() == "bool";
        let action = action.unwrap_or_else(|| {
            if is_bool {
                "SetTrue".into()
            } else if vec_inner.is_some() {
                "Append".into()
            } else {
                "Set".into()
            }
        });
        let elem = match (&vec_inner, &opt_inner) {
            (Some(v), _) => v.clone(),
            (None, Some(o)) => o.clone(),
            _ => ty.trim().to_string(),
        };
        let value_enum = has("value_enum") || ctx.value_enums.contains(&elem);
        let mut a = format!("ClapArg::new({})", lit(&id));
        if has("short") {
            let c = match get("short") {
                Some(v) if !v.is_empty() => v.trim().to_string(),
                _ => format!("'{}'", name.chars().next().unwrap_or('x')),
            };
            a.push_str(&format!(".short({c})"));
        }
        if has("long") {
            let l = match get("long") {
                Some(v) if !v.is_empty() => unquote(&v),
                _ => kebab(&name),
            };
            a.push_str(&format!(".long({})", lit(&l)));
        }
        let value_name = get("value_name")
            .map(|v| unquote(&v))
            .unwrap_or_else(|| name.to_uppercase());
        a.push_str(&format!(".value_name({})", lit(&value_name)));
        if let Some(h) = &help {
            a.push_str(&format!(".help({})", lit(h)));
        }
        if let Some(h) = &long_help {
            a.push_str(&format!(".long_help({})", lit(h)));
        }
        a.push_str(&format!(".action(ClapAction::{action})"));
        let has_default = has("default_value") || has("default_value_t");
        if let Some(v) = get("default_value")
            && !v.is_empty()
        {
            a.push_str(&format!(".default_value({v})"));
        }
        if has("default_value_t") {
            let expr = match get("default_value_t") {
                Some(v) if !v.is_empty() => v,
                _ => format!("<{elem} as Default>::default()"),
            };
            if value_enum {
                a.push_str(&format!(".default_value({elem}::__clap_name(&({expr})))"));
            } else {
                a.push_str(&format!(".default_value(&({expr}).to_string())"));
            }
        }
        if value_enum {
            a.push_str(&format!(".value_parser_values({elem}::__clap_values())"));
        } else {
            let kind = value_kind(&elem);
            if !kind.is_empty() {
                a.push_str(&format!(
                    ".value_parser(ClapValueParser::__of({}))",
                    lit(kind)
                ));
            }
        }
        let required = match get("required") {
            Some(v) => v.trim() != "false",
            None => {
                opt_inner.is_none()
                    && vec_inner.is_none()
                    && !has_default
                    && matches!(action.as_str(), "Set")
            }
        };
        if required {
            a.push_str(".required(true)");
        }
        args.push_str(&format!("        let c = c.arg({a});\n"));
        // How errors name it: `--count <COUNT>`, `<FILE>`.
        let positional = !has("short") && !has("long");
        let shown = if positional {
            if required {
                format!("<{value_name}>")
            } else {
                format!("[{value_name}]")
            }
        } else if has("long") {
            let l = match get("long") {
                Some(v) if !v.is_empty() => unquote(&v),
                _ => kebab(&name),
            };
            format!("--{l} <{value_name}>")
        } else {
            let c = match get("short") {
                Some(v) if !v.is_empty() => v.trim().trim_matches('\'').to_string(),
                _ => name.chars().next().unwrap_or('x').to_string(),
            };
            format!("-{c} <{value_name}>")
        };
        let value = match action.as_str() {
            "SetTrue" | "SetFalse" => format!("m.get_flag({})", lit(&id)),
            "Count" => format!("m.get_count({})", lit(&id)),
            _ => {
                if vec_inner.is_some() {
                    let v = format!("__clap_many::<{elem}>(m, {}, {})?", lit(&id), lit(&shown));
                    if opt_inner.is_some() {
                        format!(
                            "if m.contains_id({}) {{ Some({v}) }} else {{ None }}",
                            lit(&id)
                        )
                    } else {
                        v
                    }
                } else if opt_inner.is_some() {
                    format!("__clap_one::<{elem}>(m, {}, {})?", lit(&id), lit(&shown))
                } else {
                    format!(
                        "__clap_one::<{elem}>(m, {}, {})?.unwrap()",
                        lit(&id),
                        lit(&shown)
                    )
                }
            }
        };
        inits.push_str(&format!("            {name}: {value},\n"));
    }
    (args, inits)
}

/// The derives for `adt`, as text to put after it.
pub fn derive(adt: &ast::Adt, ctx: &Ctx, edits: &mut Vec<(TextRange, String)>) {
    let derives = clap_derives(adt, edits);
    if derives.is_empty() {
        return;
    }
    let Some(name) = adt.name().map(|n| n.text().to_string()) else {
        return;
    };
    let items = helper_items(adt, edits);
    let get = |k: &str| items.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
    let has = |k: &str| items.iter().any(|(x, _)| x == k);
    let mut out = String::new();
    match adt {
        ast::Adt::Struct(s) => {
            let fields = match s.field_list() {
                Some(ast::FieldList::RecordFieldList(l)) => Some(l),
                _ => None,
            };
            let (args, inits) = match &fields {
                Some(l) => fields_code(l, ctx, edits),
                None => (String::new(), String::new()),
            };
            let value = if fields.is_some() {
                format!("{name} {{\n{inits}        }}")
            } else {
                name.clone()
            };
            out.push_str(&format!(
                "\nimpl {name} {{\n    fn __clap_args(c: ClapCommand) -> ClapCommand {{\n{args}        c\n    }}\n    fn __clap_from(m: &ClapMatches) -> Result<{name}, ClapError> {{\n        let _ = m;\n        Ok({value})\n    }}\n}}\n"
            ));
            if derives.iter().any(|d| d == "Parser") {
                let (doc_short, doc_long) = doc_help(&doc_lines(adt));
                let mut cmd = format!(
                    "ClapCommand::new({})",
                    lit(&get("name").map(|v| unquote(&v)).unwrap_or_default())
                );
                let about = match get("about") {
                    Some(v) if !v.is_empty() => Some(unquote(&v)),
                    _ => doc_short,
                };
                if let Some(a) = about {
                    cmd.push_str(&format!(".about({})", lit(&a)));
                }
                let long_about = match get("long_about") {
                    Some(v) if v.trim() == "None" => None,
                    Some(v) if !v.is_empty() => Some(unquote(&v)),
                    _ => doc_long,
                };
                if let Some(a) = long_about {
                    cmd.push_str(&format!(".long_about({})", lit(&a)));
                }
                if has("version") {
                    let v = match get("version") {
                        Some(v) if !v.is_empty() => unquote(&v),
                        _ => "0.1.0".to_string(),
                    };
                    cmd.push_str(&format!(".version({})", lit(&v)));
                }
                if let Some(v) = get("arg_required_else_help") {
                    cmd.push_str(&format!(".arg_required_else_help({})", v.trim() != "false"));
                }
                out.push_str(&format!(
                    "\nimpl {name} {{\n    pub fn command() -> ClapCommand {{\n        {name}::__clap_args({cmd})\n    }}\n    pub fn try_parse_from<I>(args: I) -> Result<{name}, ClapError>\n    where\n        I: IntoIterator,\n        I::Item: ToString,\n    {{\n        let m = {name}::command().try_get_matches_from(args)?;\n        {name}::__clap_from(&m)\n    }}\n    pub fn parse_from<I>(args: I) -> {name}\n    where\n        I: IntoIterator,\n        I::Item: ToString,\n    {{\n        __clap_or_exit({name}::try_parse_from(args))\n    }}\n    pub fn try_parse() -> Result<{name}, ClapError> {{\n        {name}::try_parse_from(std::env::args())\n    }}\n    pub fn parse() -> {name} {{\n        __clap_or_exit({name}::try_parse())\n    }}\n    pub fn from_arg_matches(m: &ClapMatches) -> Result<{name}, ClapError> {{\n        {name}::__clap_from(m)\n    }}\n}}\n"
                ));
            }
        }
        ast::Adt::Enum(e) => {
            let Some(list) = e.variant_list() else { return };
            if derives.iter().any(|d| d == "ValueEnum") {
                let mut values = String::new();
                let mut names = String::new();
                let mut parse = String::new();
                for v in list.variants() {
                    let Some(vn) = v.name().map(|n| n.text().to_string()) else {
                        continue;
                    };
                    let vitems = helper_items(&v, edits);
                    let shown = vitems
                        .iter()
                        .find(|(k, _)| k == "name")
                        .map(|(_, x)| unquote(x))
                        .unwrap_or_else(|| kebab(&vn));
                    let (help, _) = doc_help(&doc_lines(&v));
                    let mut pv = format!("ClapPossibleValue::new({})", lit(&shown));
                    if let Some(h) = help {
                        pv.push_str(&format!(".help({})", lit(&h)));
                    }
                    values.push_str(&format!("{pv}, "));
                    names.push_str(&format!("            {name}::{vn} => {},\n", lit(&shown)));
                    parse.push_str(&format!(
                        "            {} => Ok({name}::{vn}),\n",
                        lit(&shown)
                    ));
                }
                out.push_str(&format!(
                    "\nimpl {name} {{\n    fn __clap_values() -> Vec<ClapPossibleValue> {{\n        vec![{values}]\n    }}\n    fn __clap_name(&self) -> &'static str {{\n        match self {{\n{names}        }}\n    }}\n}}\n"
                ));
                let text = adt
                    .syntax()
                    .ancestors()
                    .last()
                    .map(|r| r.text().to_string())
                    .unwrap_or_default();
                if !text.contains(&format!("FromStr for {name}")) {
                    out.push_str(&format!(
                        "\nimpl std::str::FromStr for {name} {{\n    type Err = String;\n    fn from_str(s: &str) -> Result<{name}, String> {{\n        match s {{\n{parse}            _ => Err(format!(\"invalid variant: {{}}\", s)),\n        }}\n    }}\n}}\n"
                    ));
                }
            }
            if derives.iter().any(|d| d == "Subcommand") {
                let mut subs = String::new();
                let mut arms = String::new();
                for v in list.variants() {
                    let Some(vn) = v.name().map(|n| n.text().to_string()) else {
                        continue;
                    };
                    let vitems = helper_items(&v, edits);
                    let sub_name = vitems
                        .iter()
                        .find(|(k, _)| k == "name")
                        .map(|(_, x)| unquote(x))
                        .unwrap_or_else(|| kebab(&vn));
                    let (help, long) = doc_help(&doc_lines(&v));
                    let mut c = format!("ClapCommand::new({})", lit(&sub_name));
                    if let Some(h) = help {
                        c.push_str(&format!(".about({})", lit(&h)));
                    }
                    if let Some(h) = long {
                        c.push_str(&format!(".long_about({})", lit(&h)));
                    }
                    match v.field_list() {
                        Some(ast::FieldList::RecordFieldList(l)) => {
                            let (args, inits) = fields_code(&l, ctx, edits);
                            subs.push_str(&format!("            {{\n        let c = {c};\n{args}        c\n            }},\n"));
                            arms.push_str(&format!(
                                "            {} => Ok({name}::{vn} {{\n{inits}        }}),\n",
                                lit(&sub_name)
                            ));
                        }
                        Some(ast::FieldList::TupleFieldList(l)) => {
                            let inner = l
                                .fields()
                                .next()
                                .and_then(|f| f.ty())
                                .map(|t| t.syntax().text().to_string())
                                .unwrap_or_default();
                            subs.push_str(&format!("            {inner}::__clap_args({c}),\n"));
                            arms.push_str(&format!(
                                "            {} => Ok({name}::{vn}({inner}::__clap_from(m)?)),\n",
                                lit(&sub_name)
                            ));
                        }
                        None => {
                            subs.push_str(&format!("            {c},\n"));
                            arms.push_str(&format!(
                                "            {} => Ok({name}::{vn}),\n",
                                lit(&sub_name)
                            ));
                        }
                    }
                }
                out.push_str(&format!(
                    "\nimpl {name} {{\n    fn __clap_subs() -> Vec<ClapCommand> {{\n        vec![\n{subs}        ]\n    }}\n    fn __clap_from_sub(name: &str, m: &ClapMatches) -> Result<{name}, ClapError> {{\n        let _ = m;\n        match name {{\n{arms}            _ => panic!(\"unknown subcommand\"),\n        }}\n    }}\n}}\n"
                ));
            }
        }
        ast::Adt::Union(_) => {}
    }
    let end = adt.syntax().text_range().end();
    edits.push((TextRange::new(end, end), out));
}
