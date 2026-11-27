// (fp category prelude)
// `std::num::FpCategory`, compiled in when a program names it or calls `classify`: what
// `f64::classify` / `f32::classify` return (the runtime names the variant, `κσ.$fpClass`).

/// A classification of floating point numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FpCategory {
    Nan,
    Infinite,
    Zero,
    Subnormal,
    Normal,
}
