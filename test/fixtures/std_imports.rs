// std functions imported by name (`use std::cmp::{max, min}`, `use std::mem::{swap, take as
// grab}`), and test-only items (`#[cfg(test)] mod tests`, `#[test] fn`), which are left out like
// rustc does without `--test`.
use std::cmp::{max, min};
use std::mem::{replace, swap, take as grab};

fn clamp(x: i32, lo: i32, hi: i32) -> i32 {
    max(lo, min(x, hi))
}

fn main() {
    println!("{} {} {}", clamp(15, 0, 10), clamp(-3, 0, 10), clamp(4, 0, 10));
    println!("{}", max("pear", "apple"));
    let (mut a, mut b) = (vec![1], vec![2, 3]);
    swap(&mut a, &mut b);
    let old = replace(&mut a, vec![9]);
    let taken = grab(&mut b);
    println!("{:?} {:?} {:?} {:?}", a, b, old, taken);
}

#[test]
fn not_compiled_outside_tests() {
    assert_eq!(clamp(1, 2, 3), 2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps() {
        assert_eq!(clamp(99, 0, 10), 10);
    }
}
