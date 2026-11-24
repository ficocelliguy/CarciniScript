// Deref coercion of `Cow` and `LazyLock` at arguments and returns, `K: Borrow<Q>` lookups,
// `slice.get(range)` with a range value, `u64::checked_next_power_of_two` as a function value,
// `OsStr::to_owned()` as an `OsString`, `str::is_ascii`, and a file's own `IntoIter` next to
// `impl Write`.
use std::borrow::{Borrow, Cow};
use std::cell::LazyCell;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::hash::Hash;
use std::io::{self, BufRead, Read, Write};
use std::ops::{Range, RangeFrom};
use std::path::PathBuf;
use std::sync::LazyLock;

struct Error {
    message: Cow<'static, str>,
}

fn shout(s: &str) -> String {
    s.to_uppercase()
}

fn total(b: &[u8]) -> u32 {
    b.iter().map(|&x| x as u32).sum()
}

struct Session {
    key: Cow<'static, str>,
    data: Cow<'static, [u8]>,
}

impl Session {
    fn id(&self) -> &str {
        &self.key
    }
    fn bytes(&self) -> &[u8] {
        &self.data
    }
}

fn suffix(f: &str) -> Cow<'_, str> {
    if f.is_ascii() {
        Cow::Borrowed(f)
    } else {
        Cow::Owned(f.replace('é', "e"))
    }
}

struct Context {
    n: u32,
}

static CONTEXT: LazyLock<Context> = LazyLock::new(|| Context { n: 4 });
static NAMES: LazyLock<HashMap<String, u32>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("a".to_string(), 1);
    m
});

fn shared() -> &'static Context {
    &CONTEXT
}

fn count(m: &HashMap<String, u32>) -> usize {
    m.len()
}

fn first_n(c: &Context) -> u32 {
    c.n
}

struct Cache<K, V> {
    map: HashMap<K, V>,
}

impl<K: Hash + Eq, V> Cache<K, V> {
    fn get<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
    {
        self.map.get(k)
    }

    fn has<Q: Hash + Eq + ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        self.map.contains_key(k)
    }

    fn take<Q>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.map.remove(k)
    }
}

struct Count {
    lo: usize,
    hi: Option<usize>,
}

impl From<RangeFrom<usize>> for Count {
    fn from(r: RangeFrom<usize>) -> Count {
        Count { lo: r.start, hi: None }
    }
}

impl From<Range<usize>> for Count {
    fn from(r: Range<usize>) -> Count {
        Count { lo: r.start, hi: Some(r.end) }
    }
}

fn repeat(c: impl Into<Count>) -> String {
    let c = c.into();
    format!("{}..{:?}", c.lo, c.hi)
}

fn grow(cap: u64) -> Option<u64> {
    cap.checked_mul(3).and_then(u64::checked_next_power_of_two)
}

fn cc_env(wrapper: &PathBuf, args: &[OsString]) -> OsString {
    let mut env = wrapper.as_os_str().to_owned();
    for a in args {
        env.push(" ");
        env.push(a);
    }
    let mut tail = OsStr::new("--").to_owned();
    tail.push("end");
    env.push(" ");
    env.push(tail);
    env
}

struct Bytes {
    v: Vec<u8>,
}

struct IntoIter {
    v: Vec<u8>,
}

impl Iterator for IntoIter {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        self.v.pop()
    }
}

impl IntoIterator for Bytes {
    type Item = u8;
    type IntoIter = IntoIter;
    fn into_iter(self) -> IntoIter {
        IntoIter { v: self.v }
    }
}

impl Write for Bytes {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.v.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn main() {
    // `Cow` deref coercion.
    let e = Error { message: Cow::Borrowed("bad") };
    println!("{}", shout(&e.message));
    let mut s = String::from("x");
    s.push_str(&suffix("café"));
    s.push_str(&suffix("ok"));
    println!("{s}");
    let sess = Session { key: Cow::Owned("k1".to_string()), data: Cow::Borrowed(b"ab") };
    println!("{} {} {}", sess.id(), sess.bytes().len(), total(&sess.data));

    // `LazyLock` / `LazyCell` deref coercion.
    println!("{} {}", shared().n, first_n(&CONTEXT));
    let names = NAMES.clone();
    println!("{}", count(&names));
    let cell: LazyCell<Context> = LazyCell::new(|| Context { n: 9 });
    println!("{}", first_n(&cell));

    // `K: Borrow<Q>`.
    let mut c = Cache { map: HashMap::new() };
    c.map.insert("one".to_string(), 1);
    c.map.insert("two".to_string(), 2);
    println!("{:?} {:?} {}", c.get("one"), c.get("three"), c.has("two"));
    println!("{:?} {}", c.take("two"), c.has("two"));

    // `slice.get(range)` with a range value, and ranges into an `impl Into<Count>`.
    let v = [10, 20, 30, 40];
    let r = 1..3;
    let from: RangeFrom<usize> = 2..;
    println!("{:?} {:?} {:?}", v.get(r.clone()), v.get(from), v.get(3..9));
    println!("{} {}", repeat(0..), repeat(1..3));

    // `u64::checked_next_power_of_two` as a function value.
    println!("{:?} {:?}", grow(5), grow(u64::MAX / 2));

    // `OsStr::to_owned()` is an `OsString`.
    let env = cc_env(&PathBuf::from("/bin/cc"), &[OsString::from("-O2")]);
    println!("{:?}", env);

    // `is_ascii` on strings and bytes.
    println!("{} {} {}", "abc".is_ascii(), "café".is_ascii(), b"ab\x80".is_ascii());

    // A file's own `IntoIter` with the io prelude's `bytes()` / `lines()`.
    let mut b = Bytes { v: vec![] };
    b.write_all(b"hi").unwrap();
    let back: Vec<u8> = b.into_iter().collect();
    println!("{:?}", back);
    let n: usize = io::Cursor::new(b"a\nbc").lines().map(|l| l.unwrap().len()).sum();
    let m = io::Cursor::new(b"xyz").bytes().count();
    println!("{n} {m}");
}
