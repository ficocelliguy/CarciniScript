//! The spike's type lattice. Just enough to lower arithmetic, casts and formatting correctly;
//! phase 1 replaces this with a real checker over prelude signatures.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntKind {
    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,
    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,
}

impl IntKind {
    pub fn from_name(name: &str) -> Option<IntKind> {
        Some(match name {
            "i8" => IntKind::I8,
            "i16" => IntKind::I16,
            "i32" => IntKind::I32,
            "i64" => IntKind::I64,
            "i128" => IntKind::I128,
            "isize" => IntKind::Isize,
            "u8" => IntKind::U8,
            "u16" => IntKind::U16,
            "u32" => IntKind::U32,
            "u64" => IntKind::U64,
            "u128" => IntKind::U128,
            "usize" => IntKind::Usize,
            _ => return None,
        })
    }

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntKind::I8
                | IntKind::I16
                | IntKind::I32
                | IntKind::I64
                | IntKind::I128
                | IntKind::Isize
        )
    }

    /// Kinds that fit in 32 bits get exact wrapping semantics; wider kinds are plain JS numbers.
    pub fn is_narrow(self) -> bool {
        matches!(
            self,
            IntKind::I8 | IntKind::I16 | IntKind::I32 | IntKind::U8 | IntKind::U16 | IntKind::U32
        )
    }

    pub fn is_wide_unsupported(self) -> bool {
        matches!(self, IntKind::I128 | IntKind::U128)
    }

    /// Prefix/suffix that wrap a JS integer-valued number into this kind, or None for wide kinds,
    /// which are plain JS numbers. The inner expression is always parenthesized: JS's `>>>` and
    /// `&` bind tighter than `^`/`|`, so `x ^ y >>> 0` would wrap only `y`.
    pub fn wrap_parts(self) -> Option<(&'static str, &'static str)> {
        Some(match self {
            IntKind::I8 => ("((", ") << 24 >> 24)"),
            IntKind::I16 => ("((", ") << 16 >> 16)"),
            IntKind::I32 => ("((", ") | 0)"),
            IntKind::U8 => ("((", ") & 0xff)"),
            IntKind::U16 => ("((", ") & 0xffff)"),
            IntKind::U32 => ("((", ") >>> 0)"),
            _ => return None,
        })
    }

    pub fn min_max(self) -> (&'static str, &'static str) {
        match self {
            IntKind::I8 => ("-128", "127"),
            IntKind::I16 => ("-32768", "32767"),
            IntKind::I32 => ("-2147483648", "2147483647"),
            IntKind::U8 => ("0", "255"),
            IntKind::U16 => ("0", "65535"),
            IntKind::U32 => ("0", "4294967295"),
            IntKind::U64 | IntKind::Usize | IntKind::U128 => ("0", "Number.MAX_SAFE_INTEGER"),
            _ => ("Number.MIN_SAFE_INTEGER", "Number.MAX_SAFE_INTEGER"),
        }
    }
}

/// The std collections that are a `Ty::Coll` over the type they behave like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coll {
    /// A JS array; the view is `Vec<T>`.
    VecDeque,
    /// A JS array in std's heap order; the view is `Vec<T>`.
    BinaryHeap,
    /// `κσ.BTreeMap` (a `HashMap` kept in key order); the view is `HashMap<K, V>`.
    BTreeMap,
    /// `κσ.BTreeSet`; the view is `HashSet<T>`.
    BTreeSet,
}

