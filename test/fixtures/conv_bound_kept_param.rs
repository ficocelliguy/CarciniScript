// A function's `B: Into<Vec<u8>>` / `T: AsRef<str>` / `T: AsRef<[u8]>` that the signature names
// beyond a plain argument (`-> Wrap<B>`, `&[T]`, `-> &T`) stays the caller's own type, and
// `as_ref()` / `into()` convert it where they're called.
use std::borrow::Borrow;
use std::ffi::OsStr;

#[derive(Debug)]
struct Wrap<O> {
    owner: O,
}

fn from_bytes<B>(bytes: B) -> Result<Vec<u8>, Wrap<B>>
where
    B: Into<Vec<u8>> + AsRef<[u8]>,
{
    if bytes.as_ref().is_empty() {
        Err(Wrap { owner: bytes })
    } else {
        Ok(bytes.into())
    }
}

#[derive(Clone, Debug)]
enum Flag {
    Always(String),
    IfSupported(String),
}
impl AsRef<OsStr> for Flag {
    fn as_ref(&self) -> &OsStr {
        match self {
            Flag::Always(s) | Flag::IfSupported(s) => OsStr::new(s),
        }
    }
}

fn split<T: AsRef<OsStr>>(flags: &[T]) -> (&[T], &[T]) {
    let start = flags.iter().position(|f| f.as_ref() == OsStr::new("/link"));
    flags.split_at(start.unwrap_or(flags.len()))
}

struct Name(String);
impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

fn longest<T: AsRef<str>>(xs: &[T]) -> &T {
    let mut best = &xs[0];
    for x in xs {
        if x.as_ref().len() > best.as_ref().len() {
            best = x;
        }
    }
    best
}

fn own<T: Into<String>>(x: T) -> T {
    x
}

fn keys<K: Borrow<str>>(ks: Vec<K>) -> Vec<K> {
    let mut ks = ks;
    ks.retain(|k| k.borrow().starts_with('a'));
    ks
}

fn main() {
    println!("{:?}", from_bytes(String::from("hi")));
    println!("{:?}", from_bytes(vec![1u8, 2]));
    match from_bytes(String::new()) {
        Err(w) => println!("empty string kept: {:?}", w.owner),
        Ok(_) => {}
    }
    match from_bytes(Vec::<u8>::new()) {
        Err(w) => println!("empty vec kept: {:?}", w.owner),
        Ok(_) => {}
    }

    let flags = vec![
        Flag::Always("-O".into()),
        Flag::IfSupported("/link".into()),
        Flag::Always("x".into()),
    ];
    let (a, b) = split(&flags);
    let mut n = 0;
    for f in a {
        match f {
            Flag::Always(_) => n += 1,
            Flag::IfSupported(_) => n += 2,
        }
    }
    println!("{} {} {:?}", n, b.len(), b[0]);
    let strs = ["a", "/link", "b", "c"];
    println!("{:?}", split(&strs));

    let names = vec![Name("ab".into()), Name("abcd".into()), Name("c".into())];
    println!("{}", longest(&names).0);
    println!("{}", longest(&["xy", "xyz", "z"]));
    println!("{}", longest(&[String::from("q"), String::from("qq")]));

    let s: &'static str = own("lit");
    let t: String = own(String::from("owned"));
    println!("{s} {t}");
    println!("{:?}", keys(vec!["apple", "pear", "avocado"]));
    println!("{:?}", keys(vec![String::from("b"), String::from("ab")]));
}
