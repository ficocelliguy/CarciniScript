// Ranges as values: `Range<T>` / `RangeInclusive<T>` in fields, returns and `let`s, with
// `start` / `end`, `contains`, `is_empty`, `{:?}`, `clone`, `==`, slicing, and iterating.
use std::ops::{Range, RangeInclusive};

#[derive(Debug, Clone)]
struct Span {
    name: String,
    r: Range<usize>,
}

fn half(n: usize) -> Range<usize> {
    0..n / 2
}

fn window(c: i32) -> RangeInclusive<i32> {
    (c - 1)..=(c + 1)
}

fn main() {
    let r = half(10);
    println!("{:?} {} {} {} {}", r, r.start, r.end, r.len(), r.is_empty());
    let s = Span {
        name: "mid".to_string(),
        r: 2..4,
    };
    let v = vec![10, 20, 30, 40, 50];
    println!("{:?} {} {}", &v[s.r.clone()], s.r.contains(&3), s.r.contains(&4));
    println!("{:?}", s);
    for i in s.r.clone() {
        print!("{} ", i);
    }
    println!();
    let ri = 1..=3;
    println!("{:?} {} {} {}", ri, ri.start(), ri.end(), ri.contains(&3));
    let w = window(5);
    println!("{:?} sum {}", w, w.clone().sum::<i32>());
    let text = "hello world";
    let word = 6..11;
    println!("{}", &text[word.clone()]);
    let rs: Vec<Range<i32>> = vec![0..2, 5..7, 3..3];
    println!("{:?}", rs);
    let empties: Vec<bool> = rs.iter().map(|r| r.is_empty()).collect();
    println!("{:?}", empties);
    let total: i32 = rs.into_iter().map(|r| r.sum::<i32>()).sum();
    println!("{}", total);
    let f = 0.5..1.5;
    println!("{} {}", f.contains(&1.0), f.contains(&1.5));
    let mut it = 0..4;
    it.next();
    println!("{:?} {}", it, it == (1..4));
    let squares: Vec<usize> = half(8).map(|x| x * x).collect();
    println!("{:?}", squares);
}
