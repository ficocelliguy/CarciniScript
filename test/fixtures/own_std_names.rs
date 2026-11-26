// A file's own types named like std or bundled ones (env_logger's `Formatter`, winnow's `Range`,
// jiff's `Error`, bumpalo's `String`, hashbrown's `HashMap`): the file's definition wins over the
// prelude and the models, and the qualified std names stay std's.
use std::fmt;

pub struct Formatter<'a> {
    buf: &'a mut std::string::String,
    pub indent: usize,
}
impl<'a> Formatter<'a> {
    pub fn write_str(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.buf.push(' ');
        }
        self.buf.push_str(s);
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}
impl Range {
    pub fn len(&self) -> usize {
        self.end - self.start
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub pattern: std::string::String,
    pub span: Range,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error at {}..{}: {:?}", self.span.start, self.span.end, self.pattern)
    }
}
impl std::error::Error for Error {}

pub fn parse(s: &str) -> Result<usize, Error> {
    if s.is_empty() {
        return Err(Error { pattern: s.to_string(), span: Range { start: 0, end: 0 } });
    }
    Ok(s.len())
}

#[derive(Debug)]
pub struct HashMap<K, V> {
    entries: Vec<(K, V)>,
}
impl<K: PartialEq, V> HashMap<K, V> {
    pub fn new() -> Self {
        HashMap { entries: Vec::new() }
    }
    pub fn insert_unique_unchecked(&mut self, k: K, v: V) -> (&K, &V) {
        self.entries.push((k, v));
        let e = self.entries.last().unwrap();
        (&e.0, &e.1)
    }
    pub fn get(&self, k: &K) -> Option<&V> {
        self.entries.iter().find(|e| e.0 == *k).map(|e| &e.1)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Debug, Clone, Default)]
pub struct String {
    bytes: Vec<u8>,
}
impl String {
    pub fn from_str_in(s: &str) -> String {
        String { bytes: s.as_bytes().to_vec() }
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes).unwrap()
    }
}

fn main() {
    let mut s = std::string::String::new();
    let mut f = Formatter { buf: &mut s, indent: 2 };
    f.write_str("hi");
    println!("{} {}", f.len(), f.indent);
    println!("{:?}", s);

    let r = Range { start: 1, end: 4 };
    println!("{:?} {}", r, r.len());
    let std_range: std::ops::Range<usize> = 0..3;
    println!("{:?} {}", std_range, std_range.len());

    match parse("") {
        Ok(n) => println!("ok {}", n),
        Err(e) => println!("{} / {:?}", e, e),
    }
    println!("{:?}", parse("abc"));
    let boxed: Box<dyn std::error::Error> = Box::new(parse("").unwrap_err());
    println!("{}", boxed);

    let mut m = HashMap::new();
    m.insert_unique_unchecked(1u8, "one");
    let (k, v) = m.insert_unique_unchecked(2u8, "two");
    println!("{} {}", k, v);
    println!("{:?} {:?} {}", m.get(&1), m.get(&3), m.len());
    println!("{:?}", m);
    let mut std_map = std::collections::HashMap::new();
    std_map.insert("a", 1);
    println!("{:?}", std_map.get("a"));

    let own = String::from_str_in("bump");
    let std_s: std::string::String = format!("{}!", own.as_str());
    println!("{} {:?} {:?}", std_s, own, String::default());
}
