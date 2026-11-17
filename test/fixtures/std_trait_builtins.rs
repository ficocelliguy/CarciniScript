// Std traits on built-in types with one of your types as the argument: `impl Mul<V2> for f64` /
// `for i32` (`2.0 * v`, also with a literal), `impl Add<Money> for u64`, `impl PartialEq<Host>
// for &str` / `for String` (`"home" == h`), and `impl PartialEq<u32> for Id` (`id == 3`, `!=`).
use std::ops::{Add, Mul};
#[derive(Debug, Clone, Copy, PartialEq)]
struct V2(f64, f64);
impl Mul<V2> for f64 {
    type Output = V2;
    fn mul(self, v: V2) -> V2 {
        V2(self * v.0, self * v.1)
    }
}
impl Mul<V2> for i32 {
    type Output = V2;
    fn mul(self, v: V2) -> V2 {
        V2(self as f64 * v.0, self as f64 * v.1)
    }
}
#[derive(Debug, Clone, Copy)]
struct Money(u64);
impl Add<Money> for u64 {
    type Output = Money;
    fn add(self, m: Money) -> Money {
        Money(self + m.0)
    }
}
#[derive(Debug)]
struct Host(String);
impl PartialEq<Host> for &str {
    fn eq(&self, h: &Host) -> bool {
        *self == h.0
    }
}
impl PartialEq<Host> for String {
    fn eq(&self, h: &Host) -> bool {
        *self == h.0
    }
}
#[derive(Debug, Clone, Copy)]
struct Id(u32);
impl PartialEq<u32> for Id {
    fn eq(&self, n: &u32) -> bool {
        self.0 == *n
    }
}
// Your trait on `Box<str>` / `Rc<[u8]>`: a box is its contents in JS.
trait Shout {
    fn shout(&self) -> String;
}
impl Shout for Box<str> {
    fn shout(&self) -> String {
        self.to_uppercase()
    }
}
impl Shout for std::rc::Rc<[u8]> {
    fn shout(&self) -> String {
        format!("{} bytes", self.len())
    }
}
fn main() {
    let b: Box<str> = "hi".into();
    let r: std::rc::Rc<[u8]> = vec![1u8, 2, 3].into();
    println!("{} {}", b.shout(), r.shout());
    let v = V2(1.0, 2.0);
    let x: f64 = 3.0;
    println!("{:?} {:?} {:?}", 2.0 * v, x * v, 4 * v);
    println!("{:?}", 10u64 + Money(5));
    let h = Host("home".into());
    println!("{} {} {}", "home" == h, "away" == h, String::from("home") == h);
    let id = Id(3);
    println!("{} {} {}", id == 3, id != 3, "x" != h);
}
