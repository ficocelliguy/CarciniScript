// Your traits on more std types: `VecDeque` (its own impl apart from `Vec`'s, also through a
// generic function), `Pin<Box<T>>` / `Box<T>` (the contents' impl), `str` next to a `String`
// impl that hands on to it, and `Box<dyn Trait>` fields declared before the trait.
use std::collections::VecDeque;
use std::pin::Pin;

struct Scene {
    shapes: Vec<Box<dyn Shape>>,
}

struct Single(Box<dyn Shape>);

trait Shape {
    fn area(&self) -> f64;
}

struct Sq(f64);

impl Shape for Sq {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}

trait Total {
    fn total(&self) -> i64;
    fn describe(&self) -> String {
        format!("total={}", self.total())
    }
}

impl Total for Vec<i64> {
    fn total(&self) -> i64 {
        self.iter().sum()
    }
}

impl Total for VecDeque<i64> {
    fn total(&self) -> i64 {
        self.iter().sum::<i64>() * 100
    }
}

trait Rotate {
    fn spin(&mut self);
}

impl<T> Rotate for VecDeque<T> {
    fn spin(&mut self) {
        if let Some(x) = self.pop_front() {
            self.push_back(x);
        }
    }
}

fn show<T: Total>(t: &T) -> String {
    t.describe()
}

fn spin_twice<R: Rotate>(r: &mut R) {
    r.spin();
    r.spin();
}

trait Poke {
    fn poke(&self) -> i32;
}

struct Counter(i32);

impl Poke for Pin<Box<Counter>> {
    fn poke(&self) -> i32 {
        self.0 + 1
    }
}

struct Node(i32);

impl Poke for Box<Node> {
    fn poke(&self) -> i32 {
        self.0 * 2
    }
}

trait Shout {
    fn shout(&self) -> String;
    fn twice(&self) -> String {
        format!("{} {}", self.shout(), self.shout())
    }
}

impl Shout for str {
    fn shout(&self) -> String {
        self.to_uppercase()
    }
}

impl Shout for String {
    fn shout(&self) -> String {
        self.as_str().shout()
    }
}

trait CountA {
    fn count_a(&self) -> usize;
}

impl<'a> CountA for &'a str {
    fn count_a(&self) -> usize {
        self.matches('a').count()
    }
}

impl CountA for String {
    fn count_a(&self) -> usize {
        CountA::count_a(&self.as_str())
    }
}

fn loud<S: Shout + ?Sized>(s: &S) -> String {
    s.twice()
}

fn main() {
    let v = vec![1i64, 2, 3];
    let mut d: VecDeque<i64> = VecDeque::new();
    d.push_back(4);
    d.push_front(5);
    println!("{} {}", v.total(), d.total());
    println!("{} {}", show(&v), show(&d));
    let mut q: VecDeque<&str> = ["a", "b", "c"].into_iter().collect();
    q.spin();
    println!("{:?}", q);
    spin_twice(&mut q);
    println!("{:?}", q);

    let p = Box::pin(Counter(41));
    println!("{}", p.poke());
    let n = Box::new(Node(21));
    println!("{}", n.poke());

    let s = String::from("abc");
    println!("{} {}", s.shout(), "hey".shout());
    println!("{} {}", loud(&s), loud("x"));
    println!("{} {}", s.count_a(), "banana".count_a());

    let scene = Scene {
        shapes: vec![Box::new(Sq(2.0)), Box::new(Sq(0.5))],
    };
    let total: f64 = scene.shapes.iter().map(|s| s.area()).sum();
    println!("{}", total);
    let one = Single(Box::new(Sq(3.0)));
    println!("{}", one.0.area());
}
