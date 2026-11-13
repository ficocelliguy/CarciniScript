//! User-defined types: structs and enums, their derives and `impl` blocks. The type checker builds
//! the table; codegen reads it to emit classes and lower field access, methods and patterns.

use std::collections::HashMap;

use ra_ap_syntax::{AstNode, SyntaxKind, ast, ast::HasAttrs};

use crate::ty::Ty;

/// How a struct or variant declares its fields.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    /// `{ x: i32 }`
    Named,
    /// `(i32, String)`: fields are named `0`, `1`, ...
    Tuple,
    /// No fields.
    Unit,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub shape: Shape,
    pub fields: Vec<Field>,
    /// The value of `variant as i32` (enums without data).
    pub discr: i64,
}

impl Variant {
    pub fn field(&self, name: &str) -> Option<(usize, &Field)> {
        self.fields.iter().enumerate().find(|(_, f)| f.name == name)
    }
}

#[derive(Clone, Debug)]
pub struct AdtDef {
    pub name: String,
    pub is_enum: bool,
    /// A struct has exactly one variant, named like the struct.
    pub variants: Vec<Variant>,
    /// `#[derive(..)]` traits, by last path segment (`Debug`, `Clone`, ...).
    pub derives: Vec<String>,
    /// Has `impl Display`.
    pub display: bool,
    /// Has `impl Default` (written by hand).
    pub default_impl: bool,
    /// The `#[default]` variant, for `#[derive(Default)]` on an enum.
    pub default_variant: Option<usize>,
    /// The module (`mod name;`) it was imported from; None when declared in this file.
    pub module: Option<String>,
}

impl AdtDef {
    pub fn derives(&self, t: &str) -> bool {
        self.derives.iter().any(|d| d == t)
    }

    pub fn has_default(&self) -> bool {
        self.default_impl || self.derives("Default")
    }

    pub fn variant(&self, name: &str) -> Option<(usize, &Variant)> {
        self.variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == name)
    }

    /// Whether some variant carries data (enums without data compare by tag only).
    pub fn has_data(&self) -> bool {
        self.variants.iter().any(|v| !v.fields.is_empty())
    }
}

/// How a method takes `self`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Receiver {
    /// `&self`
    Ref,
    /// `&mut self`
    MutRef,
    /// `self` / `mut self`
    Value,
}

#[derive(Clone, Debug)]
pub struct Method {
    pub params: Vec<Ty>,
    pub ret: Ty,
    pub is_async: bool,
    /// Which parameters are `&mut` to a number, string, bool or Option (passed as cells).
    pub cell_params: Vec<bool>,
    /// None for associated functions (`Point::new`).
    pub receiver: Option<Receiver>,
}

#[derive(Clone, Debug)]
pub struct TraitMethod {
    pub name: String,
    /// `Self` appears as `Ty::Param("Self")`.
    pub sig: Method,
    pub has_default: bool,
}

#[derive(Clone, Debug)]
pub struct TraitDef {
    pub supers: Vec<String>,
    pub methods: Vec<TraitMethod>,
    /// The module it was imported from; None when declared in this file.
    pub module: Option<String>,
}

#[derive(Default, Clone, Debug)]
pub struct Adts {
    pub defs: HashMap<String, AdtDef>,
    /// Methods and associated functions (inherent, from trait impls, and inherited trait
    /// defaults), by type then method name.
    pub methods: HashMap<String, HashMap<String, Method>>,
    /// User-defined traits.
    pub traits: HashMap<String, TraitDef>,
    /// The user traits each type implements.
    pub trait_impls: HashMap<String, Vec<String>>,
    /// Names of loaded modules (`mod name;`), which paths may start with.
    pub modules: std::collections::HashSet<String>,
}

impl Adts {
    pub fn method(&self, ty: &str, name: &str) -> Option<&Method> {
        self.methods.get(ty)?.get(name)
    }

