//! User-defined types: structs and enums, their derives and `impl` blocks. The type checker builds
//! the table; codegen reads it to emit classes and lower field access, methods and patterns.

use std::collections::HashMap;

use ra_ap_syntax::{
    AstNode, SyntaxKind, ast, ast::HasAttrs, ast::HasGenericParams, ast::HasName,
    ast::HasTypeBounds,
};

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
        self.derives(t)
            || self.manual.iter().any(|m| m == t)
            || (self.display && matches!(t, "Display" | "ToString"))
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
    /// Which parameters are `impl Into<X>` / `T: Into<X>`.
    pub into_params: Vec<bool>,
    /// The type arguments of the `impl`'s self type (`impl<U> Stack<U>` → `[U]`), which map a
    /// receiver's arguments onto the signature.
    pub impl_args: Vec<Ty>,
    /// The method's own type parameters (`fn map<U>`), instantiated at each call.
    pub type_params: Vec<String>,
    /// None for associated functions (`Point::new`).
    pub receiver: Option<Receiver>,
}

impl Method {
    /// The type parameters a method can take classes for (`T::make()` in its body): its own,
    /// and for an associated function (no `self`) also its `impl`'s, which a call names
    /// (`Stack::<T>::make()`) or infers.
    pub fn dict_names(&self) -> Vec<String> {
        let mut out = self.type_params.clone();
        if self.receiver.is_none() {
            for a in &self.impl_args {
                if let Ty::Param(p) = a
                    && !out.contains(p)
                {
                    out.push(p.clone());
                }
            }
        }
        out
    }
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
    /// Their types.
    pub const_tys: HashMap<String, Ty>,
    /// The ones with a default value (`const N: usize = 3;`), which impls may leave out.
    pub const_defaults: Vec<String>,
    /// The module it was imported from; None when declared in this file.
    pub module: Option<String>,
    /// Associated types (`type Unit;`), which each impl names (`type Unit = f64;`).
    pub assoc_types: Vec<String>,
    /// Type parameters (`trait Convert<T>`).
    pub type_params: Vec<String>,
    /// Each associated type's bounds (`type Unit: Debug + Copy;`).
    pub assoc_bounds: HashMap<String, Vec<String>>,
}

/// A struct's const generic parameter (`struct Buf<const N: usize>`). Instances keep its value
/// as `$N`, which the constructor takes last (or reads off a field, `[T; N]`'s length).
#[derive(Clone, Debug)]
pub struct ConstParam {
    pub name: String,
    /// `const N: usize = 4`: the default, as written.
    pub default: Option<String>,
    /// The field it can be read from, and how (see `const_path`).
    pub derive: Option<(usize, String)>,
}

#[derive(Default, Clone, Debug)]
pub struct Adts {
    pub defs: HashMap<String, AdtDef>,
    /// Structs' const generic parameters, in order, by struct.
    pub const_params: HashMap<String, Vec<ConstParam>>,
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
    /// Blanket impls (`impl<T: Display> Shout for T`): the trait's bounds on `T`, by trait. Their
    /// methods are under the key `Shout$all`.
    pub blankets: HashMap<String, Vec<String>>,
    /// Each impl's associated types (`type Unit = f64;`), by (type, trait).
    pub impl_assoc: HashMap<(String, String), HashMap<String, Ty>>,
    /// The user traits each type implements.
    pub trait_impls: HashMap<String, Vec<String>>,
    /// Names of loaded modules (`mod name;`), which paths may start with.
    pub modules: std::collections::HashSet<String>,
    /// `impl From<X> for Y` blocks, by `Y`.
    pub from_impls: HashMap<String, Vec<FromImpl>>,
    /// A generic trait implemented more than once for a type (`impl IntoS<u8> for Vec<u8>` and
    /// `impl IntoS<u16> for Vec<u8>`): each method's keys by `"Type::method"`, the first impl's
    /// under its own name, the others' as `method$1`, ..
    pub method_alts: HashMap<String, Vec<String>>,
    /// Those keys by the absolute offset of the method's name.
    pub alt_keys_at: HashMap<usize, String>,
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
    /// `impl Iterator for T { type Item = X; .. }`: `X`, by `T`. Also `impl IntoIterator`.
    pub iter_items: HashMap<String, Ty>,
    /// Types with `impl IntoIterator` (iterated through their `into_iter()`, not `next()`).
    pub into_iters: std::collections::HashSet<String>,
    /// `impl Future for T { type Output = X; .. }`: `X`, by `T`. Such a value is awaited (and
    /// passed as an `F: Future`) by polling it (`κσ.$drive`).
    pub future_outputs: HashMap<String, Ty>,
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
/// `char`, `String` (also for `str`), `Vec`, `Option`, `Result`, `HashMap`, `HashSet`,
/// `BTreeMap`, `BTreeSet`.
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
        "i128" => "i128",
        "u128" => "u128",
        "f32" => "f32",
        "f64" => "f64",
        "bool" => "bool",
        "char" => "char",
        "str" | "String" => "String",
        "Vec" => "Vec",
        "Option" => "Option",
        "Result" => "Result",
        "HashMap" => "HashMap",
        "HashSet" => "HashSet",
        "BTreeMap" => "BTreeMap",
        "BTreeSet" => "BTreeSet",
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
        Ty::Option(_) => Some("Option"),
        Ty::Result(..) => Some("Result"),
        Ty::Map(..) => Some("HashMap"),
        Ty::Set(_) => Some("HashSet"),
        Ty::Coll(crate::ty::Coll::BTreeMap, _) => Some("BTreeMap"),
        Ty::Coll(crate::ty::Coll::BTreeSet, _) => Some("BTreeSet"),
        _ => None,
    }
}

