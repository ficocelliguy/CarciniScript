// `Default::default()` where a type parameter is expected, with the `Default` bound on the
// `impl` or the function (as `T::default()`).
#[derive(Debug)]
struct Cell<T> { v: T }
impl<T> Cell<T> {
    fn swap(&mut self, n: T) -> T { std::mem::replace(&mut self.v, n) }
}
impl<T: Default> Cell<T> {
    fn take(&mut self) -> T { self.swap(Default::default()) }
    fn reset(&mut self) { let d: T = Default::default(); self.v = d; }
}
fn take2<T: Default>(c: &mut Cell<T>) -> T { c.swap(Default::default()) }
fn main() {
    let mut c = Cell { v: 5u8 };
    println!("{} {}", c.take(), c.v);
    c.v = 7;
    println!("{} {}", take2(&mut c), c.v);
    let mut s = Cell { v: String::from("x") };
    println!("{:?} {:?}", s.take(), s);
    let mut w = Cell { v: vec![1, 2] };
    w.reset();
    println!("{:?}", w);
}
