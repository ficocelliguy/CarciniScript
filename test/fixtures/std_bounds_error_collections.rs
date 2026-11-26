// Std traits as bounds on type parameters: `E: Error` (`Display` / `Debug` through it, `source()`),
// `C: Extend<T>` (a `Vec`, `String`, set, map or your own impl, by the value), `C: FromIterator<T>`
// (`collect()` and `C::from_iter` by `C`'s class, also maps and `VecDeque`), and `T: Any` (`&T as
// &dyn Any`, `Box<dyn Any>`); `R: Read`, `R: BufRead` and `W: Write` beside them.
use std::any::Any;
use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, Read, Write};

fn count<I: IntoIterator<Item = u8>>(i: I) -> usize {
    i.into_iter().count()
}
fn sum_all<I>(i: I) -> i32
where
    I: IntoIterator<Item = i32>,
{
    let mut t = 0;
    for x in i {
        t += x;
    }
    t
}
#[derive(Debug)]
struct Inner;
impl fmt::Display for Inner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "inner")
    }
}
impl Error for Inner {}
#[derive(Debug)]
struct Outer(Inner);
impl fmt::Display for Outer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "outer")
    }
}
impl Error for Outer {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}
fn has_source<E: Error + 'static>(e: &E) -> bool {
    e.source().is_some()
}
fn describe<E: Error>(e: E) -> String {
    format!("error: {}", e)
}
fn extend_with<C: Extend<u32>>(c: &mut C) {
    c.extend(vec![1, 2, 3]);
}
fn build<C: FromIterator<u32>>() -> C {
    (1..4).collect()
}
fn build2<C: FromIterator<u32>>() -> C {
    C::from_iter(vec![7, 8])
}
fn read_all<R: Read>(mut r: R) -> String {
    let mut s = String::new();
    r.read_to_string(&mut s).unwrap();
    s
}
fn first_line<R: BufRead>(r: R) -> String {
    r.lines().next().unwrap().unwrap()
}
fn put<W: Write>(w: &mut W) -> io::Result<()> {
    w.write_all(b"hi")?;
    w.flush()
}

fn main() {
    println!("{}", count(vec![1u8, 2, 3]));
    println!("{}", sum_all([4, 5, 6]));
    println!("{} {}", has_source(&Outer(Inner)), has_source(&Inner));
    println!("{}", describe(Inner));
    let mut v: Vec<u32> = Vec::new();
    extend_with(&mut v);
    println!("{:?}", v);
    let b: Vec<u32> = build();
    let c: Vec<u32> = build2();
    println!("{:?} {:?}", b, c);
    println!("{}", read_all("abc".as_bytes()));
    println!("{}", first_line("l1\nl2".as_bytes()));
    let mut out: Vec<u8> = Vec::new();
    put(&mut out).unwrap();
    println!("{:?}", out);
    more();
    any_part();
}
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
#[derive(Debug, Default)]
struct Bag(Vec<u32>);
impl FromIterator<u32> for Bag {
    fn from_iter<I: IntoIterator<Item = u32>>(i: I) -> Self {
        Bag(i.into_iter().map(|x| x * 10).collect())
    }
}
impl Extend<u32> for Bag {
    fn extend<I: IntoIterator<Item = u32>>(&mut self, i: I) {
        for x in i {
            self.0.push(x + 1);
        }
    }
}
fn ext_chars<C: Extend<char>>(c: &mut C) {
    c.extend("ab".chars());
}
fn pairs<M: FromIterator<(u32, u32)>>() -> M {
    (1..3).map(|x| (x, x * x)).collect()
}
fn twice<C: FromIterator<u32> + Extend<u32>>(xs: &[u32]) -> C {
    let mut c: C = xs.iter().copied().collect();
    c.extend(xs.iter().copied());
    c
}
pub fn more() {
    let mut s = String::from("x");
    ext_chars(&mut s);
    let mut v: Vec<char> = vec![];
    ext_chars(&mut v);
    println!("{} {:?}", s, v);
    let m: HashMap<u32, u32> = pairs();
    let b: BTreeMap<u32, u32> = pairs();
    println!("{} {:?}", m[&2], b);
    let h: HashSet<u32> = build();
    let d: VecDeque<u32> = build2();
    let bag: Bag = build();
    let bag2: Bag = build2();
    println!("{} {:?} {:?} {:?}", h.len(), d, bag, bag2);
    let mut bag3 = Bag::default();
    extend_with(&mut bag3);
    let mut hs: HashSet<u32> = HashSet::new();
    extend_with(&mut hs);
    println!("{:?} {}", bag3, hs.len());
    let t: Vec<u32> = twice(&[4, 5]);
    let t2: Bag = twice(&[4, 5]);
    println!("{:?} {:?}", t, t2);
}
fn is_string<T: Any>(t: &T) -> bool {
    (t as &dyn Any).is::<String>()
}
fn as_i32<T: Any>(t: &T) -> Option<&i32> {
    let a: &dyn Any = t;
    a.downcast_ref::<i32>()
}
fn boxed<T: Any>(t: T) -> Box<dyn Any> {
    Box::new(t)
}
fn any_part() {
    println!("{} {}", is_string(&"x".to_string()), is_string(&5));
    println!("{:?} {:?}", as_i32(&7), as_i32(&"no"));
    println!("{}", boxed(3.5f64).is::<f64>());
}
