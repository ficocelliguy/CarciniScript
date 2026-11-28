// A std conversion trait's method by path on your type: `AsRef::as_ref(&n)`,
// `<Name as AsRef<str>>::as_ref(&n)`, `Borrow::<str>::borrow(&w)`, `AsMut` / `BorrowMut`, and
// `AsRef::as_ref` as a value; `Borrow::borrow(&cell)` of a `RefCell` is the cell.
use std::borrow::{Borrow, BorrowMut};
struct W(String);
impl Borrow<str> for W { fn borrow(&self) -> &str { &self.0 } }
struct Name(String);
impl AsRef<str> for Name { fn as_ref(&self) -> &str { &self.0 } }
use std::convert::AsMut;
struct Buf(Vec<u8>);
impl AsMut<Vec<u8>> for Buf { fn as_mut(&mut self) -> &mut Vec<u8> { &mut self.0 } }
impl AsRef<[u8]> for Buf { fn as_ref(&self) -> &[u8] { &self.0 } }
impl Borrow<[u8]> for Buf { fn borrow(&self) -> &[u8] { &self.0 } }
impl BorrowMut<[u8]> for Buf { fn borrow_mut(&mut self) -> &mut [u8] { &mut self.0 } }
fn total<T: AsRef<str>>(items: &[T]) -> usize {
    items.iter().map(|t| AsRef::<str>::as_ref(t).len()).sum()
}
fn first<B: Borrow<[u8]>>(b: &B) -> u8 { Borrow::<[u8]>::borrow(b)[0] }
fn main() {
    let mut b = Buf(vec![1, 2]);
    AsMut::<Vec<u8>>::as_mut(&mut b).push(3);
    std::convert::AsMut::as_mut(&mut b).push(4);
    let r: &[u8] = std::convert::AsRef::as_ref(&b);
    println!("{:?} {}", r, r.len());
    let s = String::from("abc");
    let t: &str = AsRef::as_ref(&s);
    let u = <String as AsRef<str>>::as_ref(&s);
    println!("{} {} {}", t, u, total(&["ab", "cde"]));
    let v: Vec<&[u8]> = vec![&b].into_iter().map(AsRef::as_ref).collect();
    println!("{:?} {}", v, first(&b));
    BorrowMut::<[u8]>::borrow_mut(&mut b)[0] = 9;
    println!("{:?}", b.0);
    let w = W("w".to_string());
    let a: &str = Borrow::<str>::borrow(&w);
    let n = Name("n".to_string());
    let nb: &str = AsRef::as_ref(&n);
    let nc = <Name as AsRef<str>>::as_ref(&n);
    let nd: &str = AsRef::<str>::as_ref(&n);
    println!("{} {} {} {}", a, nb, nc, nd);
    // The blanket `impl<T> Borrow<T> for T` on a cell, not `RefCell::borrow`.
    let cell = std::cell::RefCell::new(5);
    let rc: &std::cell::RefCell<i32> = Borrow::borrow(&cell);
    *rc.borrow_mut() += 1;
    let mut c2 = std::cell::RefCell::new(1);
    let m: &mut std::cell::RefCell<i32> = BorrowMut::borrow_mut(&mut c2);
    m.replace(11);
    println!("{} {}", cell.borrow(), c2.borrow());
}
