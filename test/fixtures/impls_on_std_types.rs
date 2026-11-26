// Impls on std types and the derives that lean on them: `impl Clone for Box<dyn Trait>` and
// `#[derive(Clone)]` on a struct holding one, `impl Ext for fmt::Formatter`, a trait for
// `Option<T>`, and one trait for both `str` and `String` (or `[T]` and `Vec<T>`) whose bodies
// do the same thing.
use std::fmt;

pub trait Finder: fmt::Debug {
    fn find(&self, h: &str) -> Option<usize>;
    fn bump(&mut self);
    fn clone_box(&self) -> Box<dyn Finder>;
}
impl Clone for Box<dyn Finder> {
    fn clone(&self) -> Box<dyn Finder> {
        self.clone_box()
    }
}

#[derive(Debug, Clone)]
struct ByteFinder {
    b: char,
    hits: u32,
}
impl Finder for ByteFinder {
    fn find(&self, h: &str) -> Option<usize> {
        h.find(self.b)
    }
    fn bump(&mut self) {
        self.hits += 1;
    }
    fn clone_box(&self) -> Box<dyn Finder> {
        Box::new(self.clone())
    }
}

// Not `Clone` itself: `clone_box` builds a fresh one.
#[derive(Debug)]
struct Counter {
    n: u32,
}
impl Finder for Counter {
    fn find(&self, _: &str) -> Option<usize> {
        Some(self.n as usize)
    }
    fn bump(&mut self) {
        self.n += 10;
    }
    fn clone_box(&self) -> Box<dyn Finder> {
        Box::new(Counter { n: self.n + 1 })
    }
}

#[derive(Clone, Debug)]
pub struct Prefilter {
    finder: Box<dyn Finder>,
    len: usize,
}

pub trait FormatterExt {
    fn pad_num(&mut self, n: u32) -> fmt::Result;
}
impl FormatterExt for fmt::Formatter<'_> {
    fn pad_num(&mut self, n: u32) -> fmt::Result {
        write!(self, "{:04}", n)
    }
}
pub struct Year(u32);
impl fmt::Display for Year {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Y")?;
        f.pad_num(self.0)
    }
}

pub trait Describe {
    fn describe(&self) -> String;
}
impl<T: Describe> Describe for Option<T> {
    fn describe(&self) -> String {
        match self {
            Some(x) => format!("some {}", x.describe()),
            None => "none".to_string(),
        }
    }
}
impl Describe for u8 {
    fn describe(&self) -> String {
        format!("u8 {}", self)
    }
}

pub trait Weigh {
    fn weigh(&self) -> usize;
}
impl Weigh for str {
    fn weigh(&self) -> usize {
        self.len() * 2 + self.chars().filter(|c| c.is_uppercase()).count()
    }
}
impl Weigh for String {
    fn weigh(&self) -> usize {
        self.as_str().len() * 2 + self.as_str().chars().filter(|c| c.is_uppercase()).count()
    }
}
impl Weigh for [u8] {
    fn weigh(&self) -> usize {
        self.iter().map(|&b| b as usize).sum()
    }
}
impl Weigh for Vec<u8> {
    fn weigh(&self) -> usize {
        self[..].iter().map(|&b| b as usize).sum()
    }
}

fn main() {
    let p = Prefilter { finder: Box::new(ByteFinder { b: 'c', hits: 0 }), len: 3 };
    let mut q = p.clone();
    q.finder.bump();
    q.len += 1;
    println!("{:?}", p);
    println!("{:?}", q);
    println!("{:?} {:?}", p.finder.find("abc"), q.finder.find("xyz"));

    let c = Prefilter { finder: Box::new(Counter { n: 5 }), len: 0 };
    let mut d = c.clone();
    d.finder.bump();
    println!("{:?} {:?}", c, d);
    let mut b: Box<dyn Finder> = Box::new(Counter { n: 1 });
    let b2 = b.clone();
    b.bump();
    println!("{:?} {:?}", b, b2);

    println!("{} [{:>8}]", Year(7), Year(42).to_string());

    let o: Option<u8> = Some(3);
    let n: Option<u8> = None;
    println!("{} / {}", o.describe(), n.describe());

    let s = String::from("HeLLo");
    println!("{} {} {}", "AbC".weigh(), s.weigh(), s.as_str().weigh());
    let v: Vec<u8> = vec![1, 2, 3];
    println!("{} {}", v.weigh(), v[1..].weigh());
}
