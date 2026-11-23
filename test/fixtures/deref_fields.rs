// Fields of a `Deref` target read through the wrapper (`w.sign`), and written through
// `DerefMut` (`m.len += 2`).
use std::ops::{Deref, DerefMut};
#[derive(Debug)]
struct Span { sign: i8, len: u32, name: String }
struct Wrap(Span);
impl Deref for Wrap { type Target = Span; fn deref(&self) -> &Span { &self.0 } }
struct M { s: Span }
impl Deref for M { type Target = Span; fn deref(&self) -> &Span { &self.s } }
impl DerefMut for M { fn deref_mut(&mut self) -> &mut Span { &mut self.s } }
fn sign_of(w: &Wrap) -> i8 { w.sign }
fn main() {
    let w = Wrap(Span { sign: -1, len: 3, name: "a".into() });
    println!("{} {} {} {}", w.sign, w.len, w.name.len(), sign_of(&w));
    let mut m = M { s: Span { sign: 1, len: 0, name: String::new() } };
    m.len += 2;
    m.name.push_str("hi");
    m.sign = -5;
    println!("{:?}", m.s);
}
