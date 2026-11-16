// Struct values carrying `T`'s class through trait impls (`dyn`, `Display`, `Iterator`), struct
// update syntax, nested generic structs, hashing and equality.
use std::collections::HashSet;
use std::fmt;
use std::ops::{Add, Div, Rem};

trait Halve {
    fn halve(&self) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Num<T> {
    v: T,
    d: T,
}

impl<T: Copy + Div<Output = T> + fmt::Display> Halve for Num<T> {
    fn halve(&self) -> String {
        format!("{}", self.v / self.d)
    }
}

impl<T: Copy + Div<Output = T> + Rem<Output = T> + fmt::Display> fmt::Display for Num<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} r {}", self.v / self.d, self.v % self.d)
    }
}

impl<T: Copy + Div<Output = T>> Num<T> {
    fn with_v(&self, v: T) -> Self {
        Num { v, ..*self }
    }
    fn quot(&self) -> T {
        self.v / self.d
    }
}

struct Countdown<T> {
    n: T,
    step: T,
    left: usize,
}

impl<T: Copy + Div<Output = T>> Iterator for Countdown<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        self.n = self.n / self.step;
        Some(self.n)
    }
}

struct Outer<T> {
    xs: Vec<T>,
}

impl<T: Copy + Div<Output = T> + Add<Output = T>> Outer<T> {
    fn first_num(&self) -> Num<T> {
        Num { v: self.xs[0], d: self.xs[1] }
    }
    fn total_quot(&self) -> T {
        self.first_num().quot() + self.xs[2]
    }
}

fn mk<T>(v: T, d: T) -> Num<T> {
    Num { v, d }
}

fn describe(h: &dyn Halve) -> String {
    h.halve()
}

fn main() {
    let a = Num { v: 7, d: 2 };
    let b = Num { v: 7.0, d: 2.0 };
    let hs: Vec<Box<dyn Halve>> = vec![Box::new(a), Box::new(b), Box::new(mk(-7i8, 2))];
    for h in &hs {
        print!("{} ", h.halve());
    }
    println!();
    println!("{} | {} | {}", a, Num { v: 17u64, d: 5 }, describe(&mk(9u16, 4)));
    println!("{:?} {}", a.with_v(9), a.with_v(9).quot());
    let c: Vec<u32> = Countdown { n: 1000u32, step: 3, left: 4 }.collect();
    let f: Vec<f64> = Countdown { n: 1.0, step: 4.0, left: 2 }.collect();
    println!("{:?} {:?}", c, f);
    let o = Outer { xs: vec![9, 2, 1] };
    println!("{} {}", o.total_quot(), Outer { xs: vec![9.0, 2.0, 1.0] }.total_quot());
    let mut set = HashSet::new();
    set.insert(mk(1, 2));
    set.insert(Num { v: 1, d: 2 });
    set.insert(a.with_v(1).with_v(1));
    println!("{} {}", set.len(), mk(3, 4) == Num { v: 3, d: 4 });
}
