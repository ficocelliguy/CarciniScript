// `.as_ref()` on a parameter bounded `AsRef<[u8]>` / `AsRef<str>` / `AsRef<Path>`, and a string
// passed as an `AsRef<[u8]>` (its bytes).
use std::path::{Path, PathBuf};
fn count<B: AsRef<[u8]>>(b: B) -> usize { let s: &[u8] = b.as_ref(); s.len() }
fn first(b: impl AsRef<[u8]>) -> u8 { b.as_ref()[0] }
fn slen<S: AsRef<str>>(s: S) -> usize { let t: &str = s.as_ref(); t.len() }
fn plen<P: AsRef<Path>>(p: P) -> usize { let q: &Path = p.as_ref(); q.to_str().unwrap().len() }
fn takes(s: &[u8]) -> usize { s.len() }
fn takes_str(s: &str) -> String { s.to_uppercase() }
fn hs<H>(h: H) -> usize where H: AsRef<[u8]> { takes(h.as_ref()) }
fn hs2<S: AsRef<str>>(h: S) -> String { takes_str(h.as_ref()) }
fn main() {
    println!("{} {} {}", count(vec![1u8, 2]), count([1u8, 2, 3]), count("abcd"));
    println!("{} {} {}", first(vec![7u8]), first("A"), slen(String::from("hi")));
    println!("{} {} {}", slen("abc"), plen("a/b"), plen(PathBuf::from("xy")));
    println!("{} {}", hs(vec![1u8]), hs2("abcd"));
    let v: Vec<u8> = String::from("yo").into();
    println!("{:?}", v);
}
