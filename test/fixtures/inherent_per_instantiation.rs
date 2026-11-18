// Inherent impls for different instantiations of one type may define the same method: the
// receiver's (or the path's) type arguments pick the impl.
use std::fmt::Debug;

struct W<T>(T);

impl W<i32> {
    fn get(&self) -> i32 {
        self.0 * 2
    }
    fn kind() -> &'static str {
        "int"
    }
}

impl W<String> {
    fn get(&self) -> String {
        format!("<{}>", self.0)
    }
    fn kind() -> &'static str {
        "string"
    }
    fn describe(&self) -> String {
        format!("{} {}", Self::kind(), self.get())
    }
}

struct Slim<V, const N: usize> {
    vals: Vec<V>,
}

impl<V: Copy + Debug> Slim<V, 1> {
    fn new(v: V) -> Self {
        Slim { vals: vec![v] }
    }
    fn find(&self) -> String {
        format!("one {:?} {}", self.vals, Self::width())
    }
    fn width() -> usize {
        1
    }
}

impl<V: Copy + Debug> Slim<V, 2> {
    fn new(v: V) -> Self {
        Slim { vals: vec![v, v] }
    }
    fn find(&self) -> String {
        format!("two {:?} {}", self.vals, Self::width())
    }
    fn width() -> usize {
        2
    }
}

fn generic_find<V: Copy + Debug>(s: &Slim<V, 2>) -> String {
    s.find()
}

struct Pair<A, B>(A, B);

impl Pair<u8, String> {
    fn show(&self) -> String {
        format!("u8/String {} {}", self.0, self.1)
    }
}

impl Pair<String, u8> {
    fn show(&self) -> String {
        format!("String/u8 {} {}", self.0, self.1)
    }
}

impl Pair<Vec<i32>, bool> {
    fn show(&self) -> String {
        format!("vec {:?} {}", self.0, self.1)
    }
}

struct Num<T>(T);

impl Num<f64> {
    fn half(&self) -> f64 {
        self.0 / 2.0
    }
}

impl Num<i64> {
    fn half(&self) -> i64 {
        self.0 / 2
    }
}

fn main() {
    let a = W(21);
    let b = W("hi".to_string());
    println!("{} {}", a.get(), b.get());
    println!("{} {}", W::<i32>::kind(), W::<String>::kind());
    println!("{}", b.describe());
    let ws = vec![W(1), W(2)];
    let t: i32 = ws.iter().map(|w| w.get()).sum();
    println!("{}", t);

    let s1: Slim<u8, 1> = Slim::<u8, 1>::new(3);
    let s2 = Slim::<i16, 2>::new(-4);
    println!("{} | {}", s1.find(), s2.find());
    println!("{}", generic_find(&s2));
    println!("{} {}", Slim::<i32, 1>::width(), Slim::<i32, 2>::width());
    println!("{}", Slim::<i16, 2>::find(&s2));

    let p = Pair(1u8, "s".to_string());
    let q = Pair("t".to_string(), 2u8);
    let r = Pair(vec![1, 2], true);
    println!("{} / {} / {}", p.show(), q.show(), r.show());

    let n = Num(7.0);
    let m = Num(7i64);
    let k: Num<i64> = Num(9);
    println!("{} {} {}", n.half(), m.half(), k.half());
}
