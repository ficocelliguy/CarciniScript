// A range literal converted through your `From<Range<usize>>` / `From<RangeInclusive<usize>>`
// impls (`(1..=3).into()`, `X::from(0..2)`): a range value, picked by its kind.
use std::ops::{Range, RangeInclusive};
#[derive(Debug)]
struct ValueRange { lo: usize, hi: usize }
impl From<Range<usize>> for ValueRange {
    fn from(r: Range<usize>) -> Self { ValueRange { lo: r.start, hi: r.end } }
}
impl From<RangeInclusive<usize>> for ValueRange {
    fn from(r: RangeInclusive<usize>) -> Self { ValueRange { lo: *r.start(), hi: *r.end() + 1 } }
}
fn takes(r: impl Into<ValueRange>) -> usize { let r = r.into(); r.hi - r.lo }
fn main() {
    let a: ValueRange = (1..3).into();
    let b: ValueRange = (2..=4).into();
    println!("{:?} {:?} {:?}", a, b, ValueRange::from(0..1));
    println!("{}", takes(ValueRange::from(5..=9)));
}
