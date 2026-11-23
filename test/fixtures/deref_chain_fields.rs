// Field access through chained `Deref` / `DerefMut` impls: `w.sign` is `w.deref().deref().sign`.
use std::ops::{Deref, DerefMut};
struct Inner { sign: i32, name: String }
struct Mid { i: Inner, tag: u8 }
struct Outer { m: Mid }
impl Deref for Mid { type Target = Inner; fn deref(&self) -> &Inner { &self.i } }
impl DerefMut for Mid { fn deref_mut(&mut self) -> &mut Inner { &mut self.i } }
impl Deref for Outer { type Target = Mid; fn deref(&self) -> &Mid { &self.m } }
impl DerefMut for Outer { fn deref_mut(&mut self) -> &mut Mid { &mut self.m } }
fn show(o: &Outer) -> String { format!("{} {} {}", o.sign, o.tag, o.name) }
fn main() {
    let mut w = Outer { m: Mid { i: Inner { sign: 3, name: "a".into() }, tag: 1 } };
    println!("{}", w.sign);
    w.sign += 4;
    w.tag = 9;
    w.name.push('b');
    println!("{}", show(&w));
    let r = &mut w;
    r.sign = -1;
    println!("{} {}", w.sign, w.m.i.sign);
}