/// The type arguments of a built-in type a trait is implemented for (`HashMap<String, i64>` →
/// `[String, i64]`), matched against a receiver's.
pub fn ext_args(t: &Ty) -> Vec<Ty> {
    match t {
        Ty::Vec(e) | Ty::Option(e) | Ty::Set(e) => vec![(**e).clone()],
        Ty::Map(k, v) | Ty::Result(k, v) => vec![(**k).clone(), (**v).clone()],
        Ty::Coll(_, view) => ext_args(view),
        _ => vec![],
    }
}

/// The self type's name of `impl Trait for &str` and the like (through references).
pub fn impl_self_name(imp: &ast::Impl) -> Option<String> {
    let mut t = imp.self_ty()?;
    while let ast::Type::RefType(r) = t {
        t = r.ty()?;
    }
    match t {
        // `impl Tr for Box<str>` (or `Rc<[u8]>`, `Arc<String>`): a box is its contents in JS, so
        // the impl is the built-in type's.
        ast::Type::PathType(p)
            if p.path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_some_and(|n| matches!(n.text().to_string().as_str(), "Box" | "Rc" | "Arc"))
                && boxed_builtin(&p).is_some() =>
        {
            boxed_builtin(&p)
        }
        ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
        // `impl<const N: usize> Tr for [u8; N]`: arrays and slices are `Vec`s in JS.
        ast::Type::ArrayType(_) | ast::Type::SliceType(_) => Some("Vec".into()),
        _ => None,
    }
}

