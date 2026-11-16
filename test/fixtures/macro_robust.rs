// `macro_rules!` corner cases from real crates: doc comments as `#[doc]` attributes, one `ty` per
// repetition, `::` separators, `#[cfg]`-gated alternative definitions, `pub(crate) use` of a
// macro, `item` fragments with generics and closures with commas as `expr` arguments.

// Doc comments in the input are `#[doc = ".."]` attributes, matched by `$(#[$m:meta])*`.
macro_rules! consts {
    ($(#[$meta:meta])* $name:ident($ty:ty) { $($c:ident = $v:expr),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct $name(pub $ty);
        $(pub const $c: $name = $name($v);)+
        impl $name {
            pub fn name(&self) -> Option<&'static str> {
                if let Some(s) = self.lookup() {
                    Some(s)
                } else {
                    None
                }
            }
            fn lookup(&self) -> Option<&'static str> {
                $(if *self == $c { return Some(stringify!($c)); })+
                None
            }
        }
    };
}

consts!(
    /// A section kind.
    ///
    /// With "quotes" and \ backslashes.
    Sect(u32) {
        SECT_INFO = 1,
        SECT_ABBREV = 3,
    }
);

// Doc comments on each repetition, written out again by the transcriber.
macro_rules! codes {
    ($( $(#[$docs:meta])* ($num:expr, $konst:ident, $phrase:expr); )+) => {
        $( $(#[$docs])* const $konst: (u16, &str) = ($num, $phrase); )+
        fn all_codes() -> Vec<(u16, &'static str)> {
            vec![$($konst),+]
        }
    };
}

codes! {
    /// 100 Continue
    (100, CONTINUE, "Continue");
    /// 404 Not Found
    /// (two lines)
    (404, NOT_FOUND, "Not Found");
    (418, TEAPOT, "I'm a teapot");
}

// A doc comment in the transcriber is an attribute too (it used to comment out the next line).
macro_rules! documented {
    ($name:ident) => {
        /// Doubles its argument.
        fn $name(x: i32) -> i32 { x * 2 }
    };
}
documented!(double);

// `ty` repeated without a separator: one type per repetition.
fn zero<T: Default + std::fmt::Debug>() -> String {
    format!("{:?}", T::default())
}

macro_rules! zeros {
    ($($t:ty)*) => {
        vec![$(zero::<$t>()),*]
    };
}

macro_rules! count_types {
    ($($t:ty)*) => {
        0 $(+ { let _: Option<$t> = None; 1 })*
    };
}

// `ty` followed by a `:` (not half of `core::num::X`'s `::`).
macro_rules! renamed {
    ($($ty:ty : $name:ident => $size:expr);+) => {
        fn renamed() -> Vec<(&'static str, usize)> {
            vec![$((stringify!($name), $size + zero::<$ty>().len())),+]
        }
    };
}
renamed!(std::vec::Vec<Vec<u8>>: Nested => 1; Option<Box<u16>>: OptBox => 2);

// A `::` separator.
macro_rules! join_path {
    ($($seg:ident)::+) => {
        vec![$(stringify!($seg)),+].join("/")
    };
}

// Repetitions without a separator stay separate tokens in the expansion.
macro_rules! pairs {
    ($($k:ident : $v:tt)*) => {
        vec![$((stringify!($k), $v)),*]
    };
}
macro_rules! forward_pairs {
    ($($k:ident : $v:tt)*) => {
        pairs!($($k : $v)*)
    };
}

// `#[cfg]`-gated alternatives: only the active definition is used.
#[cfg(feature = "std")]
macro_rules! backtrace {
    () => {
        "with std"
    };
}

#[cfg(not(feature = "std"))]
macro_rules! backtrace {
    () => {
        "without std"
    };
}

#[cfg(test)]
macro_rules! mode {
    () => {
        "test"
    };
}

#[cfg(not(test))]
macro_rules! mode {
    () => {
        "normal"
    };
}

// The macro imported by path, 2018-style.
macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}
#[allow(unused_imports)]
pub(crate) use square;

mod shapes {
    macro_rules! area {
        ($w:expr, $h:expr) => {
            $w * $h
        };
    }
    #[allow(unused_imports)]
    pub(crate) use area;

    pub fn rect(w: u32, h: u32) -> u32 {
        area!(w, h)
    }
}

// An `item` fragment with commas in its generics.
macro_rules! with_doc {
    ($a:ident @ #[$meta:meta] $item:item) => {
        #[$meta]
        $item
    };
}
with_doc! {
    x @ #[derive(Debug)]
    struct Pair<A, B>(A, B);
}
macro_rules! items {
    ($($i:item)*) => { $($i)* };
}
items! {
    fn first<A: Clone, B>(p: &Pair<A, B>) -> A { p.0.clone() }
    const LIMIT: u32 = { 3 + 4 };
    struct Unit;
}

// A closure with commas as an `expr` argument, and commas inside a turbofish.
macro_rules! apply {
    ($t:ty, $x:expr, $f:expr) => {
        ($f)($x as $t, 10)
    };
}
macro_rules! size_eq {
    (@usize_eq: $lhs:expr, $rhs:expr) => {
        $lhs == $rhs
    };
}

// The same name defined again: later invocations use the new definition.
macro_rules! version {
    () => {
        1
    };
}
fn first_version() -> i32 {
    version!()
}
macro_rules! version {
    () => {
        2
    };
}

fn main() {
    println!("{:?} {:?}", SECT_INFO, SECT_ABBREV.name());
    println!("{:?}", Sect(7).name());
    println!("{:?}", all_codes());
    println!("{} {} {}", CONTINUE.0, NOT_FOUND.1, TEAPOT.1);
    println!("{}", double(21));
    println!("{:?}", zeros!(u8 i64 bool String Vec<u8> Option<Box<u8>>));
    println!("{}", count_types!(u8 u16 &str Vec<u8> (u8, u32) [u16; 3] Option<Box<u8>> fn(u8) -> u8 &'static mut [u8]));
    println!("{:?}", renamed());
    println!("{}", join_path!(a::b::c));
    println!("{:?}", forward_pairs!(a: 1 b: (2 + 3) c: 4));
    println!("{} {}", backtrace!(), mode!());
    println!("{} {}", square!(7), shapes::rect(3, 4));
    let p = Pair("left", 2);
    println!("{:?} {} {}", p, first(&p), LIMIT);
    let _ = Unit;
    println!("{}", apply!(i64, 5, |a: i64, b: i64| a * b));
    println!("{}", size_eq!(@usize_eq: zero::<Option<Result<(), u8>>>().len(), 4));
    println!("{} {}", first_version(), version!());
}
