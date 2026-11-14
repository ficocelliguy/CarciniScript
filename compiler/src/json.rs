//! serde's attributes for `JSON::stringify` / `JSON::parse`: `#[serde(rename = "..")]`,
//! `#[serde(rename_all = "camelCase")]`, `#[serde(skip)]`, `#[serde(default)]` on the structs
//! and enums of a file. (`#[derive(Serialize, Deserialize)]` itself is accepted and not needed:
//! JSON goes by the static type.)

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SourceFile, SyntaxKind,
    ast::{self, HasAttrs, HasName},
};

/// A field's attributes.
#[derive(Default, Clone, Debug)]
pub struct FieldAttrs {
    pub rename: Option<String>,
    pub skip_serializing: bool,
    pub skip_deserializing: bool,
    pub default: bool,
}

/// A struct's or enum's attributes, and its fields' and variants'.
#[derive(Default, Clone, Debug)]
pub struct SerdeInfo {
    pub rename_all: Option<String>,
    /// By variant name (`""` for a struct's own fields), then field name.
    pub fields: HashMap<(String, String), FieldAttrs>,
    /// A variant's `rename`, and its own `rename_all` (for a struct variant's fields).
    pub variants: HashMap<String, (Option<String>, Option<String>)>,
}

/// `key = "value"` and bare `key` items of the `#[serde(..)]` attributes.
fn serde_items(item: &impl HasAttrs) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for a in item.attrs() {
        if a.path().is_none_or(|p| p.syntax().text() != "serde") {
            continue;
        }
        let Some(tt) = a.syntax().descendants().find_map(ast::TokenTree::cast) else {
            continue;
        };
        let toks: Vec<_> = tt
            .syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        let mut i = 1;
        while i + 1 < toks.len() {
            if toks[i].kind() == SyntaxKind::IDENT {
                let key = toks[i].text().to_string();
                if toks[i + 1].kind() == SyntaxKind::EQ
                    && let Some(v) = toks.get(i + 2)
                {
                    let value = v.text().trim_matches('"').to_string();
                    out.push((key, Some(value)));
                    i += 3;
                    continue;
                }
                out.push((key, None));
            }
            i += 1;
        }
    }
    out
}

fn field_attrs(f: &impl HasAttrs) -> Option<FieldAttrs> {
    let items = serde_items(f);
    if items.is_empty() {
        return None;
    }
    let mut out = FieldAttrs::default();
    for (k, v) in items {
        match (k.as_str(), v) {
            ("rename", Some(v)) => out.rename = Some(v),
            ("skip", None) => {
                out.skip_serializing = true;
                out.skip_deserializing = true;
            }
            ("skip_serializing", None) => out.skip_serializing = true,
            ("skip_deserializing", None) => out.skip_deserializing = true,
            ("default", _) => out.default = true,
            _ => {}
        }
    }
    Some(out)
}

fn record_fields(info: &mut SerdeInfo, variant: &str, list: Option<ast::FieldList>) {
    match list {
        Some(ast::FieldList::RecordFieldList(l)) => {
            for f in l.fields() {
                if let (Some(n), Some(a)) = (f.name(), field_attrs(&f)) {
                    info.fields
                        .insert((variant.to_string(), n.text().to_string()), a);
                }
            }
        }
        Some(ast::FieldList::TupleFieldList(l)) => {
            for (i, f) in l.fields().enumerate() {
                if let Some(a) = field_attrs(&f) {
                    info.fields.insert((variant.to_string(), i.to_string()), a);
                }
            }
        }
        None => {}
    }
}

/// The serde attributes of the file's structs and enums, by name.
pub fn collect(src: &str) -> HashMap<String, SerdeInfo> {
    let mut out = HashMap::new();
    if !src.contains("serde") {
        return out;
    }
    let file = SourceFile::parse(src, crate::macros::EDITION).tree();
    for item in file.syntax().descendants().filter_map(ast::Item::cast) {
        let mut info = SerdeInfo::default();
        let name = match &item {
            ast::Item::Struct(s) => {
                record_fields(&mut info, "", s.field_list());
                s.name()
            }
            ast::Item::Enum(e) => {
                for v in e.variant_list().into_iter().flat_map(|l| l.variants()) {
                    let Some(vn) = v.name() else { continue };
                    let vn = vn.text().to_string();
                    let mut rename = None;
                    let mut rename_all = None;
                    for (k, val) in serde_items(&v) {
                        match (k.as_str(), val) {
                            ("rename", Some(x)) => rename = Some(x),
                            ("rename_all", Some(x)) => rename_all = Some(x),
                            _ => {}
                        }
                    }
                    info.variants.insert(vn.clone(), (rename, rename_all));
                    record_fields(&mut info, &vn, v.field_list());
                }
                e.name()
            }
            _ => continue,
        };
        let Some(name) = name else { continue };
        let holder = ast::AnyHasAttrs::cast(item.syntax().clone());
        if let Some(h) = holder {
            for (k, v) in serde_items(&h) {
                if k == "rename_all"
                    && let Some(v) = v
                {
                    info.rename_all = Some(v);
                }
            }
        }
        out.insert(name.text().to_string(), info);
    }
    out
}

/// serde's `rename_all` on a field name (snake_case in Rust).
pub fn rename_field(name: &str, rule: Option<&str>) -> String {
    let pascal = || {
        name.split('_')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<String>()
    };
    match rule {
        Some("UPPERCASE") | Some("SCREAMING_SNAKE_CASE") => name.to_ascii_uppercase(),
        Some("PascalCase") => pascal(),
        Some("camelCase") => {
            let p = pascal();
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
                None => p,
            }
        }
        Some("kebab-case") => name.replace('_', "-"),
        Some("SCREAMING-KEBAB-CASE") => name.to_ascii_uppercase().replace('_', "-"),
        _ => name.to_string(),
    }
}

/// serde's `rename_all` on a variant name (PascalCase in Rust).
pub fn rename_variant(name: &str, rule: Option<&str>) -> String {
    let snake = || {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        }
        out
    };
    match rule {
        Some("lowercase") => name.to_ascii_lowercase(),
        Some("UPPERCASE") => name.to_ascii_uppercase(),
        Some("camelCase") => {
            let mut c = name.chars();
            match c.next() {
                Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        }
        Some("snake_case") => snake(),
        Some("SCREAMING_SNAKE_CASE") => snake().to_ascii_uppercase(),
        Some("kebab-case") => snake().replace('_', "-"),
        Some("SCREAMING-KEBAB-CASE") => snake().to_ascii_uppercase().replace('_', "-"),
        _ => name.to_string(),
    }
}
