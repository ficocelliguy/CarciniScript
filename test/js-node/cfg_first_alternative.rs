// `#[cfg]` alternatives of one name that each need some feature: none is on for a script (and
// `rustc` with no features would define none, so this isn't a differential fixture). The first
// written is kept, so the file compiles as it would with the first feature set that builds it.
#[cfg(not(any(feature = "unsafe", not(feature = "std"))))]
fn decompress() -> &'static str {
    "safe"
}
#[cfg(all(feature = "unsafe", feature = "std"))]
fn decompress() -> &'static str {
    "unsafe"
}

#[cfg(all(feature = "phf", feature = "rev-mappings"))]
fn build_rev_map() -> &'static str {
    "phf"
}
#[cfg(all(not(feature = "phf"), feature = "rev-mappings"))]
fn build_rev_map() -> &'static str {
    "plain"
}

// Two impls of one header defining one method.
struct W;
#[cfg(all(feature = "union", feature = "const_new"))]
impl W {
    fn from_const() -> &'static str {
        "union"
    }
}
#[cfg(all(not(feature = "union"), feature = "const_new"))]
impl W {
    fn from_const() -> &'static str {
        "enum"
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
}

// Two impls of one header that don't collide both stay.
struct Both;
#[cfg(feature = "a")]
impl Both {
    fn a(&self) -> u8 {
        1
    }
}
#[cfg(feature = "b")]
impl Both {
    fn b(&self) -> u8 {
        2
    }
}

fn main() {
    println!("{} {} {} {}", decompress(), build_rev_map(), W::from_const(), Styled.wrap());
    println!("{}", Both.a() + Both.b());
}
