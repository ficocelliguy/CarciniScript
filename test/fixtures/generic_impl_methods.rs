// An `impl<T>`'s `T` in methods with `self`: `/`, `%`, wrapping `+`, `parse::<T>()`, `T::default()`,
// `!` and `<<` by the class each value carries, also for values made in generic functions.
use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Not, Rem, Shl, Sub};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq)]
struct V2(i32, i32);

impl Div for V2 {
    type Output = V2;
    fn div(self, o: V2) -> V2 {
        V2(self.0 / o.0, self.1 / o.1)
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2(self.0 + o.0, self.1 + o.1)
    }
}

#[derive(Debug, Clone)]
struct Stats<T> {
    items: Vec<T>,
}

impl<T: Copy + Add<Output = T> + Div<Output = T> + Debug> Stats<T> {
    fn new() -> Self {
        Stats { items: Vec::new() }
    }
    fn push(&mut self, x: T) {
        self.items.push(x);
    }
    fn halves(&self, two: T) -> Vec<T> {
        self.items.iter().map(|&x| x / two).collect()
    }
    fn pair_sum(&self) -> T {
        self.items[0] + self.items[1]
    }
}

struct Acc<T> {
    total: T,
}

impl<T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T> + Rem<Output = T> + Default> Acc<T> {
    fn add(&mut self, x: T) -> &mut Self {
        self.total = self.total + x;
        self
    }
    fn scaled(&self, k: T) -> T {
        self.total * k
    }
    fn rem(&self, k: T) -> T {
        self.total % k
    }
    fn reset(&mut self) {
        self.total = T::default();
    }
}

#[derive(Debug, Default)]
struct Parser<T> {
    seen: Vec<T>,
}

impl<T: FromStr + Debug> Parser<T> {
    fn feed(&mut self, s: &str) -> bool {
        match s.parse::<T>() {
            Ok(v) => {
                self.seen.push(v);
                true
            }
            Err(_) => false,
        }
    }
}

struct Bits<T>(T);

impl<T: Copy + Not<Output = T> + Shl<u32, Output = T> + Debug> Bits<T> {
    fn inv(&self) -> T {
        !self.0
    }
    fn up(&self, n: u32) -> T {
        self.0 << n
    }
    fn doubled(&self) -> Bits<T> {
        Bits(self.up(1))
    }
}

fn halve_all<T: Copy + Div<Output = T>>(xs: &[T], two: T) -> Vec<T> {
    xs.iter().map(|&x| x / two).collect()
}

struct Wrap<T> {
    xs: Vec<T>,
}

impl<T: Copy + Div<Output = T>> Wrap<T> {
    fn halved(&self, two: T) -> Vec<T> {
        halve_all(&self.xs, two)
    }
}

fn make_stats<T: Copy + Add<Output = T> + Div<Output = T> + Debug>(xs: &[T]) -> Stats<T> {
    let mut s = Stats::new();
    for &x in xs {
        s.push(x);
    }
    s
}

fn main() {
    let mut s: Stats<i32> = Stats::new();
    s.push(7);
    s.push(-9);
    println!("{:?} {}", s.halves(2), s.pair_sum());
    let f = make_stats(&[7.0, 3.0]);
    println!("{:?}", f.halves(2.0));
    let v = make_stats(&[V2(8, 6), V2(2, 4)]);
    println!("{:?} {:?}", v.halves(V2(2, 2)), v.pair_sum());
    let b = make_stats(&[200u8, 100]);
    println!("{} {:?}", b.pair_sum(), b.clone().halves(3));
    let mut a = Acc { total: 250u8 };
    a.add(3).add(4);
    println!("{} {} {}", a.total, a.scaled(2), a.rem(7));
    a.reset();
    println!("{}", a.total);
    let mut a = Acc { total: 7i64 };
    println!("{} {}", a.add(-20).rem(4), a.scaled(3));
    let mut p: Parser<u8> = Parser::default();
    println!("{} {} {:?}", p.feed("200"), p.feed("300"), p.seen);
    let mut q = Parser { seen: vec![1.5f64] };
    q.feed("2.25");
    println!("{:?}", q.seen);
    let x = Bits(5u8);
    println!("{} {} {:?} {}", x.inv(), x.up(6), x.doubled().0, Bits(1i32).inv());
    let w = Wrap { xs: vec![9, 5] };
    println!("{:?} {:?}", w.halved(2), Wrap { xs: vec![9.0] }.halved(2.0));
}