    /// What `segs` names: a struct (`Point`, `Self`), a variant (`Shape::Circle`,
    /// `Self::Circle`), or an associated function (`Point::new`). `self_ty` is the type of the
    /// enclosing `impl`.
    pub fn resolve(&self, self_ty: Option<&str>, segs: &[String]) -> Option<Resolved> {
        // `crate::util::Point` / `util::Point` name `Point`.
        let mut skip = 0;
        while skip + 1 < segs.len()
            && (matches!(segs[skip].as_str(), "crate" | "self")
                || self.modules.contains(&segs[skip]))
        {
            skip += 1;
        }
        let segs = &segs[skip..];
        let ty_name = |s: &str| -> Option<String> {
            if s == "Self" {
                self_ty.map(str::to_string)
            } else {
                self.defs.contains_key(s).then(|| s.to_string())
            }
        };
        match segs {
            [one] => {
                let n = ty_name(one)?;
                (!self.defs[&n].is_enum).then_some(Resolved::Ctor(n, 0))
            }
            [.., t, item] => {
                let n = ty_name(t)?;
                let def = &self.defs[&n];
                if def.is_enum
                    && let Some((i, _)) = def.variant(item)
                {
                    return Some(Resolved::Ctor(n, i));
                }
                Some(Resolved::Assoc(n, item.clone()))
            }
            [] => None,
        }
    }

    /// Display name of a constructor: `Point`, `Shape::Circle`.
    pub fn ctor_name(&self, adt: &str, v: usize) -> String {
        let def = &self.defs[adt];
        if def.is_enum {
            format!("{adt}::{}", def.variants[v].name)
        } else {
            adt.to_string()
        }
    }
}

/// What a path in expression or pattern position names.
pub enum Resolved {
    /// A struct, or an enum variant: (type name, variant index).
    Ctor(String, usize),
    /// An associated function or method: (type name, method name).
    Assoc(String, String),
}

/// The traits named in an item's `#[derive(..)]` attributes, by last path segment.
pub fn derives_of(item: &impl HasAttrs) -> Vec<String> {
    let mut out = Vec::new();
    for attr in item.attrs() {
        let text: String = attr
            .syntax()
            .text()
            .to_string()
            .split_whitespace()
            .collect();
        let Some(inner) = text
            .strip_prefix("#[derive(")
            .and_then(|t| t.strip_suffix(")]"))
        else {
            continue;
        };
        for d in inner.split(',') {
            let d = d.trim();
            if let Some(last) = d.rsplit("::").next().filter(|s| !s.is_empty()) {
                out.push(last.to_string());
            }
        }
    }
    out
}

/// The derives CarciniScript implements.
pub const KNOWN_DERIVES: &[&str] = &[
    "Debug",
    "Clone",
    "Copy",
    "PartialEq",
    "Eq",
    "Hash",
    "PartialOrd",
    "Ord",
    "Default",
];

/// `impl Trait for Type`: the trait's last path segment and the type's name.
pub fn impl_parts(imp: &ast::Impl) -> (Option<String>, Option<String>) {
    let name_of = |t: Option<ast::Type>| match t? {
        ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
        _ => None,
    };
    (name_of(imp.trait_()), name_of(imp.self_ty()))
}

/// The file's items, plus items declared inside function bodies (`fn main() { fn helper() {} }`).
/// Nested items can't use the function's locals, so they behave like top-level ones. Memoized
/// per file: the checker and codegen ask many times.
pub fn all_items(file: &ast::SourceFile) -> Vec<ast::Item> {
    thread_local! {
        static LAST: std::cell::RefCell<Option<(ra_ap_syntax::SyntaxNode, Vec<ast::Item>)>> =
            const { std::cell::RefCell::new(None) };
    }
    LAST.with(|last| {
        if let Some((node, items)) = &*last.borrow()
            && node == file.syntax()
        {
            return items.clone();
        }
        let items: Vec<ast::Item> = file
            .syntax()
            .descendants()
            .filter(|n| {
                n.parent().is_some_and(|p| {
                    matches!(p.kind(), SyntaxKind::SOURCE_FILE | SyntaxKind::STMT_LIST)
                })
            })
            .filter_map(ast::Item::cast)
            .collect();
        *last.borrow_mut() = Some((file.syntax().clone(), items.clone()));
        items
    })
}
