// std trait methods named by path: `Deref::deref(&w)`, `DerefMut::deref_mut(&mut w)`, and
// `<W as Clone>::clone(&w)` / `<W as ToString>::to_string(&w)` / `<W as Deref>::deref(&w)`.
use std::ops::{Deref, DerefMut};

#[derive(Clone, Debug)]
struct W(Vec<u8>);
impl Deref for W {
    type Target = Vec<u8>;
    fn deref(&self) -> &Vec<u8> {
        &self.0
    }
}
impl DerefMut for W {
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        &mut self.0
    }
}
impl std::fmt::Display for W {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "w{}", self.0.len())
    }
}

fn main() {
    let mut w = W(vec![1, 2]);
    println!("{}", Deref::deref(&w).len());
    DerefMut::deref_mut(&mut w).push(3);
    println!("{:?}", <W as Deref>::deref(&w));
    println!("{:?}", <W as Clone>::clone(&w));
    println!("{}", <W as ToString>::to_string(&w));
    println!("{}", std::ops::Deref::deref(&w)[0]);
    let s = String::from("hi");
    let t: &str = Deref::deref(&s);
    println!("{}", t);
    let b = Box::new(5);
    println!("{}", *Deref::deref(&b));
}
