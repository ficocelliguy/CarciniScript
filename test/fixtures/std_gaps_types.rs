// Std types filled in (PLAN "Std-library gaps"): `PhantomData`, `std::path::{Path, PathBuf}`,
// `OsStr` / `OsString`, `cell::Ref` / `RefMut` named as types, `io::Cursor`, `NonZero*`,
// `Infallible` and `fmt::Arguments`. (Paths here avoid `join` / `push`, whose separator is the
// host's natively: `\` on Windows.)
use std::cell::{Ref, RefCell, RefMut};
use std::convert::Infallible;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{BufRead, Cursor, Read};
use std::marker::PhantomData;
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Meters;

#[derive(Clone, PartialEq, Default)]
struct Length<U> {
    value: f64,
    unit: PhantomData<U>,
}

impl<U> Length<U> {
    fn new(value: f64) -> Self {
        Length {
            value,
            unit: PhantomData,
        }
    }
}

struct Id<T> {
    raw: u32,
    _kind: std::marker::PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    fn new(raw: u32) -> Id<T> {
        Id {
            raw,
            _kind: PhantomData::<fn() -> T>,
        }
    }
}

fn describe(p: &Path) -> String {
    format!("{} ({:?})", p.display(), p.extension())
}

fn ext_of(p: &Path) -> Option<&str> {
    p.extension().and_then(|e| e.to_str())
}

struct Store {
    items: RefCell<Vec<i32>>,
}

impl Store {
    fn get(&self) -> Ref<'_, Vec<i32>> {
        self.items.borrow()
    }
    fn get_mut(&self) -> RefMut<'_, Vec<i32>> {
        self.items.borrow_mut()
    }
}

struct Name(String);

impl FromStr for Name {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Infallible> {
        Ok(Name(s.to_string()))
    }
}

fn first_nonzero(v: &[usize]) -> Option<NonZeroUsize> {
    v.iter().find_map(|x| NonZeroUsize::new(*x))
}

fn log(out: &mut Vec<String>, args: fmt::Arguments<'_>) {
    out.push(format!("[{}]", args));
}

fn main() {
    // PhantomData
    let a: Length<Meters> = Length::new(2.5);
    let b = Length::<Meters> {
        value: 2.5,
        unit: Default::default(),
    };
    println!("{} {}", a.value, a == b.clone());
    let c: Length<Meters> = Length::default();
    println!("{}", c.value);
    let id: Id<String> = Id::new(4);
    println!("{}", id.raw);
    let p: PhantomData<i32> = PhantomData;
    println!("{}", p == PhantomData);

    // Path / PathBuf
    let file = Path::new("/home/user/scripts/hack.js");
    println!("{}", file.display());
    println!("{:?} {:?} {:?}", file.file_name(), file.extension(), file.file_stem());
    println!("{:?}", file.parent());
    println!("{:?} {}", file.to_str(), file.exists());
    let rel = PathBuf::from("a/b/c.rs");
    println!("{:?}", rel);
    println!("{}", describe(&rel));
    println!("{:?}", ext_of(&rel));
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    println!("{:?}", parts);
    println!("{} {} {}", rel.starts_with("a"), rel.starts_with("a/b"), rel.starts_with("b"));
    println!("{} {}", rel.ends_with("c.rs"), rel.ends_with("b/c.rs"));
    println!("{}", rel.with_extension("js").display());
    println!("{:?}", rel.strip_prefix("a").map(|p| p.to_path_buf()));
    let mut buf = rel.to_path_buf();
    println!("{} {:?}", buf.pop(), buf);
    buf.set_extension("txt");
    println!("{:?}", buf);
    println!("{:?} {:?}", Path::new("noext").extension(), Path::new(".bashrc").file_stem());
    println!("{:?} {:?}", Path::new("a").parent(), Path::new("dir/").file_name());
    println!("{}", rel == PathBuf::from(String::from("a/b/c.rs")));
    let parsed: PathBuf = "x/y.toml".parse().unwrap();
    println!("{:?}", parsed.file_stem());

    // OsStr / OsString
    let os = OsString::from("hello");
    println!("{:?} {}", os, os.to_string_lossy());
    let e: &OsStr = rel.extension().unwrap();
    println!("{:?} {:?}", e, e.to_str());
    println!("{:?}", OsStr::new("x.y").to_str());
    println!("{:?}", os.clone().into_string());

    // Ref / RefMut
    let s = Store {
        items: RefCell::new(vec![1]),
    };
    s.get_mut().push(2);
    {
        let r: Ref<Vec<i32>> = s.get();
        println!("{:?} {}", *r, r.len());
    }
    {
        let mut m: RefMut<Vec<i32>> = s.get_mut();
        m.push(3);
    }
    let first = Ref::map(s.get(), |v| &v[0]);
    println!("{} {:?}", *first, s.get());

    // io::Cursor
    let text = Cursor::new("line one\nline two\r\nthree");
    for line in text.lines() {
        println!("{:?}", line.unwrap());
    }
    let mut bytes = Cursor::new(b"abc\ndef".to_vec());
    let mut first_line = String::new();
    let n = bytes.read_line(&mut first_line).unwrap();
    println!("{} {:?} {}", n, first_line, bytes.position());
    let mut rest = Vec::new();
    bytes.read_to_end(&mut rest).unwrap();
    println!("{:?}", rest);
    let mut two = [0u8; 2];
    let mut c3 = Cursor::new(vec![9u8, 8, 7]);
    c3.read_exact(&mut two).unwrap();
    println!("{:?}", two);
    println!("{:?}", c3.read_exact(&mut two).is_err());

    // NonZero*
    let nz = NonZeroUsize::new(5).unwrap();
    println!("{} {:?} {:?}", nz.get(), NonZeroUsize::new(0), first_nonzero(&[0, 0, 3]));
    let all: Vec<Option<NonZeroUsize>> = vec![0, 2].into_iter().map(NonZeroUsize::new).collect();
    println!("{:?}", all);
    let nz2: NonZeroU32 = NonZeroU32::new(7).expect("nonzero");
    println!("{} {}", nz2, nz2.get() + 1);

    // Infallible
    let n: Name = "bob".parse().unwrap();
    println!("{}", n.0);
    let r: Result<i32, Infallible> = Ok(3);
    println!("{:?}", r);

    // fmt::Arguments
    let mut logs = Vec::new();
    log(&mut logs, format_args!("{} + {} = {}", 1, 2, 1 + 2));
    log(&mut logs, format_args!("{:>4}", "x"));
    println!("{:?}", logs);
    println!("{}", format_args!("{:03}", 7));
    println!("{}", fmt::format(format_args!("{}-{}", "a", "b")));
}
