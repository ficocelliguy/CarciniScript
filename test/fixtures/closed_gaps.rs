// Smaller gaps closed together: renamed `Error`s, a `use` inside a block, `#[cfg]` alternatives
// under one name, associated type bounds in return position, a generic type alias and a GAT, a
// struct literal's closure typed by the struct's bound, a `&mut dyn Write` field, `#[cfg]` on a
// macro's arguments, `Option<Option<T>>` through a generic, `as_mut()` passed on, a
// `Vec<&mut i32>` built by hand (also inside a tuple and an enum payload), and `partition` over
// `iter_mut()`.
use std::fmt::Display;
use std::io::Write;

mod a {
    #[derive(Debug)]
    pub struct Error(pub i32);
}
mod b {
    #[derive(Debug)]
    pub struct Error(pub String);
}
use a::Error as AError;
use b::Error as BError;
#[derive(Debug)]
struct Error {
    msg: String,
}
fn fa() -> Result<i32, AError> {
    Err(AError(1))
}
fn fb() -> Result<i32, BError> {
    Err(BError("x".into()))
}
fn fc() -> Result<i32, Error> {
    Err(Error { msg: "y".into() })
}

mod geo {
    pub struct Point {
        pub x: i32,
    }
    pub fn mk(x: i32) -> Point {
        Point { x }
    }
}

mod fast {
    pub fn name() -> &'static str {
        "fast"
    }
    pub struct Impl(pub i32);
}
mod slow {
    pub fn name() -> &'static str {
        "slow"
    }
    pub struct Impl(pub i32);
}
#[cfg(feature = "fast")]
use fast::Impl as Chosen;
#[cfg(not(feature = "fast"))]
use slow::Impl as Chosen;
#[cfg(feature = "fast")]
use fast::name;
#[cfg(not(feature = "fast"))]
use slow::name;

fn evens() -> impl Iterator<Item: Display> {
    (0..3).map(|x| x * 2)
}
fn nested() -> impl Iterator<Item: Iterator<Item: Display>> {
    (0..2).map(|i| (0..i + 1).map(|j| j * 10))
}
fn join<I: Iterator<Item: Display>>(it: I) -> String {
    it.map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}

type List<'a, T: 'a> = &'a [T];
fn total(l: List<'_, i32>) -> i32 {
    l.iter().sum()
}
trait Container {
    type Item<'a>
    where
        Self: 'a;
    fn first<'a>(&'a self) -> Option<Self::Item<'a>>;
}
struct Words(Vec<String>);
impl Container for Words {
    type Item<'a> = &'a str;
    fn first<'a>(&'a self) -> Option<&'a str> {
        self.0.first().map(|s| s.as_str())
    }
}

struct Memo<F: Fn(u64) -> u64> {
    f: F,
    cache: Vec<u64>,
}
impl<F: Fn(u64) -> u64> Memo<F> {
    fn get(&mut self, x: u64) -> u64 {
        let v = (self.f)(x);
        self.cache.push(v);
        v
    }
}
struct Pair<F>
where
    F: Fn(i32, i32) -> i32,
{
    op: F,
}

struct Logger<'a> {
    out: &'a mut dyn Write,
    count: usize,
}
impl<'a> Logger<'a> {
    fn log(&mut self, s: &str) {
        writeln!(self.out, "{}: {}", self.count, s).unwrap();
        self.count += 1;
    }
}

trait Show {
    fn show(&self) -> String;
}
impl Show for i32 {
    fn show(&self) -> String {
        format!("i{}", self)
    }
}
impl<T: Show> Show for Option<T> {
    fn show(&self) -> String {
        match self {
            None => "none".into(),
            Some(x) => format!("some({})", x.show()),
        }
    }
}
fn via<T: Show>(t: &T) -> String {
    t.show()
}

fn bump(o: Option<&mut i32>) {
    if let Some(x) = o {
        *x += 1;
    }
}
enum Slot<'a> {
    Live(&'a mut i32),
    Dead,
}

fn main() {
    println!("{:?} {:?} {:?}", fa(), fb(), fc());
    {
        use geo::Point as P;
        use geo::mk as make;
        let p: P = make(3);
        println!("{}", p.x);
    }
    let q = {
        use std::collections::HashMap as Map;
        let mut m: Map<i32, i32> = Map::new();
        m.insert(1, 2);
        m.len()
    };
    println!("{}", q);
    let c = Chosen(4);
    println!("{} {}", name(), c.0);
    println!("{}", join(evens()));
    for row in nested() {
        println!("{}", join(row));
    }
    let v = vec![1, 2, 3];
    println!("{}", total(&v));
    let w = Words(vec!["hi".into()]);
    println!("{:?}", w.first());
    let mut m = Memo { f: |x| x * x + 1, cache: Vec::new() };
    println!("{} {}", m.get(3), m.cache.len());
    let p = Pair { op: |a, b| a.max(b) };
    println!("{}", (p.op)(2, 5));
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut l = Logger { out: &mut buf, count: 0 };
        l.log("a");
        l.log("b");
    }
    print!("{}", String::from_utf8(buf).unwrap());
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let mut l2 = Logger { out: &mut lock, count: 5 };
    l2.log("c");
    let cfgd = vec![
        1,
        #[cfg(target_os = "nope")]
        99,
        #[cfg(not(target_os = "nope"))]
        3,
    ];
    println!("{:?}", cfgd);
    let a: Option<Option<i32>> = Some(None);
    let n: Option<Option<i32>> = None;
    println!("{} {} {} {}", via(&a), via(&n), a.show(), n.show());
    println!("{:?} {:?} {:?}", a, n, Some(Some(4)));
    let mut o: Option<i32> = Some(1);
    bump(o.as_mut());
    o.as_mut().map(|n| *n += 10);
    if let Some(n) = o.as_mut() {
        *n *= 2;
    }
    let r = o.as_mut();
    bump(r);
    println!("{:?}", o);
    let mut x = 1;
    let mut y = 2;
    let mut z = 3;
    let mut refs: Vec<&mut i32> = Vec::new();
    refs.push(&mut x);
    refs.push(&mut y);
    for r in refs.iter_mut() {
        **r += 10;
    }
    let mut tagged: Vec<(usize, &mut i32)> = vec![(0, &mut z)];
    for (i, r) in tagged.iter_mut() {
        **r += 100 + *i as i32;
    }
    let mut slots = vec![Slot::Live(&mut x), Slot::Dead];
    for s in slots.iter_mut() {
        if let Slot::Live(r) = s {
            **r *= 2;
        }
    }
    println!("{} {} {}", x, y, z);
    let mut parts = vec![1, 2, 3, 4, 5, 6];
    let (evens, odds): (Vec<&mut i32>, Vec<&mut i32>) =
        parts.iter_mut().partition(|x| **x % 2 == 0);
    for e in evens {
        *e *= 10;
    }
    for o in odds {
        *o += 1;
    }
    println!("{:?}", parts);
}
