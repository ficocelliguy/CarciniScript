// Inherent impls for instantiations that are one type in JS because a reference is erased:
// `W<&T>` next to `W<Vec<T>>`, `Split<&T>` next to `Split<Ptr<T>>` (as in bitvec's
// `Split`), each with a method of one name. A receiver whose argument is a `Vec` or the struct
// takes that impl's, any other the reference's.
pub struct W<T>(pub T);
impl<'a, T> W<&'a T> {
    pub fn n(&self) -> usize { 1 }
}
impl<T> W<Vec<T>> {
    pub fn n(&self) -> usize { self.0.len() + 10 }
}

pub struct Ptr<'a, T: ?Sized>(&'a T);
#[allow(dead_code)]
pub struct Split<T> { source: T, mid: usize }
impl<'a, T: ?Sized> Split<&'a T> {
    pub fn via_immutable(self) -> usize { self.mid }
}
impl<'a, T: ?Sized> Split<&'a mut T> {
    pub fn via_mutable(self) -> usize { self.mid * 2 }
}
impl<'a, T: ?Sized> Split<Ptr<'a, T>> {
    pub fn via_immutable(self) -> usize { self.mid + 100 }
}

fn main() {
    let x = 5u8;
    let a = W(&x);
    let b = W(vec![1, 2, 3]);
    let s = String::from("hi");
    let c = W(&s);
    println!("{} {} {}", a.n(), b.n(), c.n());
    let v = [1u8, 2];
    let s1 = Split { source: &v[..], mid: 1 };
    let s2 = Split { source: Ptr(&v[..]), mid: 1 };
    let mut m = [0u8; 2];
    let s3 = Split { source: &mut m[..], mid: 3 };
    println!("{} {} {}", s1.via_immutable(), s2.via_immutable(), s3.via_mutable());
}
