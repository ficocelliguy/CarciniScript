// Blanket impls of one trait for `&T` and `&mut T` (one value in JS), as bitvec's `AddressExt`:
// each call takes the impl for how its receiver is written (`&x`, `&mut x`, a local declared or
// initialized as either).
pub trait AddressExt {
    type Referent;
    fn addr(self) -> usize;
    fn kind(self) -> &'static str;
}
impl<T> AddressExt for &T {
    type Referent = T;
    fn addr(self) -> usize { 1 }
    fn kind(self) -> &'static str { "shared" }
}
impl<T> AddressExt for &mut T {
    type Referent = T;
    fn addr(self) -> usize { 2 }
    fn kind(self) -> &'static str { "unique" }
}
fn main() {
    let mut x = 5u8;
    let s = String::from("a");
    println!("{} {}", (&x).addr(), (&mut x).addr());
    let r = &s;
    let w: &mut u8 = &mut x;
    println!("{} {}", r.kind(), w.kind());
    let v = vec![1, 2];
    println!("{} {}", (&v).kind(), (&s).addr());
}
