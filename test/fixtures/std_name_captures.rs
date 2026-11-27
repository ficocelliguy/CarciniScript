// Names a program shares with std: a local `OsStr` beside `std::ffi::OsStr`, a user `fn Ok`
// beside the preludes' `Ok(..)`, `impl PartialEq<str>` written before `impl PartialEq`, an
// inherent `as_ref` beside `impl AsRef<str>`, and doc comments naming `io::Write` / `chrono_tz`
// (which load nothing).
use std::path::PathBuf;

/// A doc comment naming crate::io::Write and `use chrono_tz::Tz;` changes nothing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tz {
    Asia__Irkutsk,
    PRC,
}

#[derive(Clone)]
pub struct OsStr {
    name: &'static std::ffi::OsStr,
}

impl OsStr {
    fn from_static_ref(name: &'static std::ffi::OsStr) -> Self {
        Self { name }
    }
    fn to_os_string(&self) -> std::ffi::OsString {
        self.name.to_owned()
    }
}

impl From<&'static str> for OsStr {
    fn from(name: &'static str) -> Self {
        Self::from_static_ref(name.as_ref())
    }
}

#[derive(Debug)]
pub struct Error {
    msg: String,
}
pub type Result<T, E = Error> = core::result::Result<T, E>;

#[allow(non_snake_case)]
pub fn Ok<T>(value: T) -> Result<T> {
    Result::Ok(value)
}

fn half(x: u32) -> Result<u32> {
    if x % 2 == 1 {
        return Err(Error { msg: format!("{x} is odd") });
    }
    Ok(x / 2)
}

fn path_of(s: &str) -> core::result::Result<PathBuf, core::convert::Infallible> {
    s.parse()
}

pub struct T {
    s: String,
}
impl PartialEq<str> for T {
    fn eq(&self, rhs: &str) -> bool {
        rhs.len() == self.s.len()
    }
}
impl PartialEq for T {
    fn eq(&self, other: &T) -> bool {
        self.s == other.s
    }
}

pub struct Name {
    first: String,
    last: String,
}
impl Name {
    pub fn as_ref(&self) -> (&str, &str) {
        (&self.first, &self.last)
    }
}
impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.last
    }
}

fn shout<S: AsRef<str>>(s: &S) -> String {
    s.as_ref().to_uppercase()
}

fn main() {
    let tzs = [Tz::Asia__Irkutsk, Tz::PRC];
    println!("{:?}", tzs);
    let o = OsStr::from("hello");
    println!("{:?}", o.to_os_string());
    println!("{:?} {}", half(8).map_err(|e| e.msg), half(3).unwrap_err().msg);
    println!("{:?}", path_of("a/b").unwrap());
    let (a, b) = (T { s: "ab".to_string() }, T { s: "ab".to_string() });
    let c = T { s: "xy".to_string() };
    println!("{} {} {} {}", a == b, a == c, a == *"zz", a == *"zzz");
    let n = Name { first: "ada".to_string(), last: "lovelace".to_string() };
    let (f, l) = n.as_ref();
    println!("{f} {l} {}", shout(&n));
}
