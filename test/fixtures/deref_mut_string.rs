// A string or number reached through `DerefMut`: `deref_mut` returns a slot over it, so
// `n.push('c')` and `*c += 4` write the field.
use std::ops::{Deref, DerefMut};
struct Name(String);
impl Deref for Name { type Target = String; fn deref(&self) -> &String { &self.0 } }
impl DerefMut for Name { fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 } }
struct Count { n: u32 }
impl Deref for Count { type Target = u32; fn deref(&self) -> &u32 { &self.n } }
impl DerefMut for Count { fn deref_mut(&mut self) -> &mut u32 { &mut self.n } }
fn main() {
    let mut n = Name(String::from("ab"));
    n.push('c');
    n.push_str("de");
    println!("{} {} {}", *n, n.len(), n.to_uppercase());
    *n = String::from("x");
    println!("{}", n.0);
    let mut c = Count { n: 1 };
    *c += 4;
    println!("{} {}", *c, c.n);
}