/// The built-in type in `Box<str>` / `Rc<[T]>` / `Arc<String>`: its extension key's name.
fn boxed_builtin(p: &ast::PathType) -> Option<String> {
    let seg = p.path()?.segment()?;
    let ast::GenericArg::TypeArg(a) = ast::HasGenericArgs::generic_arg_list(&seg)?
        .generic_args()
        .next()?
    else {
        return None;
    };
    let name = match a.ty()? {
        ast::Type::ArrayType(_) | ast::Type::SliceType(_) => "Vec".to_string(),
        ast::Type::PathType(inner) => inner.path()?.segment()?.name_ref()?.text().to_string(),
        _ => return None,
    };
    ext_key(&name).map(|_| name)
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
        "BitAndAssign" => "bitand_assign",
        "BitOrAssign" => "bitor_assign",
        "BitXorAssign" => "bitxor_assign",
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
    /// `x.try_into()` to an integer type: `κσ.$tryInt(x, bits, signed, big)`.
    TryInt { bits: u32, signed: bool, big: bool },
    /// `x.try_into()` through an `impl TryFrom<X> for Y`: `Y.try_from(x)`.
    TryFrom { adt: String },
    /// `v.try_into()` from a slice to an array: `Ok` of a copy (lengths aren't in the types).
    TryVec,
    /// `n.try_into()` to a `char`: `κσ.$tryChar(n)`.
    TryChar,
    /// A runtime function of the value (`?` into an `anyhow::Error`: `κσ.$anyhow.from(e)`).
    Wrap { js: String },
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
        if let Some(k) = self.alt_keys_at.get(&at) {
            return k.clone();
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
    "Serialize",
    "Deserialize",
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
    // (`Box<str>`: the built-in type, see `impl_self_name`.)
    let self_name = match imp.self_ty() {
        Some(ast::Type::PathType(p))
            if p.path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .is_some_and(|n| matches!(n.text().to_string().as_str(), "Box" | "Rc" | "Arc")) =>
        {
            boxed_builtin(&p).or_else(|| name_of(Some(ast::Type::PathType(p))))
        }
        t => name_of(t),
    };
    (name_of(imp.trait_()), self_name)
}

/// `impl<T: Bound> Trait for T`: (the trait, `T`, `T`'s bounds from the parameter list and the
/// `where` clause).
pub fn blanket_impl(imp: &ast::Impl) -> Option<(String, String, Vec<String>)> {
    let b = blanket_parts(imp)?;
    // (`impl<T: Tr> Tr for &T` is no blanket impl: it forwards, see `forwarding_impl`.)
    if b.2.contains(&b.0) && wrapped_param(imp).is_some() {
        return None;
    }
    Some(b)
}

fn blanket_parts(imp: &ast::Impl) -> Option<(String, String, Vec<String>)> {
    let (Some(tr), self_name) = impl_parts(imp) else {
        return None;
    };
    // (`for &T`, `for &mut I`, `for Box<T>`: references and boxes are erased in JS.)
    let self_name = match self_name {
        Some(n) if !matches!(n.as_str(), "Box" | "Rc" | "Arc") => n,
        _ => wrapped_param(imp)?,
    };
    let tp = imp
        .generic_param_list()?
        .generic_params()
        .find_map(|p| match p {
            ast::GenericParam::TypeParam(tp)
                if tp.name().is_some_and(|n| n.text() == self_name.as_str()) =>
            {
                Some(tp)
            }
            _ => None,
        })?;
    // (`impl<T> Trait for T` through a reference too, `for &T`.)
    let names = |l: ast::TypeBoundList| -> Vec<String> {
        l.bounds()
            .filter_map(|b| match b.ty()? {
                ast::Type::PathType(p) => Some(p.path()?.segment()?.name_ref()?.text().to_string()),
                _ => None,
            })
            .collect()
    };
    let mut bounds: Vec<String> = tp.type_bound_list().map(names).unwrap_or_default();
    if let Some(wc) = imp.where_clause() {
        for pred in wc.predicates() {
            if let Some(ast::Type::PathType(pt)) = pred.ty()
                && pt
                    .path()
                    .and_then(|p| p.as_single_name_ref())
                    .is_some_and(|n| n.text() == self_name.as_str())
                && let Some(l) = pred.type_bound_list()
            {
                bounds.extend(names(l));
            }
        }
    }
    Some((tr, self_name, bounds))
}

/// `impl<T> Tr for &T` / `&mut T` / `Box<T>` / `Rc<T>` / `Arc<T>`: `T`, a type parameter of the
/// impl (or `dyn Tr` for `Box<dyn Tr>`).
fn wrapped_param(imp: &ast::Impl) -> Option<String> {
    use ra_ap_syntax::ast::HasGenericArgs;
    let mut t = imp.self_ty()?;
    let mut wrapped = false;
    loop {
        t = match t {
            ast::Type::RefType(r) => r.ty()?,
            ast::Type::PathType(ref p)
                if p.path()?.segment()?.name_ref().is_some_and(|n| {
                    matches!(n.text().to_string().as_str(), "Box" | "Rc" | "Arc")
                }) =>
            {
                let args: Vec<ast::GenericArg> = p
                    .path()?
                    .segment()?
                    .generic_arg_list()?
                    .generic_args()
                    .collect();
                match args.as_slice() {
                    [ast::GenericArg::TypeArg(ta)] => ta.ty()?,
                    _ => return None,
                }
            }
            ast::Type::PathType(p) if wrapped => {
                let n = p.path()?.as_single_name_ref()?.text().to_string();
                let is_param = imp.generic_param_list()?.generic_params().any(|g| {
                    matches!(g, ast::GenericParam::TypeParam(tp)
                        if tp.name().is_some_and(|x| x.text() == n.as_str()))
                });
                return is_param.then_some(n);
            }
            // (`impl Tr for Box<dyn Tr>`: "dyn Tr".)
            ast::Type::DynTraitType(d) if wrapped => {
                let b = d.type_bound_list()?.bounds().next()?;
                let ast::Type::PathType(p) = b.ty()? else {
                    return None;
                };
                return Some(format!("dyn {}", p.path()?.segment()?.name_ref()?.text()));
            }
            _ => return None,
        };
        wrapped = true;
    }
}

/// `impl<T: Tr + ?Sized> Tr for &T { fn f(&self) { (**self).f() } }` (or for `&mut T`,
/// `Box<T>`): every method hands the call on to `T`'s impl. References and boxes are erased in
/// JS, so a value of `&T` already is a `T` with that impl: the impl is left out.
fn forwarding_impl(imp: &ast::Impl) -> bool {
    if !wrapping_impl(imp) {
        return false;
    }
    let Some(items) = imp.assoc_item_list() else {
        return true;
    };
    items.assoc_items().all(|item| match item {
        ast::AssocItem::Fn(f) => forwards(&f),
        ast::AssocItem::TypeAlias(_) | ast::AssocItem::Const(_) => true,
        ast::AssocItem::MacroCall(_) => false,
    })
}

/// `impl<T: Tr> Tr for &T` (or `&mut T`, `Box<T>`, ..): the trait for a wrapper of `T` given
/// `T`'s impl of it.
pub fn wrapping_impl(imp: &ast::Impl) -> bool {
    match wrapped_param(imp) {
        Some(d) if d.starts_with("dyn ") => impl_parts(imp).0.is_some_and(|tr| d[4..] == tr),
        Some(_) => blanket_parts(imp).is_some_and(|(tr, _, bounds)| bounds.contains(&tr)),
        None => false,
    }
}

/// Whether `f`'s body is just the same method of what `self` points at, with `f`'s parameters:
/// `(**self).f(a, b)`, `T::f(self, a)`, `Tr::f(&**self, a)`.
fn forwards(f: &ast::Fn) -> bool {
    use ra_ap_syntax::ast::HasArgList;
    fn strip(mut e: ast::Expr) -> Option<ast::Expr> {
        loop {
            e = match e {
                ast::Expr::ParenExpr(p) => p.expr()?,
                ast::Expr::RefExpr(r) => r.expr()?,
                ast::Expr::PrefixExpr(p) if p.op_kind() == Some(ast::UnaryOp::Deref) => p.expr()?,
                ast::Expr::MethodCallExpr(m)
                    if m.arg_list().is_some_and(|l| l.args().next().is_none())
                        && m.name_ref().is_some_and(|n| {
                            matches!(
                                n.text().to_string().as_str(),
                                "as_ref"
                                    | "as_mut"
                                    | "deref"
                                    | "deref_mut"
                                    | "borrow"
                                    | "borrow_mut"
                            )
                        }) =>
                {
                    m.receiver()?
                }
                other => return Some(other),
            };
        }
    }
    let path_name = |e: ast::Expr| -> Option<String> {
        match strip(e)? {
            ast::Expr::PathExpr(p) => Some(p.path()?.as_single_name_ref()?.text().to_string()),
            _ => None,
        }
    };
    let Some(name) = f.name().map(|n| n.text().to_string()) else {
        return false;
    };
    let Some(pl) = f.param_list() else {
        return false;
    };
    let params: Vec<String> = pl
        .params()
        .map(|p| match p.pat() {
            Some(ast::Pat::IdentPat(ip)) => ip.name().map(|n| n.text().to_string()),
            _ => None,
        })
        .collect::<Option<_>>()
        .unwrap_or_else(|| vec![String::new()]);
    let Some(body) = f.body().and_then(|b| b.stmt_list()) else {
        return false;
    };
    // (The call is the tail, or the only statement: `(**self).consume(buf);`.)
    let stmts: Vec<ast::Stmt> = body.statements().collect();
    let tail = match (body.tail_expr(), stmts.as_slice()) {
        (Some(t), []) => t,
        (None, [ast::Stmt::ExprStmt(s)]) => match s.expr() {
            Some(e) => e,
            None => return false,
        },
        _ => return false,
    };
    // (Through parentheses and `unsafe { .. }`.)
    let mut tail = tail;
    loop {
        tail = match tail {
            ast::Expr::ParenExpr(p) => match p.expr() {
                Some(e) => e,
                None => return false,
            },
            ast::Expr::BlockExpr(b)
                if b.label().is_none()
                    && matches!(b.modifier(), None | Some(ast::BlockModifier::Unsafe(_)))
                    && b.stmt_list()
                        .is_some_and(|l| l.statements().next().is_none()) =>
            {
                match b.stmt_list().and_then(|l| l.tail_expr()) {
                    Some(e) => e,
                    None => return false,
                }
            }
            _ => break,
        };
    }
    let (callee, recv, args): (String, Option<ast::Expr>, Vec<ast::Expr>) = match tail {
        ast::Expr::MethodCallExpr(m) => {
            if pl.self_param().is_none() {
                return false;
            }
            let Some(n) = m.name_ref() else { return false };
            (
                n.text().to_string(),
                m.receiver(),
                m.arg_list().into_iter().flat_map(|l| l.args()).collect(),
            )
        }
        ast::Expr::CallExpr(c) => {
            let Some(ast::Expr::PathExpr(p)) = c.expr() else {
                return false;
            };
            let Some(n) = p
                .path()
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
            else {
                return false;
            };
            let mut args = c.arg_list().into_iter().flat_map(|l| l.args());
            let recv = if pl.self_param().is_some() {
                args.next()
            } else {
                None
            };
            (n.text().to_string(), recv, args.collect())
        }
        _ => return false,
    };
    callee == name
        && (pl.self_param().is_none() || recv.and_then(path_name).as_deref() == Some("self"))
        && args.len() == params.len()
        && args
            .into_iter()
            .zip(&params)
            .all(|(a, p)| path_name(a).as_deref() == Some(p.as_str()))
}

/// The default methods of trait `tr` (declared in `items`) that a forwarding impl leaves to the
/// default while another impl of `tr` overrides them: then `&T`'s method isn't `T`'s.
fn shadowed_defaults(imp: &ast::Impl, tr: &str, items: &[ast::Item]) -> bool {
    let fn_names = |l: Option<ast::AssocItemList>| -> Vec<String> {
        l.into_iter()
            .flat_map(|l| l.assoc_items())
            .filter_map(|i| match i {
                ast::AssocItem::Fn(f) => f.name().map(|n| n.text().to_string()),
                _ => None,
            })
            .collect()
    };
    let own = fn_names(imp.assoc_item_list());
    let defaults: Vec<String> = items
        .iter()
        .filter_map(|i| match i {
            ast::Item::Trait(t) if t.name().is_some_and(|n| n.text() == tr) => Some(t),
            _ => None,
        })
        .flat_map(|t| t.assoc_item_list())
        .flat_map(|l| l.assoc_items())
        .filter_map(|i| match i {
            ast::AssocItem::Fn(f) if f.body().is_some() => f.name().map(|n| n.text().to_string()),
            _ => None,
        })
        .filter(|n| !own.contains(n))
        .collect();
    !defaults.is_empty()
        && items.iter().any(|i| match i {
            ast::Item::Impl(other) if other.syntax() != imp.syntax() => {
                impl_parts(other).0.as_deref() == Some(tr)
                    && fn_names(other.assoc_item_list())
                        .iter()
                        .any(|n| defaults.contains(n))
            }
            _ => false,
        })
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
        // `impl<T: Tr> Tr for &T` handing every call on to `T`'s impl: nothing to do in JS.
        let items: Vec<ast::Item> = items
            .iter()
            .filter(|i| match i {
                ast::Item::Impl(imp) => {
                    !(forwarding_impl(imp)
                        && !impl_parts(imp)
                            .0
                            .is_some_and(|tr| shadowed_defaults(imp, &tr, &items)))
                }
                _ => true,
            })
            .cloned()
            .collect();
        // `impl Tr for String` next to `impl Tr for str` (or `Vec<T>` next to `[T]`): one type in
        // JS, so the borrowed type's impl serves and the owned one (which usually hands on to it,
        // `self.as_str().f()`) goes.
        let twin = |imp: &ast::Impl| -> Option<(String, bool)> {
            let tr = imp.trait_()?.syntax().text().to_string();
            let mut t = imp.self_ty()?;
            while let ast::Type::RefType(r) = t {
                t = r.ty()?;
            }
            let owned = match &t {
                ast::Type::PathType(p) => {
                    let n = p.path()?.segment()?.name_ref()?.text().to_string();
                    match n.as_str() {
                        "String" | "Vec" => true,
                        "str" => false,
                        _ => return None,
                    }
                }
                ast::Type::SliceType(_) | ast::Type::ArrayType(_) => false,
                _ => return None,
            };
            let kind = match &t {
                ast::Type::PathType(p) if p.syntax().text().to_string().starts_with("Vec") => "seq",
                ast::Type::SliceType(_) | ast::Type::ArrayType(_) => "seq",
                _ => "str",
            };
            Some((format!("{tr}:{kind}"), owned))
        };
        let borrowed: Vec<String> = items
            .iter()
            .filter_map(|i| match i {
                ast::Item::Impl(imp) => twin(imp).filter(|(_, owned)| !owned).map(|(k, _)| k),
                _ => None,
            })
            .collect();
        let items: Vec<ast::Item> = items
            .into_iter()
            .filter(|i| match i {
                ast::Item::Impl(imp) => {
                    !twin(imp).is_some_and(|(k, owned)| owned && borrowed.contains(&k))
                }
                _ => true,
            })
            .collect();
        *last.borrow_mut() = Some((file.syntax().clone(), items.clone()));
        items
    })
}

