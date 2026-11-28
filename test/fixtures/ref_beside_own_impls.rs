// `impl Tr for A` next to `impl Tr for &A` that behave differently (one value in JS): each call
// takes the impl rustc's method probe finds first for the receiver as written (`a`, `&a`, a
// local `r = &a`) and the method's `self`, `&self` or `&mut self`; a reference passed for a
// type parameter carries which one it is.
pub trait Tr {
    fn go(&mut self) -> u32;
    fn peek(&self) -> u32;
    fn take(self) -> u32;
}
pub struct A(u32);
impl Tr for A {
    fn go(&mut self) -> u32 { self.0 }
    fn peek(&self) -> u32 { self.0 * 10 }
    fn take(self) -> u32 { self.0 * 100 }
}
impl Tr for &A {
    fn go(&mut self) -> u32 { self.0 + 1 }
    fn peek(&self) -> u32 { self.0 * 10 + 1 }
    fn take(self) -> u32 { self.0 * 100 + 1 }
}
fn g<T: Tr>(t: T) -> u32 { t.take() }
fn h<T: Tr>(t: &T) -> u32 { t.peek() }
fn main() {
    println!("{} {} {}", g(A(5)), g(&A(6)), h(&A(7)));
    let mut a = A(3);
    println!("{} {}", a.go(), a.peek());
    let mut r = &a;
    println!("{} {} {}", r.go(), r.peek(), r.take());
    println!("{} {}", (&a).take(), (&a).peek());
    let b = A(4);
    println!("{}", b.take());
}
