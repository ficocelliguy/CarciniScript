// Trait features: associated types (`type Unit;`), generic traits (`trait Convert<T>`),
// blanket impls (`impl<T: Display> Shout for T`), `impl FromStr` with `parse`, `Into` /
// `AsRef` parameters and bounds (also in `where`), operators on references, and a struct
// generic over an iterator.
use std::collections::HashMap;
use std::fmt::{self, Debug, Display};
use std::ops::Add;
use std::str::FromStr;

// ---- associated types
trait Shape {
    type Unit: Debug;
    fn area(&self) -> Self::Unit;
    fn name(&self) -> String;
    fn describe(&self) -> String {
        format!("{} with area {:?}", self.name(), self.area())
    }
}

struct Square(f64);

struct Grid {
    w: u32,
    h: u32,
}

impl Shape for Square {
    type Unit = f64;
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn name(&self) -> String {
        "square".into()
    }
}

impl Shape for Grid {
    type Unit = u32;
    fn area(&self) -> Self::Unit {
        self.w * self.h
    }
    fn name(&self) -> String {
        format!("{}x{} grid", self.w, self.h)
    }
}

fn report<S: Shape>(s: &S) -> String {
    format!("{:?} / {}", s.area(), s.describe())
}

trait Container {
    type Item;
    fn items(&self) -> Vec<Self::Item>;
    fn count(&self) -> usize {
        self.items().len()
    }
}

struct Bag {
    words: Vec<String>,
}

impl Container for Bag {
    type Item = String;
    fn items(&self) -> Vec<String> {
        self.words.clone()
    }
}

// ---- generic traits
trait Convert<T> {
    fn convert(&self) -> T;
    fn both(&self) -> (T, T) {
        (self.convert(), self.convert())
    }
}

struct Celsius(f64);

impl Convert<String> for Celsius {
    fn convert(&self) -> String {
        format!("{}C", self.0)
    }
}

impl Convert<f64> for Square {
    fn convert(&self) -> f64 {
        self.0 * 2.0
    }
}

fn label<C: Convert<String>>(c: &C) -> String {
    c.convert()
}

// ---- blanket impls
trait Shout {
    fn shout(&self) -> String;
    fn twice(&self) -> String {
        format!("{} {}", self.shout(), self.shout())
    }
}

impl<T: Display> Shout for T {
    fn shout(&self) -> String {
        format!("{}!", self)
    }
}

trait Describe {
    fn describe_it(&self) -> String;
}

impl<T> Describe for T
where
    T: Debug,
{
    fn describe_it(&self) -> String {
        format!("<{:?}>", self)
    }
}

struct Money(i64);

impl Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "${}", self.0)
    }
}

#[derive(Debug)]
struct Pt {
    x: i32,
}

fn loud<S: Shout>(s: &S) -> String {
    s.twice()
}

// ---- FromStr
#[derive(Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug)]
enum PointError {
    Missing,
    Bad(String),
}

impl Display for PointError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PointError::Missing => write!(f, "missing a comma"),
            PointError::Bad(s) => write!(f, "bad number {:?}", s),
        }
    }
}

impl FromStr for Point {
    type Err = PointError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (a, b) = s.split_once(',').ok_or(PointError::Missing)?;
        let x = a.trim().parse::<i32>().map_err(|_| PointError::Bad(a.to_string()))?;
        let y = b.trim().parse::<i32>().map_err(|_| PointError::Bad(b.to_string()))?;
        Ok(Point { x, y })
    }
}

fn read(lines: &[&str]) -> Result<Vec<Point>, PointError> {
    let mut out = Vec::new();
    for l in lines {
        let p: Point = l.parse()?;
        out.push(p);
    }
    Ok(out)
}

// ---- Into / AsRef
fn greet(name: impl Into<String>) -> String {
    let n: String = name.into();
    format!("hi {}", n)
}

fn shout_str<S: AsRef<str>>(s: S) -> String {
    s.as_ref().to_uppercase()
}

