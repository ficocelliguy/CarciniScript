// `Cow<str>` against a `&str` / `String`: `let t: &str = &c;` by deref coercion (also for
// your own `Deref` types), and `c == "x"`, `"x" == c`, `c == s`, `s == c`.
use std::borrow::Cow;
use std::ops::Deref;
struct W(String);
impl Deref for W { type Target = str; fn deref(&self) -> &str { &self.0 } }
fn norm(s: &str) -> Cow<str> {
    if s.contains(' ') { Cow::Owned(s.replace(' ', "_")) } else { Cow::Borrowed(s) }
}
fn main() {
    let a = norm("a b");
    let b = norm("ab");
    let s = String::from("a_b");
    println!("{} {} {} {}", a == s, s == a, a != "x", "ab" == b);
    println!("{} {}", a == b, a == norm("a b"));
    let v: Cow<[u8]> = Cow::Borrowed(&[1, 2]);
    let w: Cow<[u8]> = Cow::Owned(vec![1, 2]);
    println!("{}", v == w);
    let t: &str = &a;
    println!("{} {}", t.len(), a.len());
    if b == "ab" { println!("yes"); }
    let w = W("hi".into());
    let wt: &str = &w;
    let c: Cow<str> = Cow::Borrowed("x");
    let ct: &str = &c;
    println!("{} {} {}", wt, ct, "x" == c);
}
