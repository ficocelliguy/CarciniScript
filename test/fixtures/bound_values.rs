// `std::ops::Bound` as a value: matched, imported by variant, compared, mapped.
use std::ops::Bound::{self, *};

fn lo(b: Bound<&u32>) -> u32 {
    match b {
        Bound::Included(&i) => i,
        Bound::Excluded(&i) => i + 1,
        Bound::Unbounded => 0,
    }
}

fn show(b: Bound<u32>) -> String {
    match b {
        Included(x) => format!("[{x}"),
        Excluded(x) => format!("({x}"),
        Unbounded => "*".into(),
    }
}

fn main() {
    println!("{} {} {}", lo(Bound::Included(&3)), lo(Excluded(&3)), lo(Unbounded));
    let v = vec![Included(1), Excluded(2), Unbounded, std::ops::Bound::Included(9)];
    for b in &v {
        print!("{} ", show(*b));
    }
    println!();
    println!("{:?} {:?}", v, Included(&4).cloned());
    println!("{}", Bound::Excluded(3).map(|x| x * 2) == Excluded(6));
    println!("{:?}", Included(5u32).as_ref().map(|x| x + 1));
}
