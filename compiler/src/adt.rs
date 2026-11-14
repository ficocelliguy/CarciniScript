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
    /// The JS property: the name, or `name$f` when a method of the type has the same name
    /// (a JS instance property would hide the prototype's method).
    pub js: String,
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
    /// Type parameters (`struct Stack<T>`): field types refer to them as `Ty::Param`.
    pub type_params: Vec<String>,
    /// std traits implemented by hand (`impl Ord for Task`): `Ord`, `PartialOrd`, `PartialEq`,
    /// `Eq`, `Clone`.
    pub manual: Vec<String>,
}

impl AdtDef {
    pub fn derives(&self, t: &str) -> bool {
        self.derives.iter().any(|d| d == t)
    }

    /// Derived or implemented by hand.
    pub fn implements(&self, t: &str) -> bool {
        self.derives(t) || self.manual.iter().any(|m| m == t)
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
    /// The type arguments of the `impl`'s self type (`impl<U> Stack<U>` → `[U]`), which map a
    /// receiver's arguments onto the signature.
    pub impl_args: Vec<Ty>,
    /// The method's own type parameters (`fn map<U>`), instantiated at each call.
    pub type_params: Vec<String>,
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
    /// Required associated constants (`const SCALE: f64;`).
    pub consts: Vec<String>,
    /// The module it was imported from; None when declared in this file.
    pub module: Option<String>,
}

#[derive(Default, Clone, Debug)]
pub struct Adts {
    pub defs: HashMap<String, AdtDef>,
    /// Enum variants under a `#[cfg(..)]` (by enum, then variant index): they may not exist, so
    /// exhaustiveness checking never asks for them.
    pub cfg_variants: std::collections::HashSet<(String, usize)>,
    /// `impl Deref for T { type Target = X; .. }`: `X`, by `T`.
    pub deref_targets: HashMap<String, Ty>,
    /// Types with an `impl DerefMut`.
    pub deref_mut: std::collections::HashSet<String>,
    /// Methods and associated functions (inherent, from trait impls, and inherited trait
    /// defaults), by type then method name.
    pub methods: HashMap<String, HashMap<String, Method>>,
    /// User-defined traits.
    pub traits: HashMap<String, TraitDef>,
    /// The user traits each type implements.
    pub trait_impls: HashMap<String, Vec<String>>,
    /// Names of loaded modules (`mod name;`), which paths may start with.
    pub modules: std::collections::HashSet<String>,
    /// `impl From<X> for Y` blocks, by `Y`.
    pub from_impls: HashMap<String, Vec<FromImpl>>,
    /// `impl Add<R> for T` and the other `std::ops` traits, by `T`.
    pub op_impls: HashMap<String, Vec<OpImpl>>,
    /// Associated constants (`impl Grid { const W: usize = 3; }`), by type then name.
    pub assoc_consts: HashMap<String, HashMap<String, Ty>>,
    /// Built-in types with user trait impls (`impl Money for f64`), by `ext_key`. Their methods
    /// are in `methods` under that key.
    pub ext_types: std::collections::HashSet<String>,
    /// `impl fmt::Debug for T`: the absolute offset of its `fmt`'s name, by `T`. That `fmt` is
    /// stored (and emitted) as `fmt$dbg`, apart from a `Display` impl's `fmt`.
    pub debug_fmt: HashMap<String, usize>,
    /// `impl Iterator for T { type Item = X; .. }`: `X`, by `T`.
    pub iter_items: HashMap<String, Ty>,
}

/// Methods every struct or enum has (derived or from the runtime), which a custom iterator's
/// value doesn't forward to its iterator.
pub fn own_method(m: &str) -> bool {
    matches!(
        m,
        "clone"
            | "to_string"
            | "eq"
            | "ne"
            | "cmp"
            | "partial_cmp"
            | "lt"
            | "le"
            | "gt"
            | "ge"
            | "fmt"
            | "hash"
    )
}

/// The key a built-in type's extension-trait methods are stored under: `i32`, `f64`, `bool`,
/// `char`, `String` (also for `str`), `Vec`.
pub fn ext_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "i8" => "i8",
        "i16" => "i16",
        "i32" => "i32",
        "i64" => "i64",
        "isize" => "isize",
        "u8" => "u8",
        "u16" => "u16",
        "u32" => "u32",
        "u64" => "u64",
        "usize" => "usize",
        "f32" => "f32",
        "f64" => "f64",
        "bool" => "bool",
        "char" => "char",
        "str" | "String" => "String",
        "Vec" => "Vec",
        _ => return None,
    })
}

/// `ext_key` for a value of type `t`.
pub fn ext_key_of(t: &Ty) -> Option<&'static str> {
    match t {
        Ty::Int(k) => ext_key(&format!("{k:?}").to_lowercase()),
        Ty::IntLit => Some("i32"),
        Ty::Float { f32: true } => Some("f32"),
        Ty::Float { f32: false } => Some("f64"),
        Ty::Bool => Some("bool"),
        Ty::Char => Some("char"),
        Ty::Str => Some("String"),
        Ty::Vec(_) => Some("Vec"),
        _ => None,
    }
}