/// How to read the const parameter `n` back from a value of type `ty` at run time: a JS suffix
/// for the value's expression (`?.length` for `[T; N]`, `?.[0]?.length` for `[[T; N]; M]`,
/// `?.$N` for a struct `Buf<N>`). None when the type doesn't hold it.
pub fn const_path(ty: &ast::Type, n: &str, adts: &Adts) -> Option<String> {
    use ra_ap_syntax::ast::HasGenericArgs;
    match ty {
        ast::Type::RefType(r) => const_path(&r.ty()?, n, adts),
        ast::Type::ParenType(p) => const_path(&p.ty()?, n, adts),
        ast::Type::ArrayType(a) => {
            if a.const_arg()
                .and_then(|c| c.expr())
                .is_some_and(|e| names_const(&e, n))
            {
                return Some("?.length".into());
            }
            Some(format!("?.[0]{}", const_path(&a.ty()?, n, adts)?))
        }
        ast::Type::SliceType(s) => Some(format!("?.[0]{}", const_path(&s.ty()?, n, adts)?)),
        ast::Type::TupleType(t) => t
            .fields()
            .enumerate()
            .find_map(|(i, f)| Some(format!("?.[{i}]{}", const_path(&f, n, adts)?))),
        ast::Type::PathType(p) => {
            let seg = p.path()?.segment()?;
            let name = seg.name_ref()?.text().to_string();
            let args: Vec<ast::GenericArg> = seg
                .generic_arg_list()
                .into_iter()
                .flat_map(|l| l.generic_args())
                .filter(|g| !matches!(g, ast::GenericArg::LifetimeArg(_)))
                .collect();
            let inner = |g: &ast::GenericArg| match g {
                ast::GenericArg::TypeArg(t) => const_path(&t.ty()?, n, adts),
                _ => None,
            };
            if let Some(cps) = adts.const_params.get(&name) {
                let def = adts.defs.get(&name)?;
                return args.iter().enumerate().find_map(|(i, g)| {
                    let pn = def.type_params.get(i)?;
                    (cps.iter().any(|c| &c.name == pn) && generic_arg_names(g, n))
                        .then(|| format!("?.${pn}"))
                });
            }
            match name.as_str() {
                // (Erased: the value itself, or null.)
                "Box" | "Rc" | "Arc" | "Option" => args.iter().find_map(inner),
                "Vec" => Some(format!("?.[0]{}", args.iter().find_map(inner)?)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether a const argument is just the const parameter `n` (`N` or `{ N }`).
pub fn names_const(e: &ast::Expr, n: &str) -> bool {
    match e {
        ast::Expr::PathExpr(p) => p
            .path()
            .and_then(|p| p.as_single_name_ref())
            .is_some_and(|x| x.text() == n),
        ast::Expr::BlockExpr(b) if b.statements().next().is_none() => {
            b.tail_expr().is_some_and(|t| names_const(&t, n))
        }
        _ => false,
    }
}

/// Whether a generic argument is the const parameter `n` (`Buf<N>` parses `N` as a type).
pub fn generic_arg_names(g: &ast::GenericArg, n: &str) -> bool {
    match g {
        ast::GenericArg::TypeArg(t) => matches!(t.ty(), Some(ast::Type::PathType(p))
            if p.path().and_then(|p| p.as_single_name_ref()).is_some_and(|x| x.text() == n)),
        ast::GenericArg::ConstArg(c) => c.expr().is_some_and(|e| names_const(&e, n)),
        _ => false,
    }
}
