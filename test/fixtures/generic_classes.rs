// Erased generics that need the type's class at run time: an enum's `impl<T>` methods with
// `self` (operators, `T::default()`, `parse::<T>()`), a derived `Default` on a generic struct with
// a bare `T` field, `{:?}` of a `T` field in an `impl<T: Debug>` method, a trait implemented for
// both an integer and a float type called through `T: Trait`, and `impl<T> S<Vec<T>>`.
use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Rem, Shl};

enum Tree<T> {
    Leaf(T),
    Node(Box<Tree<T>>, Box<Tree<T>>),
}

impl<T: Copy + Add<Output = T> + Div<Output = T> + Default + std::str::FromStr> Tree<T> {
    fn sum(&self) -> T {
        match self {
            Tree::Leaf(x) => *x,
            Tree::Node(a, b) => a.sum() + b.sum(),
        }
    }
    fn halved(&self, d: T) -> T {
        self.sum() / d
    }
    fn zero(&self) -> T {
        T::default()
    }
    fn parsed(&self, s: &str) -> Option<T> {
        s.parse::<T>().ok()
    }
}

#[derive(Debug, Clone, Copy)]
enum V<T> {
    One(T),
    Two(T, T),
}

impl<T: Copy + Mul<Output = T> + Rem<Output = T> + Shl<u32, Output = T>> V<T> {
    fn scaled(self, k: T) -> V<T> {
        match self {
            V::One(a) => V::One(a * k),
            V::Two(a, b) => V::Two(a * k, b * k),
        }
    }
    fn modk(self, k: T) -> T {
        match self {
            V::One(a) => a % k,
            V::Two(a, _) => a % k,
        }
    }
    fn sh(self) -> T {
        match self {
            V::One(a) => a << 1,
            V::Two(a, _) => a << 1,
        }
    }
}

#[derive(Default, Debug)]
struct History<T> {
    items: Vec<T>,
    last: T,
    n: usize,
}

#[derive(Default, Debug)]
struct Pair<A, B> {
    a: A,
    b: B,
    o: Option<A>,
}

struct W<T> {
    v: T,
}

impl<T: Debug> W<T> {
    fn show(&self) -> String {
        format!("{:?}", self.v)
    }
}

fn show2<T: Debug>(x: T) -> String {
    format!("{:?}", x)
}

trait Money {
    fn money(&self) -> String;
}

impl Money for i32 {
    fn money(&self) -> String {
        format!("${}", self)
    }
}

impl Money for u8 {
    fn money(&self) -> String {
        format!("{}c", self)
    }
}

impl Money for f64 {
    fn money(&self) -> String {
        format!("${:.2}", self)
    }
}

fn pay<T: Money>(x: T) -> String {
    x.money()
}

fn pay_all<T: Money + Copy>(xs: &[T]) -> Vec<String> {
    xs.iter().map(|x| pay(*x)).collect()
}

struct S<T>(T);

impl<T: Copy + Add<Output = T>> S<Vec<T>> {
    fn total(&self, z: T) -> T {
        let mut a = z;
        for x in &self.0 {
            a = a + *x;
        }
        a
    }
}

fn main() {
    let t = Tree::Node(Box::new(Tree::Leaf(7)), Box::new(Tree::Leaf(8)));
    println!("{} {} {:?}", t.halved(4), t.zero(), t.parsed("5"));
    let f = Tree::Leaf(7.0f64);
    println!("{} {:?} {:?}", f.halved(4.0), f.parsed("2.5"), f.parsed("x"));
    let v = V::Two(200u8, 3);
    println!("{:?} {} {}", v.scaled(2), v.modk(7), v.sh());
    let w = V::One(-7i32);
    println!("{:?} {} {}", w.scaled(3), w.modk(4), w.sh());

    let h: History<f64> = Default::default();
    let h2 = History::<i32>::default();
    let p = Pair::<u8, String>::default();
    let mut h3 = History::<String>::default();
    h3.last.push_str("x");
    h3.items.push("y".into());
    println!("{:?} {:?} {:?} {} {:?}", h, h2, p, h.last + 1.5, h3);

    println!(
        "{} {} {} {} {}",
        W { v: 1.0f64 }.show(),
        W { v: 1i32 }.show(),
        W { v: 'c' }.show(),
        show2(2.0f64),
        show2("s")
    );
    let ws = vec![W { v: 3.0f64 }, W { v: 0.5 }];
    println!("{} {}", ws[0].show(), ws[1].show());

    println!("{} {} {} {}", pay(2.0f64), pay(2i32), pay(2u8), 3.0f64.money());
    println!("{:?} {:?}", pay_all(&[1.0, 2.5]), pay_all(&[1i32, 2]));

    println!("{} {}", S(vec![1, 2, 3]).total(0), S(vec![1.5, 2.5]).total(0.0));
}