/// The self type's name of `impl Trait for &str` and the like (through references).
pub fn impl_self_name(imp: &ast::Impl) -> Option<String> {
    let mut t = imp.self_ty()?;
    while let ast::Type::RefType(r) = t {
        t = r.ty()?;
    }
    match t {
        ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
        _ => None,
    }
}

/// One `impl Add<R> for T` (or `Sub`, `Neg`, `AddAssign`, ..): its method is `key` (`add$0`).
#[derive(Clone, Debug)]
pub struct OpImpl {
    /// The trait's name: `Add`, `AddAssign`, `Neg`, ...
    pub trait_name: String,
    /// The right operand's type (`Self` when the trait has no argument; unused for `Neg`/`Not`).
    pub rhs: Ty,
    pub key: String,
    /// Absolute offset of the method's name.
    pub at: usize,
}

/// The method of a `std::ops` trait, for the traits CarciniScript overloads.
pub fn op_method(trait_name: &str) -> Option<&'static str> {
    Some(match trait_name {
        "Add" => "add",
        "Sub" => "sub",
        "Mul" => "mul",
        "Div" => "div",
        "Rem" => "rem",
        "BitAnd" => "bitand",
        "BitOr" => "bitor",
        "BitXor" => "bitxor",
        "Neg" => "neg",
        "Not" => "not",
        "AddAssign" => "add_assign",
        "SubAssign" => "sub_assign",
        "MulAssign" => "mul_assign",
        "DivAssign" => "div_assign",
        "RemAssign" => "rem_assign",
        "Index" => "index",
        "IndexMut" => "index_mut",
        _ => return None,
    })
}

/// One `impl From<X> for Y`: its `from` is the static method `key`.
#[derive(Clone, Debug)]
pub struct FromImpl {
    pub src: Ty,
    pub key: String,
    /// Absolute offset of the `from` function's name.
    pub at: usize,
}

/// What a `Y::from(x)`, `x.into()` or `?` converts with.
#[derive(Clone, Debug)]
pub enum Conv {
    /// Nothing to do in JS (`&str` -> `String`, numeric widening).
    Identity,
    /// `Y.key(x)`.
    From { adt: String, key: String },
    /// An overloaded operator: `lhs.key(rhs)` (or `operand.key()` for `-x` / `!x`).
    Method { key: String },
    /// A user trait method on a built-in type: `$ext$key.prototype.method.call(recv, ..)`.
    Ext { key: String },
}

impl Adts {
    pub fn method(&self, ty: &str, name: &str) -> Option<&Method> {
        self.methods.get(ty)?.get(name)
    }

    /// The key a method is stored (and emitted) under: its name, except for the `from` of a
    /// `From` impl, whose name is at `at`.
    pub fn method_key(&self, ty: &str, name: &str, at: usize) -> String {
        if name == "from"
            && let Some(i) = self
                .from_impls
                .get(ty)
                .and_then(|v| v.iter().find(|i| i.at == at))
        {
            return i.key.clone();
        }
        if let Some(i) = self
            .op_impls
            .get(ty)
            .and_then(|v| v.iter().find(|i| i.at == at))
        {
            return i.key.clone();
        }
        if name == "fmt" && self.debug_fmt.get(ty) == Some(&at) {
            return "fmt$dbg".into();
        }
        name.to_string()
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
        let mut segs = &segs[skip..];
        // `std::cmp::Reverse(x)` is the built-in `Reverse`.
        if let [std, cmp, rest @ ..] = segs
            && matches!(std.as_str(), "std" | "core")
            && cmp == "cmp"
            && rest.first().is_some_and(|r| r == "Reverse")
        {
            segs = rest;
        }
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

/// Whether `node` is (inside) an item that only exists in test builds: `#[cfg(test)]` or
/// `#[test]`. Like `rustc` without `--test`, the compiler leaves those out.
pub fn test_only(node: &ra_ap_syntax::SyntaxNode) -> bool {
    node.ancestors().filter_map(ast::Item::cast).any(|item| {
        let attrs: Vec<ast::Attr> = match &item {
            ast::Item::Fn(f) => f.attrs().collect(),
            ast::Item::Module(m) => m.attrs().collect(),
            ast::Item::Impl(i) => i.attrs().collect(),
            ast::Item::Struct(s) => s.attrs().collect(),
            ast::Item::Enum(e) => e.attrs().collect(),
            ast::Item::Use(u) => u.attrs().collect(),
            ast::Item::Const(k) => k.attrs().collect(),
            _ => return false,
        };
        attrs.iter().any(|a| {
            let text: String = a.syntax().text().to_string().split_whitespace().collect();
            text == "#[test]" || text == "#[cfg(test)]"
        })
    })
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
            .filter(|n| !test_only(n))
            .filter_map(ast::Item::cast)
            .collect();
        *last.borrow_mut() = Some((file.syntax().clone(), items.clone()));
        items
    })
}
