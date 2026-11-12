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
    /// The Netscript handle or one of its namespaces. Methods get snake_case → camelCase.
    Ns,
    Fn,
    /// The result of calling an `async fn`; `.await` unwraps it.
    Future(Box<Ty>),
    /// No type information (e.g. NS return values until phase 2 bindings). Lowered with plain JS
    /// semantics.
    Unknown,
}

impl Ty {
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
            "NS" => Ty::Ns,
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
            Ty::Ns => "NS".into(),
            Ty::Fn => "fn".into(),
            Ty::Future(t) => format!("impl Future<Output = {}>", t.describe()),
            Ty::Unknown => "{unknown}".into(),
        }
    }
}
