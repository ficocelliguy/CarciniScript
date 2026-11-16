// A newtype with `From<&str>` and `From<String>` (both, as std types often have) and
// `AsRef<str>`: `Host::from(..)`, `.into()` from either, and `as_ref()`.
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
struct Host(String);

impl From<&str> for Host {
    fn from(s: &str) -> Self {
        Host(s.to_owned())
    }
}

impl From<String> for Host {
    fn from(s: String) -> Self {
        Host(s)
    }
}

impl AsRef<str> for Host {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.0)
    }
}

fn longest(items: &[Host]) -> usize {
    items.iter().map(|s| s.as_ref().len()).max().unwrap_or(0)
}

fn main() {
    let a = Host::from("home");
    let b: Host = String::from("n00dles").into();
    let c: Host = "joes".into();
    let owned = format!("{}-{}", a.as_ref(), 2);
    let d = Host::from(owned);
    println!("{a} {b} {c} {d} {}", a == Host("home".into()));
    let hosts = vec![a, b, c, d];
    println!("{}", longest(&hosts));
}
