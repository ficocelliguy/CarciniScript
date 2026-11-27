//! The type lattice the checker infers over and codegen lowers from: enough to lower
//! arithmetic, casts and formatting correctly.

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

    /// The kinds that are JS BigInts: `i128` / `u128` only. The 64-bit kinds (`i64` / `u64`, like
    /// `isize` / `usize`) are JS numbers, exact to 2^53, so their arithmetic runs at number speed.
    pub fn is_big(self) -> bool {
        matches!(self, IntKind::I128 | IntKind::U128)
    }

    /// The 64-bit kinds, JS numbers that may go past 2^53 (and print as `κσ.$fmtI64` does).
    pub fn is_64(self) -> bool {
        matches!(self, IntKind::I64 | IntKind::U64 | IntKind::Isize | IntKind::Usize)
    }

    /// Prefix/suffix that wrap a JS integer-valued number into this kind, or None for wide kinds,
    /// which are plain JS numbers. The inner expression is always parenthesized: JS's `>>>` and
    /// `&` bind tighter than `^`/`|`, so `x ^ y >>> 0` would wrap only `y`.
    /// The width in bits.
    pub fn bits(self) -> u32 {
        match self {
            IntKind::I8 | IntKind::U8 => 8,
            IntKind::I16 | IntKind::U16 => 16,
            IntKind::I32 | IntKind::U32 => 32,
            IntKind::I64 | IntKind::U64 | IntKind::Isize | IntKind::Usize => 64,
            IntKind::I128 | IntKind::U128 => 128,
        }
    }

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
    /// The bundled `indexmap`'s `κσ.IndexMap` (a `HashMap`, which keeps insertion order, with
    /// positions); the view is `HashMap<K, V>`.
    IndexMap,
    /// `κσ.IndexSet`; the view is `HashSet<T>`.
    IndexSet,
    /// `[T; N]` with its length known, as the exhaustiveness check sees it (only there: arrays
    /// are `Vec<T>` everywhere else); the view is `Vec<T>`.
    FixedArray(usize),
}

/// Which of std's range types a `Ty::Range` is. All are the runtime's range iterator; a range
/// without a start (`..7`, `..`) is marked so, and starts at 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeKind {
    /// `a..b`
    Range,
    /// `a..=b`
    Inclusive,
    /// `a..`
    From,
    /// `..b`
    To,
    /// `..=b`
    ToInclusive,
    /// `..`
    Full,
}

impl RangeKind {
    pub fn from_name(name: &str) -> Option<RangeKind> {
        Some(match name {
            "Range" => RangeKind::Range,
            "RangeInclusive" => RangeKind::Inclusive,
            "RangeFrom" => RangeKind::From,
            "RangeTo" => RangeKind::To,
            "RangeToInclusive" => RangeKind::ToInclusive,
            "RangeFull" => RangeKind::Full,
            _ => return None,
        })
    }

    /// The kind a range expression makes, by which bounds it has.
    pub fn of(start: bool, end: bool, inclusive: bool) -> RangeKind {
        match (start, end, inclusive) {
            (true, true, false) => RangeKind::Range,
            (true, true, true) => RangeKind::Inclusive,
            (true, false, _) => RangeKind::From,
            (false, true, false) => RangeKind::To,
            (false, true, true) => RangeKind::ToInclusive,
            (false, false, _) => RangeKind::Full,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RangeKind::Range => "Range",
            RangeKind::Inclusive => "RangeInclusive",
            RangeKind::From => "RangeFrom",
            RangeKind::To => "RangeTo",
            RangeKind::ToInclusive => "RangeToInclusive",
            RangeKind::Full => "RangeFull",
        }
    }

    pub fn inclusive(self) -> bool {
        matches!(self, RangeKind::Inclusive | RangeKind::ToInclusive)
    }

    pub fn has_start(self) -> bool {
        matches!(self, RangeKind::Range | RangeKind::Inclusive | RangeKind::From)
    }

    pub fn has_end(self) -> bool {
        !matches!(self, RangeKind::From | RangeKind::Full)
    }

