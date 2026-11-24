// A file's own `Vec` (bumpalo's `collections/vec.rs`): the program's `Vec` is its struct,
// `std::vec::Vec` is still std's, and the io prelude compiled in after the program keeps
// std's `Vec::new()`.
use std::fmt::Write as _;
use std::io::{self, BufRead, Read, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct Vec<T> {
    items: std::vec::Vec<T>,
    tag: &'static str,
}

impl<T: Clone> Vec<T> {
    pub fn new_in(tag: &'static str) -> Vec<T> {
        Vec { items: std::vec::Vec::new(), tag }
    }
    pub fn push(&mut self, t: T) {
        self.items.push(t);
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn to_std(&self) -> std::vec::Vec<T> {
        self.items.clone()
    }
}

impl io::Write for Vec<u8> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        for x in b {
            self.push(*x);
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

macro_rules! bvec {
    ($tag:expr; $($x:expr),*) => {{
        let mut v = Vec::new_in($tag);
        $(v.push($x);)*
        v
    }};
}

fn main() {
    let mut v: Vec<u8> = Vec::new_in("bump");
    v.write_all(b"hello").unwrap();
    write!(v, " {}", 42).unwrap();
    println!("{} {:?}", v.len(), String::from_utf8(v.to_std()).unwrap());

    let w: Vec<i32> = bvec!["w"; 1, 2, 3];
    println!("{:?}", w);
    assert_eq!(w.clone(), bvec!["w"; 1, 2, 3]);

    // std's `Vec` through the io prelude: lines and bytes of a reader.
    let text = "a\nbb\nccc\n";
    let lines: std::vec::Vec<String> = io::Cursor::new(text).lines().map(|l| l.unwrap()).collect();
    let bytes: std::vec::Vec<u8> = io::Cursor::new("xy").bytes().map(|b| b.unwrap()).collect();
    let mut s = String::new();
    write!(s, "{:?} {:?}", lines, bytes).unwrap();
    println!("{}", s);
}
