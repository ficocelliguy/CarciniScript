// Your traits on `RangeFrom` / `RangeTo` / `RangeToInclusive` / `RangeFull`, next to `Range`'s.
use std::ops::{Range, RangeFrom, RangeFull, RangeTo, RangeToInclusive};

trait Describe {
    fn describe(&self) -> String;
    fn twice(&self) -> String {
        format!("{0}; {0}", self.describe())
    }
}

impl Describe for Range<usize> {
    fn describe(&self) -> String {
        format!("{} to {}", self.start, self.end)
    }
}

impl Describe for RangeFrom<usize> {
    fn describe(&self) -> String {
        format!("from {}", self.start)
    }
}

impl Describe for RangeTo<i32> {
    fn describe(&self) -> String {
        format!("below {}", self.end)
    }
}

impl Describe for RangeToInclusive<i32> {
    fn describe(&self) -> String {
        format!("up to {}", self.end)
    }
}

impl Describe for RangeFull {
    fn describe(&self) -> String {
        "everything".to_string()
    }
}

fn show<T: Describe>(t: &T) -> String {
    t.describe()
}

fn dynamic(items: &[&dyn Describe]) {
    for d in items {
        println!("dyn: {}", d.describe());
    }
}

struct Window {
    tail: RangeFrom<usize>,
    head: RangeTo<usize>,
}

fn head_len(r: RangeTo<usize>) -> usize {
    r.end
}

fn main() {
    let r = 3usize..;
    println!("{}", r.describe());
    println!("{}", (..7).describe());
    println!("{}", (..=7).describe());
    println!("{}", (..).describe());
    println!("{}", (2usize..5).describe());
    println!("{}", (..7).twice());

    println!("{}", show(&(5usize..)));
    println!("{}", show(&(..2)));
    println!("{}", show(&(..=2)));
    println!("{}", show(&(..)));
    println!("{}", show(&(1usize..4)));
    dynamic(&[&(9usize..), &(..1), &(..), &(0usize..1)]);

    // Still iterators where one is wanted.
    let v: Vec<i32> = (10..).take(3).collect();
    println!("{:?}", v);
    let mut n = 0;
    for i in 4usize.. {
        if i > 6 {
            break;
        }
        n += i;
    }
    println!("{}", n);

    // As values: `{:?}`, fields, slicing.
    let to: RangeTo<usize> = ..2;
    let from: RangeFrom<usize> = 1..;
    let full: RangeFull = ..;
    let incl: RangeToInclusive<usize> = ..=3;
    println!("{:?} {:?} {:?} {:?}", to, from, full, incl);
    println!("{} {} {}", to.end, from.start, incl.end);
    let xs = [10, 20, 30, 40, 50];
    println!("{:?} {:?} {:?} {:?}", &xs[to], &xs[from], &xs[full], &xs[incl]);
    println!("{} {}", to.contains(&1), incl.contains(&4));
    let w = Window { tail: 3.., head: ..1 };
    println!("{:?} {:?} {}", &xs[w.tail.clone()], &xs[w.head], head_len(..4));
    println!("{}", (..5) == (..5));
}
