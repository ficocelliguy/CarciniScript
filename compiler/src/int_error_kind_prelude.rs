// (int error kind prelude)
// `std::num::IntErrorKind`, compiled in when a program names it: what `ParseIntError::kind()`
// returns (the runtime's parse error keeps the variant's name, so `kind()` is a lookup).

/// Why parsing an integer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IntErrorKind {
    Empty,
    InvalidDigit,
    PosOverflow,
    NegOverflow,
    Zero,
}
