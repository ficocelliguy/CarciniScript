// `extern crate core as c;` (and `std as s`): paths through the alias are std's, also inside
// macro arguments, where `::` is two `:` tokens.
extern crate alloc;
extern crate core as c;
extern crate std as s;

use c::fmt::Write;

fn largest(v: &[i32]) -> i32 {
    v.iter().fold(i32::MIN, |m, &x| c::cmp::max(m, x))
}

fn main() {
    println!("{}", c::cmp::max(1, 2));
    println!("{}", s::cmp::min(4, 3));
    let v: alloc::vec::Vec<i32> = alloc::vec![1, 2];
    println!("{:?} {}", v, largest(&v));
    let mut out = String::new();
    write!(out, "{} {}", c::u8::MAX, c::cmp::min(7, 9)).unwrap();
    println!("{}", out);
    let o: c::option::Option<i32> = Some(3);
    println!("{:?}", o);
    assert_eq!(c::cmp::max(2, 5), 5, "max of {}", c::cmp::min(2, 5));
    let words: s::collections::HashMap<&str, i32> = s::collections::HashMap::new();
    println!("{}", words.len());
}