    /// The `kind` its impls are registered under at run time (`κσ.$extReg`).
    pub fn rt_kind(self) -> &'static str {
        match self {
            RangeKind::Range => "range",
            RangeKind::Inclusive => "rangeincl",
            RangeKind::From => "rangefrom",
            RangeKind::To => "rangeto",
            RangeKind::ToInclusive => "rangetoincl",
            RangeKind::Full => "rangefull",
        }
    }
}

impl Coll {
    pub fn from_name(name: &str) -> Option<Coll> {
        Some(match name {
            "VecDeque" => Coll::VecDeque,
            "BinaryHeap" => Coll::BinaryHeap,
            "BTreeMap" => Coll::BTreeMap,
            "BTreeSet" => Coll::BTreeSet,
            "IndexMap" => Coll::IndexMap,
            "IndexSet" => Coll::IndexSet,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Coll::VecDeque => "VecDeque",
            Coll::BinaryHeap => "BinaryHeap",
            Coll::BTreeMap => "BTreeMap",
            Coll::BTreeSet => "BTreeSet",
            Coll::IndexMap => "IndexMap",
            Coll::IndexSet => "IndexSet",
            Coll::FixedArray(_) => "array",
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
    /// `&mut T` for a number, string, bool or `Option` `T`, as a closure's parameter only
    /// (`F: FnMut(&mut String)`, `|n: &mut i32|`): the closure takes a cell, and a call passes
    /// one (`f(&mut n)`). Anywhere else it's `T`.
    MutRef(Box<Ty>),
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
    /// What `f.debug_struct(..)` and friends return: `DebugStruct`, `DebugTuple`, `DebugList`,
    /// `DebugSet` or `DebugMap`. The runtime's `κσ.DbgBuilder`.
    FmtBuilder(String),
    /// A `std::hash::Hasher` (`H: Hasher` in an `impl Hash`, `DefaultHasher`): the runtime's
    /// `κσ.Hasher`, collecting what's written to it.
    Hasher,
    /// A random number generator of the bundled `rand` crate (`ThreadRng`, `StdRng`, `R: Rng`):
    /// the runtime's `RandRng`.
    Rng,
    /// A generic function's type parameter, inside its body. Erased in JS.
    Param(String),
    /// A const generic argument (`3` in `Buf<3>`, or a `const` item's name), as written in Rust.
    ConstVal(String),
    /// `dyn Trait` / `impl Trait` / `Box<dyn Trait>`: any value whose type implements the trait.
    Dyn(String, Vec<Ty>),
    /// `RefCell<T>` / `Cell<T>`: a `κσ.RefCell` holding the value at `.v`. (`Rc<T>` is erased.)
    RefCell(Box<Ty>),
    /// An `I: IntoIterator<Item = T>` parameter (or `impl IntoIterator<Item = T>`): any iterable
    /// of `T` (a Vec, set, map or iterator) is passed; the function turns it into the runtime's
    /// iterator on entry (`κσ.$intoIter`).
    IntoIter(Box<Ty>),
    /// What `v.iter()` / `v.iter_mut()` / `v.into_iter()` return. Only usable as a `for` loop
    /// source until iterator adapters land.
    Iter {
        elem: Box<Ty>,
        mutable: bool,
    },
    /// A range as a value (`Range<T>`, `RangeInclusive<T>`, `RangeFrom<T>`, .. by `.1`): a
    /// `let r = a..b`, a field or a return type. At run time the same iterator a range
    /// expression makes, so codegen sees it as an iterator (`view`); `start` / `end`,
    /// `contains`, `{:?}` and slicing are its own.
    Range(Box<Ty>, RangeKind),
    /// `VecDeque<T>`, `BinaryHeap<T>`, `BTreeMap<K, V>`, `BTreeSet<T>`: the collection over the
    /// type it behaves like at run time (`Vec<T>`, `HashMap<K, V>`, `HashSet<T>`).
    Coll(Coll, Box<Ty>),
    /// A type-inference variable. Only exists inside the type checker; never reaches codegen.
    Var(u32),
    /// No type information (e.g. an NS value without a binding, `JsValue`). Lowered with plain JS
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
            Ty::IntoIter(e) => Ty::IntoIter(b(e)),
            Ty::Range(e, _) => Ty::Iter {
                elem: b(e),
                mutable: false,
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
            "Formatter" => Ty::Formatter,
            // (A tuple's extension key, `Tuple2`.)
            k => match crate::adt::tuple_arity(k) {
                Some(n) => Ty::Tuple(vec![Ty::Unknown; n]),
                None => Ty::Unknown,
            },
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
            Ty::Int(k) if k.is_big() => "0n",
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
    /// The type parameters it names (`T` in `Vec<T>`).
    pub fn collect_params(&self, out: &mut Vec<String>) {
        match self {
            Ty::Param(p) => {
                if !out.contains(p) {
                    out.push(p.clone());
                }
            }
            t => {
                for i in t.inner_types() {
                    i.collect_params(out);
                }
            }
        }
    }

    pub fn has_ambiguous_option(&self) -> bool {
        match self {
            Ty::Option(inner) => matches!(**inner, Ty::Option(_)) || inner.has_ambiguous_option(),
            Ty::Vec(e) | Ty::Future(e) | Ty::Set(e) | Ty::Coll(_, e) => e.has_ambiguous_option(),
            Ty::Tuple(es) => es.iter().any(Ty::has_ambiguous_option),
            Ty::Map(k, v) => k.has_ambiguous_option() || v.has_ambiguous_option(),
            Ty::Iter { elem, .. } => elem.has_ambiguous_option(),
            _ => false,
        }
    }

    /// Whether a float appears anywhere in the type: such types aren't `Ord` or `Hash`.
    /// The types directly inside this one (`Vec<T>` → `[T]`).
    /// Whether the type parameter `name` appears in this type.
    pub fn mentions_param(&self, name: &str) -> bool {
        match self {
            Ty::Param(p) => p == name,
            Ty::Closure { params, ret } => {
                params.iter().any(|p| p.mentions_param(name)) || ret.mentions_param(name)
            }
            t => t.inner_types().iter().any(|i| i.mentions_param(name)),
        }
    }

    pub fn inner_types(&self) -> Vec<Ty> {
        match self {
            Ty::Vec(e)
            | Ty::Option(e)
            | Ty::Set(e)
            | Ty::Future(e)
            | Ty::RefCell(e)
            | Ty::MutRef(e)
            | Ty::IntoIter(e) => {
                vec![(**e).clone()]
            }
            Ty::Iter { elem, .. } | Ty::Range(elem, _) => vec![(**elem).clone()],
            Ty::Tuple(es) => es.clone(),
            Ty::Map(k, v) | Ty::Entry(k, v) | Ty::Result(k, v) => {
                vec![(**k).clone(), (**v).clone()]
            }
            Ty::Closure { params, ret } => [params.clone(), vec![(**ret).clone()]].concat(),
            Ty::Adt(_, args) | Ty::Dyn(_, args) => args.clone(),
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
            // (An empty one: `m.get_many::<String>("x").unwrap_or_default()`.)
            Ty::Iter { .. } => true,
            Ty::RefCell(inner) => inner.has_default(),
            Ty::Extern(n) => n == "JsValue",
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
            Ty::FmtBuilder(k) => format!("{k}<'_, '_>"),
            Ty::Hasher => "impl Hasher".into(),
            Ty::Rng => "ThreadRng".into(),
            Ty::Param(n) => n.clone(),
            Ty::ConstVal(v) => v.clone(),
            Ty::Dyn(n, args) if args.is_empty() => format!("dyn {n}"),
            Ty::Dyn(n, args) => format!(
                "dyn {n}<{}>",
                args.iter().map(Ty::describe).collect::<Vec<_>>().join(", ")
            ),
            Ty::RefCell(t) => format!("RefCell<{}>", t.describe()),
            Ty::Iter {
                elem,
                mutable: false,
            } => format!("std::slice::Iter<'_, {}>", elem.describe()),
            Ty::Iter {
                elem,
                mutable: true,
            } => format!("std::slice::IterMut<'_, {}>", elem.describe()),
            Ty::IntoIter(e) => format!("impl IntoIterator<Item = {}>", e.describe()),
            Ty::Range(_, RangeKind::Full) => "RangeFull".into(),
            Ty::Range(e, k) => format!("{}<{}>", k.name(), e.describe()),
            Ty::MutRef(t) => format!("&mut {}", t.describe()),
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

/// Path segments with a leading `std::` / `core::` removed (`std::f64::consts::PI` → `f64::consts::PI`),
/// and a prelude type's module with it (`std::vec::Vec::new` → `Vec::new`, as `Vec::new` is read).
pub fn std_tail<'a>(segs: &'a [&'a str]) -> &'a [&'a str] {
    match segs {
        ["std" | "core" | "alloc", rest @ ..] => match rest {
            ["vec", "Vec", ..]
            | ["string", "String" | "ToString", ..]
            | ["boxed", "Box", ..]
            | ["option", "Option", ..]
            | ["result", "Result", ..] => &rest[1..],
            _ => rest,
        },
        all => all,
    }
}

/// `names` with a prelude type's std module removed (`std::vec::Vec::new` → `Vec::new`, as
/// `std_tail` reads such a path), for the handlers that match on the segments.
pub fn strip_prelude_modules(names: Vec<String>) -> Vec<String> {
    let s: Vec<&str> = names.iter().map(String::as_str).collect();
    match s.as_slice() {
        ["std" | "core" | "alloc", "vec", "Vec", ..]
        | ["std" | "core" | "alloc", "string", "String" | "ToString", ..]
        | ["std" | "core" | "alloc", "boxed", "Box", ..]
        | ["std" | "core" | "alloc", "option", "Option", ..]
        | ["std" | "core" | "alloc", "result", "Result", ..] => names[2..].to_vec(),
        _ => names,
    }
}

/// A std constant such as `i32::MAX` or `f64::consts::PI`, as a JS expression and its type.
pub fn std_const(tail: &[&str]) -> Option<(&'static str, Ty)> {
    // `Err(fmt::Error)` from your own `fmt::Write` or `Display`.
    if let ["fmt", "Error"] = tail {
        return Some(("κσ.$fmtError", Ty::Extern("fmt::Error".into())));
    }
    if let ["char", "REPLACEMENT_CHARACTER"] = tail {
        return Some(("\"\\u{fffd}\"", Ty::Char));
    }
    if let [ty_name, konst] = tail {
        if let Ty::Int(k) = Ty::from_name(ty_name) {
            // `i128` / `u128` are BigInts; the 64-bit kinds are JS numbers, so their `MIN` /
            // `MAX` are the safe-integer bounds, as `usize`'s are.
            let (min, max) = match k {
                IntKind::I128 => (
                    "-170141183460469231731687303715884105728n",
                    "170141183460469231731687303715884105727n",
                ),
                IntKind::U128 => ("0n", "340282366920938463463374607431768211455n"),
                _ => k.min_max(),
            };
            match *konst {
                "MAX" => return Some((max, Ty::Int(k))),
                "MIN" => return Some((min, Ty::Int(k))),
                _ => {}
            }
        }
        if let Ty::Float { f32 } = Ty::from_name(ty_name) {
            // The format's integer constants.
            let int = |v: &'static str, k| Some((v, Ty::Int(k)));
            match (*konst, f32) {
                ("RADIX", _) => return int("2", IntKind::U32),
                ("DIGITS", false) => return int("15", IntKind::U32),
                ("DIGITS", true) => return int("6", IntKind::U32),
                ("MANTISSA_DIGITS", false) => return int("53", IntKind::U32),
                ("MANTISSA_DIGITS", true) => return int("24", IntKind::U32),
                ("MAX_EXP", false) => return int("1024", IntKind::I32),
                ("MAX_EXP", true) => return int("128", IntKind::I32),
                ("MIN_EXP", false) => return int("-1021", IntKind::I32),
                ("MIN_EXP", true) => return int("-125", IntKind::I32),
                ("MAX_10_EXP", false) => return int("308", IntKind::I32),
                ("MAX_10_EXP", true) => return int("38", IntKind::I32),
                ("MIN_10_EXP", false) => return int("-307", IntKind::I32),
                ("MIN_10_EXP", true) => return int("-37", IntKind::I32),
                _ => {}
            }
            // (An `f32` is the double it rounds to.)
            let v = match (*konst, f32) {
                ("INFINITY", _) => "Infinity",
                ("NEG_INFINITY", _) => "-Infinity",
                ("NAN", _) => "NaN",
                ("MAX", false) => "Number.MAX_VALUE",
                ("MIN", false) => "-Number.MAX_VALUE",
                ("EPSILON", false) => "Number.EPSILON",
                ("MIN_POSITIVE", false) => "2.2250738585072014e-308",
                ("MAX", true) => "3.4028234663852886e38",
                ("MIN", true) => "-3.4028234663852886e38",
                ("EPSILON", true) => "1.1920928955078125e-7",
                ("MIN_POSITIVE", true) => "1.1754943508222875e-38",
                _ => return None,
            };
            return Some((v, Ty::Float { f32 }));
        }
    }
    if let [ty @ ("f64" | "f32"), "consts", c] = tail {
        let f32 = *ty == "f32";
        let v = match (*c, f32) {
            ("PI", false) => "Math.PI",
            ("TAU", false) => "6.283185307179586",
            ("E", false) => "Math.E",
            ("SQRT_2", false) => "Math.SQRT2",
            ("FRAC_1_SQRT_2", false) => "Math.SQRT1_2",
            ("FRAC_1_PI", false) => "0.3183098861837907",
            ("FRAC_2_PI", false) => "0.6366197723675814",
            ("FRAC_2_SQRT_PI", false) => "1.1283791670955126",
            ("FRAC_PI_2", false) => "1.5707963267948966",
            ("FRAC_PI_3", false) => "1.0471975511965979",
            ("FRAC_PI_4", false) => "0.7853981633974483",
            ("FRAC_PI_6", false) => "0.5235987755982989",
            ("FRAC_PI_8", false) => "0.39269908169872414",
            ("LN_2", false) => "Math.LN2",
            ("LN_10", false) => "Math.LN10",
            ("LOG2_E", false) => "Math.LOG2E",
            ("LOG2_10", false) => "3.321928094887362",
            ("LOG10_E", false) => "Math.LOG10E",
            ("LOG10_2", false) => "0.3010299956639812",
            ("PI", true) => "3.1415927410125732",
            ("TAU", true) => "6.2831854820251465",
            ("E", true) => "2.7182817459106445",
            ("SQRT_2", true) => "1.4142135381698608",
            ("FRAC_1_SQRT_2", true) => "0.7071067690849304",
            ("FRAC_1_PI", true) => "0.31830987334251404",
            ("FRAC_2_PI", true) => "0.6366197466850281",
            ("FRAC_2_SQRT_PI", true) => "1.128379225730896",
            ("FRAC_PI_2", true) => "1.5707963705062866",
            ("FRAC_PI_3", true) => "1.0471975803375244",
            ("FRAC_PI_4", true) => "0.7853981852531433",
            ("FRAC_PI_6", true) => "0.5235987901687622",
            ("FRAC_PI_8", true) => "0.39269909262657166",
            ("LN_2", true) => "0.6931471824645996",
            ("LN_10", true) => "2.3025851249694824",
            ("LOG2_E", true) => "1.4426950216293335",
            ("LOG2_10", true) => "3.321928024291992",
            ("LOG10_E", true) => "0.4342944920063019",
            ("LOG10_2", true) => "0.3010300099849701",
            _ => return None,
        };
        return Some((v, Ty::Float { f32 }));
    }
    None
}

/// A std constant through a `use`: `PI` after `use std::f64::consts::PI;`, `consts::PI` after
/// `use std::f64::consts;` (`std_fns` maps the imported name to its path after `std`).
pub fn std_alias_const(
    std_fns: &std::collections::HashMap<String, Vec<String>>,
    segs: &[&str],
) -> Option<(&'static str, Ty)> {
    let (first, rest) = segs.split_first()?;
    let mut tail: Vec<&str> = std_fns.get(*first)?.iter().map(String::as_str).collect();
    tail.extend(rest);
    std_const(&tail)
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