fn total<T: AsRef<[i32]>>(xs: T) -> i32 {
    xs.as_ref().iter().sum()
}

struct Book {
    title: String,
    tags: Vec<String>,
}

impl Book {
    fn new(title: impl Into<String>) -> Book {
        Book { title: title.into(), tags: Vec::new() }
    }

    fn tag<S>(mut self, t: S) -> Book
    where
        S: Into<String>,
    {
        self.tags.push(t.into());
        self
    }
}

fn count_words<S: AsRef<str>>(lines: &[S]) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for l in lines {
        for w in l.as_ref().split_whitespace() {
            *m.entry(w.to_string()).or_insert(0) += 1;
        }
    }
    m
}

// ---- operators on references
#[derive(Debug, Clone, Copy, PartialEq)]
struct V2(i32, i32);

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2(self.0 + o.0, self.1 + o.1)
    }
}

impl<'a> Add<&'a V2> for &'a V2 {
    type Output = V2;
    fn add(self, o: &V2) -> V2 {
        V2(self.0 + o.0, self.1 + o.1)
    }
}

// ---- a struct generic over an iterator
struct Pairs<I: Iterator<Item = i32>> {
    it: I,
}

impl<I: Iterator<Item = i32>> Iterator for Pairs<I> {
    type Item = (i32, i32);
    fn next(&mut self) -> Option<(i32, i32)> {
        Some((self.it.next()?, self.it.next()?))
    }
}

fn main() {
    let s = Square(1.5);
    let g = Grid { w: 3, h: 4 };
    println!("{} | {}", s.describe(), g.describe());
    println!("{} | {}", report(&s), report(&g));
    let a: f64 = s.area();
    let b: u32 = g.area() + 1;
    println!("{} {}", a, b);
    let bag = Bag { words: vec!["a".into(), "b".into()] };
    println!("{:?} {}", bag.items(), bag.count());

    let c = Celsius(21.5);
    println!("{} {:?} {}", c.convert(), c.both(), label(&c));
    let d: f64 = s.convert();
    println!("{} {:?}", d, s.both());

    println!("{} {} {}", 5.shout(), "hey".shout(), 2.5.shout());
    println!("{} {}", Money(30).shout(), Money(7).twice());
    println!("{} {}", loud(&'c'), loud(&Money(1)));
    println!("{} {} {}", Pt { x: 3 }.describe_it(), vec![1, 2].describe_it(), Some(1.0).describe_it());
    let name = String::from("ann");
    println!("{} {}", name.shout(), name.describe_it());

    println!("{:?}", "1, 2".parse::<Point>());
    println!("{:?}", Point::from_str("3,4"));
    match "12".parse::<Point>() {
        Ok(p) => println!("ok {:?}", p),
        Err(e) => println!("error: {}", e),
    }
    println!("{:?}", read(&["1,1", "2, 3"]));
    println!("{:?}", read(&["1,1", "x,3"]).map_err(|e| e.to_string()));

    let owned = String::from("bob");
    println!("{} {}", greet("ann"), greet(owned.clone()));
    println!("{} {}", shout_str("abc"), shout_str(owned));
    println!("{} {}", total(vec![1, 2, 3]), total([4, 5]));
    let book = Book::new("Dune").tag("scifi").tag(String::from("classic"));
    println!("{} {:?}", book.title, book.tags);
    let m = count_words(&["a b a", "b c"]);
    let mut keys: Vec<_> = m.iter().collect();
    keys.sort();
    println!("{:?}", keys);

    let p = V2(1, 2);
    let q = V2(10, 20);
    println!("{:?} {:?} {}", p + q, &p + &q, (&p + &q) == p + q);

    let pairs = Pairs { it: vec![1, 2, 3, 4, 5].into_iter() };
    println!("{:?}", pairs.collect::<Vec<_>>());
    let evens = Pairs { it: (0..10).filter(|x| x % 2 == 0) };
    println!("{:?}", evens.map(|(a, b)| a * b).collect::<Vec<_>>());
}
