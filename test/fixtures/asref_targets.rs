// `.as_ref()` on a parameter bounded by `AsRef<X>` behind a reference (`&'a S` with
// `S: AsRef<OsStr> + ?Sized`), `s.as_ref()` on a string as a `&[u8]` or `&OsStr` with
// `std::path` in use, a blanket `impl<H: AsRef<[u8]>> From<&H>`, and a trait's generic function
// called through a type parameter with a turbofish (its `Into` / `Fn` / `Default` bounds).
use std::ffi::OsStr;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

struct Builder<'a> {
    prefix: &'a OsStr,
    suffix: &'a OsStr,
}

impl<'a> Builder<'a> {
    fn prefix<S: AsRef<OsStr> + ?Sized>(&mut self, prefix: &'a S) -> &mut Self {
        self.prefix = prefix.as_ref();
        self
    }
    fn suffix<S: AsRef<OsStr> + ?Sized>(&mut self, suffix: &'a S) -> &mut Self {
        self.suffix = suffix.as_ref();
        self
    }
    fn len(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }
}

struct Name {
    name: PathBuf,
}

impl Name {
    fn from_static_ref(s: &'static OsStr) -> Name {
        Name {
            name: PathBuf::from(s),
        }
    }
}

impl From<&'static str> for Name {
    fn from(name: &'static str) -> Self {
        Self::from_static_ref(name.as_ref())
    }
}

struct Input<'h> {
    haystack: &'h [u8],
    start: usize,
}

impl<'h> Input<'h> {
    fn new<H: ?Sized + AsRef<[u8]>>(haystack: &'h H) -> Input<'h> {
        Input {
            haystack: haystack.as_ref(),
            start: 0,
        }
    }
    fn len(&self) -> usize {
        self.haystack.len() - self.start
    }
    fn first(&self) -> Option<u8> {
        self.haystack.first().copied()
    }
}

impl<'h, H: ?Sized + AsRef<[u8]>> From<&'h H> for Input<'h> {
    fn from(haystack: &'h H) -> Input<'h> {
        Input::new(haystack)
    }
}

fn byte_len(b: &[u8]) -> usize {
    b.len()
}

fn code(s: &str) -> Option<u16> {
    let b: &[u8] = s.as_ref();
    if b.len() == 3 { s.parse().ok() } else { None }
}

trait Order {
    fn widen<R: Into<i64>>(x: R) -> i64;
    fn run<F: Fn(i32) -> i32>(f: F) -> i32;
    fn mix<S: Into<String>, R: Default + Debug>(s: S) -> (String, R);
}

struct Asc;

impl Order for Asc {
    fn widen<R: Into<i64>>(x: R) -> i64 {
        x.into() + 1
    }
    fn run<F: Fn(i32) -> i32>(f: F) -> i32 {
        f(10)
    }
    fn mix<S: Into<String>, R: Default + Debug>(s: S) -> (String, R) {
        (s.into(), R::default())
    }
}

fn go<O: Order>() -> String {
    let a = O::widen::<u8>(3);
    let b = O::run::<fn(i32) -> i32>(|x| x * 2);
    let (s, r) = O::mix::<&str, u16>("mixed");
    format!("{a} {b} {s} {r:?}")
}

fn main() {
    let mut b = Builder {
        prefix: OsStr::new(""),
        suffix: OsStr::new(""),
    };
    b.prefix("tmp").suffix(".rs");
    println!("{}", b.len());
    let n = Name::from("café");
    println!("{}", n.name.display());
    let p: &Path = "/a/b".as_ref();
    println!("{}", p.display());

    let v: Vec<u8> = vec![7, 8];
    println!("{} {}", Input::from("héllo").len(), Input::from(&v).len());
    println!("{:?} {:?}", Input::from("ab").first(), Input::new("é").first());
    println!("{}", byte_len("naïve".as_ref()));
    println!("{:?} {:?}", code("404"), code("40"));
    println!("{}", go::<Asc>());
}
