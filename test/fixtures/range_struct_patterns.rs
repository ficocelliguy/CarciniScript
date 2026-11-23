// Struct patterns of std's ranges: `let Range { start, end } = r;`, in parameters and `match`.
use std::ops::{Range, RangeFrom, RangeTo};
fn width(Range { start, end }: Range<usize>) -> usize { end - start }
fn lo(r: &RangeFrom<i32>) -> i32 { let RangeFrom { start } = r.clone(); start }
fn main() {
    let r: Range<usize> = 2..5;
    let Range { start, end } = r.clone();
    println!("{} {}", start, end);
    let Range { start: s, .. } = r.clone();
    println!("{} {}", s, width(r));
    let t: RangeTo<u8> = ..9;
    let RangeTo { end } = t;
    println!("{} {}", end, lo(&(4..)));
    match 1..3 {
        std::ops::Range { start: 0, .. } => println!("zero"),
        Range { start, end } => println!("{}..{}", start, end),
    }
}
