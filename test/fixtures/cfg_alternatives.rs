// Alternative definitions of one name under `#[cfg]`s that combine features beyond `x` /
// `not(x)`. A script has no features (as `rustc` here has none): of the alternatives, the one
// that's on with every feature off is kept, so the two compilers agree.

// Complementary predicates that aren't each other's `not(..)` syntactically.
#[cfg(all(feature = "std", not(feature = "pass-through-ffi-panics")))]
fn catch_panic() -> &'static str {
    "unwind"
}
#[cfg(any(not(feature = "std"), feature = "pass-through-ffi-panics"))]
fn catch_panic() -> &'static str {
    "pass through"
}

// Three alternatives.
#[cfg(all(feature = "js", feature = "std"))]
fn random_seed() -> Option<u64> {
    Some(1)
}
#[cfg(all(not(feature = "js"), feature = "std"))]
fn random_seed() -> Option<u64> {
    Some(2)
}
#[cfg(not(feature = "std"))]
fn random_seed() -> Option<u64> {
    None
}

// A `cfg_attr`'d definition isn't conditional; the feature-gated one gives way.
#[cfg_attr(feature = "read_buf", allow(dead_code))]
fn reader() -> u8 {
    0
}
#[cfg(feature = "read_buf")]
fn reader() -> u8 {
    1
}

// Fields, variants and `use`s with complementary predicates.
#[cfg(all(feature = "a", not(feature = "b")))]
use std::collections::BTreeMap as Map;
#[cfg(any(not(feature = "a"), feature = "b"))]
use std::collections::HashMap as Map;

#[derive(Debug)]
struct Config {
    #[cfg(all(feature = "a", feature = "b"))]
    level: u64,
    #[cfg(any(not(feature = "a"), not(feature = "b")))]
    level: u32,
}

#[derive(Debug)]
enum Mode {
    #[cfg(all(feature = "a", feature = "b"))]
    Fast(u64),
    #[cfg(any(not(feature = "a"), not(feature = "b")))]
    Fast(u32),
    Slow,
}

// Three inline modules of one name.
#[cfg(all(feature = "unicode-word-boundary", feature = "syntax"))]
mod is_word_char {
    pub fn check() -> &'static str {
        "full"
    }
}
#[cfg(all(feature = "unicode-word-boundary", not(feature = "syntax")))]
mod is_word_char {
    pub fn check() -> &'static str {
        "partial"
    }
}
#[cfg(not(feature = "unicode-word-boundary"))]
mod is_word_char {
    pub fn check() -> &'static str {
        "none"
    }
}

// Three structs with the impls that go with them.
#[cfg(feature = "rng-rand")]
struct RngImp;
#[cfg(feature = "rng-rand")]
impl RngImp {
    fn name() -> &'static str {
        "rand"
    }
}
#[cfg(all(feature = "rng-getrandom", not(feature = "rng-rand")))]
struct RngImp;
#[cfg(all(feature = "rng-getrandom", not(feature = "rng-rand")))]
impl RngImp {
    fn name() -> &'static str {
        "getrandom"
    }
}
#[cfg(all(not(feature = "rng-getrandom"), not(feature = "rng-rand")))]
struct RngImp;
#[cfg(all(not(feature = "rng-getrandom"), not(feature = "rng-rand")))]
impl RngImp {
    fn name() -> &'static str {
        "fallback"
    }
}

// Methods of one impl.
struct Styled;
impl Styled {
    #[cfg(all(not(feature = "wrap_help"), feature = "help"))]
    fn wrap(&self) -> &'static str {
        "no wrap"
    }
    #[cfg(feature = "wrap_help")]
    fn wrap(&self) -> &'static str {
        "wrap"
    }
    #[cfg(not(feature = "help"))]
    fn wrap(&self) -> &'static str {
        "no help"
    }
}

fn main() {
    println!("{} {:?} {}", catch_panic(), random_seed(), reader());
    let mut m: Map<&str, u32> = Map::new();
    m.insert("k", 1);
    println!("{}", m["k"]);
    let c = Config { level: 3 };
    println!("{:?} {:?} {:?}", c, Mode::Fast(4), Mode::Slow);
    println!("{} {} {}", is_word_char::check(), RngImp::name(), Styled.wrap());
}
