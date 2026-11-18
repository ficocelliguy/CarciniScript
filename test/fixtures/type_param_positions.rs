// Type parameters as a trait's arguments (`impl<T> Store<T> for Bag<T>`, `S: Store<T>`, whose `T`
// a call settles from `S`'s impl), as an impl's associated type (`type Item = T;`), in a method's
// `where F: FnOnce() -> T` (on a trait, an impl and an inherent method), and `T: Into<i64>` /
// `Item = P` with `P: Into<i64>` widening narrower numbers.
use std::fmt::Debug;
use std::path::PathBuf;

trait Store<T> {
    fn put(&mut self, v: T);
    fn count(&self) -> usize;
}

struct Bag<T> {
    items: Vec<T>,
}

impl<T> Store<T> for Bag<T> {
    fn put(&mut self, v: T) {
        self.items.push(v);
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

fn fill<T: Clone, S: Store<T>>(s: &mut S, v: T, n: usize) {
    for _ in 0..n {
        s.put(v.clone());
    }
}

trait Container {
    type Item;
    fn get(&self, i: usize) -> Option<&Self::Item>;
    fn first(&self) -> Option<&Self::Item> {
        self.get(0)
    }
}

struct Stack<T> {
    v: Vec<T>,
}

impl<T> Container for Stack<T> {
    type Item = T;
    fn get(&self, i: usize) -> Option<&T> {
        self.v.get(i)
    }
}

fn show_first<C: Container>(c: &C) -> String
where
    C::Item: Debug,
{
    format!("{:?}", c.first())
}

fn take_first<C>(c: &C) -> Option<C::Item>
where
    C: Container,
    C::Item: Clone,
{
    c.first().cloned()
}

trait Alloc<T> {
    fn alloc(&mut self, v: T) -> usize;
}

struct Arena<'a, T, U> {
    slots: Vec<T>,
    tag: &'a U,
}

impl<'a, T, U: Debug> Alloc<T> for Arena<'a, T, U> {
    fn alloc(&mut self, v: T) -> usize {
        self.slots.push(v);
        println!("alloc in {:?}", self.tag);
        self.slots.len() - 1
    }
}

fn alloc_two<T, A: Alloc<T>>(a: &mut A, x: T, y: T) -> (usize, usize) {
    (a.alloc(x), a.alloc(y))
}

trait Mapper<In> {
    type Out;
    fn map_one(&self, x: In) -> Self::Out;
}

struct Doubler;

impl Mapper<i32> for Doubler {
    type Out = i64;
    fn map_one(&self, x: i32) -> i64 {
        x as i64 * 2
    }
}

struct Pair<T>(T);

impl<T: Clone> Mapper<T> for Pair<T> {
    type Out = (T, T);
    fn map_one(&self, x: T) -> (T, T) {
        (self.0.clone(), x)
    }
}

fn apply<M: Mapper<i32>>(m: &M, xs: &[i32]) -> Vec<M::Out> {
    xs.iter().map(|&x| m.map_one(x)).collect()
}

trait Runner {
    fn run<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T;
}

struct Logged {
    name: String,
}

impl Runner for Logged {
    fn run<F, T>(&self, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        println!("start {}", self.name);
        let r = f();
        println!("end {}", self.name);
        r
    }
}

struct Pool;

impl Pool {
    fn install<OP, R>(&self, op: OP) -> R
    where
        OP: FnOnce() -> R,
    {
        op()
    }
}

fn widen<P: Into<i64>>(p: P) -> i64 {
    p.into() * 2
}

fn halve<P: Into<f64>>(p: P) -> f64 {
    p.into() / 2.0
}

fn bump<P>(p: P) -> i32
where
    P: Into<i32>,
{
    p.into() + 1
}

fn shift(p: impl Into<u128>) -> u128 {
    p.into() << 70
}

fn sum_from<I, P>(it: I) -> i64
where
    I: IntoIterator<Item = P>,
    P: Into<i64>,
{
    let mut s = 0;
    for p in it {
        s += p.into();
    }
    s
}

// A bound naming a parameter declared after it (`F: FnOnce() -> P` with `P: Into<PathBuf>`).
trait WithPath {
    fn with_path<F, P>(self, path: F) -> String
    where
        F: FnOnce() -> P,
        P: Into<PathBuf>;
}

struct Failure(String);

impl WithPath for Failure {
    fn with_path<F, P>(self, path: F) -> String
    where
        F: FnOnce() -> P,
        P: Into<PathBuf>,
    {
        let p: PathBuf = path().into();
        format!("{} at {}", self.0, p.display())
    }
}

fn label<F, P>(f: F) -> String
where
    F: FnOnce() -> P,
    P: Into<String>,
{
    let s: String = f().into();
    format!("<{}>", s)
}

fn first_of<T, A>(make: T) -> Option<i32>
where
    A: Iterator<Item = i32>,
    T: Fn(i32) -> A,
{
    make(3).next()
}

fn main() {
    let failure = Failure("oops".into());
    println!("{}", failure.with_path(|| PathBuf::from("/tmp/x")));
    println!("{} {:?}", label(|| "hi"), first_of(|n| n..n + 2));

    let mut b = Bag { items: Vec::new() };
    fill(&mut b, "x".to_string(), 3);
    println!("{} {:?}", b.count(), b.items);

    let s = Stack { v: vec![3, 4] };
    println!("{}", show_first(&s));
    let e: Stack<String> = Stack { v: vec![] };
    println!("{}", show_first(&e));
    let w = Stack { v: vec!["x".to_string()] };
    let f: Option<String> = take_first(&w);
    println!("{:?}", f);

    let tag = "main".to_string();
    let mut a = Arena { slots: Vec::new(), tag: &tag };
    println!("{:?}", alloc_two(&mut a, 1.5, 2.5));
    println!("{:?}", a.slots);

    println!("{:?}", apply(&Doubler, &[1, 2, 3]));
    println!("{:?}", apply(&Pair(9), &[1, 2]));
    println!("{:?}", Pair("a").map_one("b"));

    let l = Logged { name: "job".into() };
    let v = l.run(|| 40 + 2);
    println!("{}", v);
    let t: String = l.run(|| "hi".to_string());
    println!("{}", t);
    println!("{}", Pool.install(|| 7 * 6));

    let n: i32 = 3;
    let byte: u8 = 200;
    println!("{} {} {} {}", widen(n), widen(byte), widen(4u8), widen(1i64 << 40));
    println!("{}", widen(true));
    println!("{} {} {}", halve(n), halve(2.5f32), halve(7u16));
    println!("{} {}", bump(true), bump(-4i8));
    println!("{} {}", shift(3u8), shift(5u64));
    println!("{} {}", sum_from(vec![1i32, 2, 3]), sum_from([4u8, 5]));
    println!("{} {}", sum_from(vec![7i64]), sum_from(vec![true, false, true]));
}
