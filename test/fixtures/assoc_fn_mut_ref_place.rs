// An associated fn (no `self`) returning `&mut T` / `Option<&mut T>`, called by path and used
// as a place: `Bx::get(&mut b).push(3)`, `*Cx::num(&mut c) += 1`, and bound with `let`.
struct Bx<T> { v: Box<T> }
impl<T> Bx<T> {
    fn get(this: &mut Self) -> &mut T { &mut this.v }
    fn first(this: &mut Self) -> Option<&mut T> { Some(&mut this.v) }
}
struct Cx { v: Vec<u8>, n: u8 }
impl Cx {
    fn num(this: &mut Self) -> &mut u8 { &mut this.n }
    fn opt(this: &mut Self) -> Option<&mut u8> { Some(&mut this.n) }
}
fn main() {
    let mut b = Bx { v: Box::new(vec![1]) };
    let r = Bx::get(&mut b);
    r.push(4);
    println!("{:?}", b.v);
    let mut n = Bx { v: Box::new(5) };
    let q = Bx::get(&mut n);
    *q += 10;
    *Bx::get(&mut n) += 1;
    if let Some(x) = Bx::first(&mut n) { *x *= 2; }
    println!("{}", n.v);
    let mut c = Cx { v: vec![], n: 1 };
    let k = Cx::num(&mut c);
    *k += 5;
    if let Some(x) = Cx::opt(&mut c) { *x += 100; }
    *Cx::opt(&mut c).unwrap() += 1;
    println!("{:?} {}", c.v, c.n);
    let mut s = Bx { v: Box::new(String::from("a")) };
    Bx::get(&mut s).push('b');
    *Bx::get(&mut s) += "c";
    println!("{}", s.v);
}