impl Coll {
    pub fn from_name(name: &str) -> Option<Coll> {
        Some(match name {
            "VecDeque" => Coll::VecDeque,
            "BinaryHeap" => Coll::BinaryHeap,
            "BTreeMap" => Coll::BTreeMap,
            "BTreeSet" => Coll::BTreeSet,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Coll::VecDeque => "VecDeque",
            Coll::BinaryHeap => "BinaryHeap",
            Coll::BTreeMap => "BTreeMap",
            Coll::BTreeSet => "BTreeSet",
        }
    }

    /// `VecDeque<i32>` from the view's description `Vec<i32>`.
    pub fn describe(self, view: &str) -> String {
        let args = view.find('<').map(|i| &view[i..]).unwrap_or("");
        format!("{}{args}", self.name())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    Int(IntKind),
    /// An unsuffixed integer literal whose kind is decided by context (defaults to i32).
    IntLit,
    Float {
        f32: bool,
    },
    Bool,
    Char,
    Str,
    Unit,
    Never,
    /// A JS interface from the typed bindings (`NS`, `Server`, `Hacknet`, ...). Without bindings,
    /// `NS` is untyped: any method or field is allowed and snake_case maps to camelCase.
    Extern(String),
    Fn,
    /// The result of calling an `async fn`; `.await` unwraps it.
    Future(Box<Ty>),
    /// `Vec<T>`. Slices (`&[T]`) and fixed-size arrays (`[T; N]`) are represented the same way:
    /// all of them are JS arrays.
    Vec(Box<Ty>),
    /// `Option<T>`: `None` is `null` (and `undefined` from JS reads as `None`), `Some(x)` is `x`
    /// itself. `Option<Option<T>>` and `Option<()>` would be ambiguous, so they're rejected.
    Option(Box<Ty>),
    /// `(A, B, ...)` with at least one element (the empty tuple is `Unit`). A JS array.
    Tuple(Vec<Ty>),
    /// `HashSet<T>`: the runtime's `κσ.HashSet`, keyed by structural value.
    Set(Box<Ty>),
    /// `HashMap<K, V>`: the runtime's `κσ.HashMap`, keyed like `HashSet`.
    Map(Box<Ty>, Box<Ty>),
    /// What `map.entry(k)` returns; only usable with `.or_insert(..)` and friends.
    Entry(Box<Ty>, Box<Ty>),
    /// A closure or function value: `impl Fn(A, B) -> R`. A JS function.
    Closure {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    /// `std::cmp::Ordering`: -1 (`Less`), 0 (`Equal`), 1 (`Greater`).
    Ordering,
    /// A struct or enum declared in the file, with its type arguments (`Stack<i32>`): an
    /// instance of its JS class.
    Adt(String, Vec<Ty>),
    /// `Result<T, E>`: the runtime's `κσ.Ok(v)` / `κσ.Err(e)` (`$tag` 0 / 1, value at `[0]`).
    Result(Box<Ty>, Box<Ty>),
    /// `fmt::Formatter`: the buffer a `Display` impl writes into.
    Formatter,
    /// A generic function's type parameter, inside its body. Erased in JS.
    Param(String),
    /// `dyn Trait` / `impl Trait` / `Box<dyn Trait>`: any value whose type implements the trait.
    Dyn(String),
    /// `RefCell<T>` / `Cell<T>`: a `κσ.RefCell` holding the value at `.v`. (`Rc<T>` is erased.)
    RefCell(Box<Ty>),
    /// What `v.iter()` / `v.iter_mut()` / `v.into_iter()` return. Only usable as a `for` loop
    /// source until iterator adapters land.
    Iter {
        elem: Box<Ty>,
        mutable: bool,
    },
    /// `VecDeque<T>`, `BinaryHeap<T>`, `BTreeMap<K, V>`, `BTreeSet<T>`: the collection over the
    /// type it behaves like at run time (`Vec<T>`, `HashMap<K, V>`, `HashSet<T>`).
    Coll(Coll, Box<Ty>),
    /// A type-inference variable. Only exists inside the type checker; never reaches codegen.
    Var(u32),
    /// No type information (e.g. NS return values until phase 2 bindings). Lowered with plain JS
    /// semantics.
    Unknown,
}

impl Ty {
    /// The type with every `VecDeque` / `BTreeMap` / .. replaced by the type it behaves like at
    /// run time, which is all codegen needs outside constructors and kind-specific methods.
    pub fn view(&self) -> Ty {
        let b = |t: &Ty| Box::new(t.view());
        match self {
            Ty::Coll(_, v) => v.view(),
            Ty::Vec(e) => Ty::Vec(b(e)),
            Ty::Option(e) => Ty::Option(b(e)),
            Ty::Set(e) => Ty::Set(b(e)),
            Ty::Future(e) => Ty::Future(b(e)),
            Ty::RefCell(e) => Ty::RefCell(b(e)),
            Ty::Map(k, v) => Ty::Map(b(k), b(v)),
            Ty::Entry(k, v) => Ty::Entry(b(k), b(v)),
            Ty::Result(k, v) => Ty::Result(b(k), b(v)),
            Ty::Tuple(es) => Ty::Tuple(es.iter().map(Ty::view).collect()),
            Ty::Adt(n, args) => Ty::Adt(n.clone(), args.iter().map(Ty::view).collect()),
            Ty::Closure { params, ret } => Ty::Closure {
                params: params.iter().map(Ty::view).collect(),
                ret: b(ret),
            },
            Ty::Iter { elem, mutable } => Ty::Iter {
                elem: b(elem),
                mutable: *mutable,
            },
            other => other.clone(),
        }
    }

    pub fn from_name(name: &str) -> Ty {
        if let Some(k) = IntKind::from_name(name) {
            return Ty::Int(k);
        }
        match name {
            "f64" => Ty::Float { f32: false },
            "f32" => Ty::Float { f32: true },
            "bool" => Ty::Bool,
            "char" => Ty::Char,
            "String" | "str" => Ty::Str,
            "NS" => Ty::Extern("NS".into()),
            _ => Ty::Unknown,
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int(_) | Ty::IntLit | Ty::Float { .. })
    }

    /// Settle a literal type once it can't be refined further.
    pub fn default_lit(self) -> Ty {
        if self == Ty::IntLit {
            Ty::Int(IntKind::I32)
        } else {
            self
        }
    }

    /// Values that are immutable in JS (numbers, strings, booleans), so copying a container of
    /// them needs only a shallow copy.
    pub fn is_js_primitive(&self) -> bool {
        matches!(
            self,
            Ty::Int(_) | Ty::IntLit | Ty::Float { .. } | Ty::Bool | Ty::Char | Ty::Str
        )
    }

    /// JS expression for `T::default()`, for `unwrap_or_default`.
    pub fn default_js(&self) -> Option<&'static str> {
        Some(match self {
            Ty::Int(IntKind::I128 | IntKind::U128) => "0n",
            Ty::Int(_) | Ty::IntLit | Ty::Float { .. } => "0",
            Ty::Bool => "false",
            Ty::Str => "\"\"",
            Ty::Char => r#""\0""#,
            Ty::Vec(_) => "[]",
            Ty::Option(_) => "null",
            _ => return None,
        })
    }

    /// Option types the null representation can't express.
    pub fn has_ambiguous_option(&self) -> bool {
        match self {
            Ty::Option(inner) => {
                matches!(**inner, Ty::Option(_) | Ty::Unit) || inner.has_ambiguous_option()
            }
            Ty::Vec(e) | Ty::Future(e) | Ty::Set(e) | Ty::Coll(_, e) => e.has_ambiguous_option(),
            Ty::Tuple(es) => es.iter().any(Ty::has_ambiguous_option),
            Ty::Map(k, v) => k.has_ambiguous_option() || v.has_ambiguous_option(),
            Ty::Iter { elem, .. } => elem.has_ambiguous_option(),
            _ => false,
        }
    }

    /// Whether a float appears anywhere in the type: such types aren't `Ord` or `Hash`.
    /// The types directly inside this one (`Vec<T>` → `[T]`).
    pub fn inner_types(&self) -> Vec<Ty> {
        match self {
            Ty::Vec(e) | Ty::Option(e) | Ty::Set(e) | Ty::Future(e) | Ty::RefCell(e) => {
                vec![(**e).clone()]
            }
            Ty::Iter { elem, .. } => vec![(**elem).clone()],
            Ty::Tuple(es) => es.clone(),
            Ty::Map(k, v) | Ty::Entry(k, v) | Ty::Result(k, v) => {
                vec![(**k).clone(), (**v).clone()]
            }
            Ty::Closure { params, ret } => [params.clone(), vec![(**ret).clone()]].concat(),
            Ty::Adt(_, args) => args.clone(),
            // The elements, not the view: a `BTreeMap`'s keys need `Ord`, not `Hash`.
            Ty::Coll(_, view) => view.inner_types(),
            _ => vec![],
        }
    }

    /// Visit this type and every type inside it.
    pub fn walk(&self, f: &mut impl FnMut(&Ty)) {
        f(self);
        for t in self.inner_types() {
            t.walk(f);
        }
    }

    pub fn contains_float(&self) -> bool {
        match self {
            Ty::Float { .. } => true,
            Ty::Vec(e) | Ty::Option(e) | Ty::Set(e) | Ty::Future(e) | Ty::Coll(_, e) => {
                e.contains_float()
            }
            Ty::Tuple(es) => es.iter().any(Ty::contains_float),
            Ty::Map(k, v) | Ty::Result(k, v) => k.contains_float() || v.contains_float(),
            _ => false,
        }
    }

    /// Types with a `Default` (for `unwrap_or_default` and `entry(..).or_default()`).
    pub fn has_default(&self) -> bool {
        match self {
            Ty::Int(_)
            | Ty::IntLit
            | Ty::Float { .. }
            | Ty::Bool
            | Ty::Char
            | Ty::Str
            | Ty::Unit => true,
            Ty::Vec(_) | Ty::Option(_) | Ty::Set(_) | Ty::Map(..) | Ty::Coll(..) | Ty::Unknown => {
                true
            }
            Ty::Tuple(es) => es.iter().all(Ty::has_default),
            _ => false,
        }
    }

    pub fn int_kind(&self) -> Option<IntKind> {
        match self {
            Ty::Int(k) => Some(*k),
            Ty::IntLit => Some(IntKind::I32),
            _ => None,
        }
    }

    /// Combine the operand types of a binary arithmetic expression.
    pub fn unify(a: &Ty, b: &Ty) -> Ty {
        match (a, b) {
            (Ty::IntLit, other) | (other, Ty::IntLit) if other.is_numeric() => other.clone(),
            (Ty::Never, other) | (other, Ty::Never) => other.clone(),
            (Ty::Unknown, _) | (_, Ty::Unknown) => Ty::Unknown,
            (x, y) if x == y => x.clone(),
            _ => Ty::Unknown,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Ty::Int(k) => format!("{k:?}").to_lowercase(),
            Ty::IntLit => "{integer}".into(),
            Ty::Float { f32: true } => "f32".into(),
            Ty::Float { f32: false } => "f64".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "char".into(),
            Ty::Str => "String".into(),
            Ty::Unit => "()".into(),
            Ty::Never => "!".into(),
            Ty::Extern(name) => name.clone(),
            Ty::Fn => "fn".into(),
            Ty::Future(t) => format!("impl Future<Output = {}>", t.describe()),
            Ty::Vec(t) => format!("Vec<{}>", t.describe()),
            Ty::Option(t) => format!("Option<{}>", t.describe()),
            Ty::Tuple(es) => describe_tuple(es.iter().map(Ty::describe).collect()),
            Ty::Set(t) => format!("HashSet<{}>", t.describe()),
            Ty::Coll(k, view) => k.describe(&view.describe()),
            Ty::Map(k, v) => format!("HashMap<{}, {}>", k.describe(), v.describe()),
            Ty::Entry(k, v) => format!("Entry<'_, {}, {}>", k.describe(), v.describe()),
            Ty::Closure { params, ret } => {
                describe_closure(params.iter().map(Ty::describe).collect(), ret.describe())
            }
            Ty::Ordering => "Ordering".into(),
            Ty::Adt(n, args) if args.is_empty() => n.clone(),
            Ty::Adt(n, args) => format!(
                "{n}<{}>",
                args.iter().map(Ty::describe).collect::<Vec<_>>().join(", ")
            ),
            Ty::Result(t, e) => format!("Result<{}, {}>", t.describe(), e.describe()),
            Ty::Formatter => "Formatter<'_>".into(),
            Ty::Param(n) => n.clone(),
            Ty::Dyn(n) => format!("dyn {n}"),
            Ty::RefCell(t) => format!("RefCell<{}>", t.describe()),
            Ty::Iter {
                elem,
                mutable: false,
            } => format!("std::slice::Iter<'_, {}>", elem.describe()),
            Ty::Iter {
                elem,
                mutable: true,
            } => format!("std::slice::IterMut<'_, {}>", elem.describe()),
            Ty::Var(_) => "_".into(),
            Ty::Unknown => "{unknown}".into(),
        }
    }
}

/// `impl Fn(A, B) -> R` (the `-> R` is left out for `()`).
pub fn describe_closure(params: Vec<String>, ret: String) -> String {
    let ret = if ret == "()" {
        String::new()
    } else {
        format!(" -> {ret}")
    };
    format!("impl Fn({}){ret}", params.join(", "))
}

/// `(A, B)`, or `(A,)` for a one-element tuple.
pub fn describe_tuple(parts: Vec<String>) -> String {
    if parts.len() == 1 {
        format!("({},)", parts[0])
    } else {
        format!("({})", parts.join(", "))
    }
}

/// Path segments with a leading `std::` / `core::` removed (`std::f64::consts::PI` → `f64::consts::PI`).
pub fn std_tail<'a>(segs: &'a [&'a str]) -> &'a [&'a str] {
    match segs {
        ["std" | "core", rest @ ..] => rest,
        all => all,
    }
}

/// A std constant such as `i32::MAX` or `f64::consts::PI`, as a JS expression and its type.
pub fn std_const(tail: &[&str]) -> Option<(&'static str, Ty)> {
    if let [ty_name, konst] = tail {
        if let Ty::Int(k) = Ty::from_name(ty_name) {
            let (min, max) = k.min_max();
            match *konst {
                "MAX" => return Some((max, Ty::Int(k))),
                "MIN" => return Some((min, Ty::Int(k))),
                _ => {}
            }
        }
        if let Ty::Float { f32 } = Ty::from_name(ty_name) {
            let v = match *konst {
                "INFINITY" => "Infinity",
                "NEG_INFINITY" => "-Infinity",
                "NAN" => "NaN",
                "MAX" if !f32 => "Number.MAX_VALUE",
                "MIN" if !f32 => "-Number.MAX_VALUE",
                "EPSILON" if !f32 => "Number.EPSILON",
                _ => return None,
            };
            return Some((v, Ty::Float { f32 }));
        }
    }
    if let [ty @ ("f64" | "f32"), "consts", c] = tail {
        let v = match *c {
            "PI" => "Math.PI",
            "E" => "Math.E",
            "TAU" => "(2 * Math.PI)",
            "SQRT_2" => "Math.SQRT2",
            "LN_2" => "Math.LN2",
            "LN_10" => "Math.LN10",
            _ => return None,
        };
        return Some((v, Ty::Float { f32: *ty == "f32" }));
    }
    None
}

/// Float method names CarciniScript lowers. Values of unknown type (NS results until typed
/// bindings land) are assumed to be `f64` when one of these is called on them.
pub fn is_float_method(m: &str) -> bool {
    matches!(
        m,
        "sqrt"
            | "abs"
            | "floor"
            | "ceil"
            | "trunc"
            | "round"
            | "exp"
            | "ln"
            | "log10"
            | "log2"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "cbrt"
            | "powi"
            | "powf"
            | "min"
            | "max"
            | "clamp"
            | "is_nan"
            | "is_finite"
            | "to_string"
    )
}

/// Slice methods that change the slice in place, which work on a range of a `Vec`
/// (`v[1..4].sort()`): the range is copied out, changed and written back.
pub fn slice_mut_method(m: &str) -> bool {
    matches!(
        m,
        "sort"
            | "sort_unstable"
            | "sort_by"
            | "sort_by_key"
            | "sort_unstable_by"
            | "sort_unstable_by_key"
            | "reverse"
            | "fill"
            | "swap"
            | "rotate_left"
            | "rotate_right"
            | "copy_from_slice"
            | "clone_from_slice"
    )
}
