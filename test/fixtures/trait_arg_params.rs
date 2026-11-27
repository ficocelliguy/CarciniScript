// Impl type parameters only the trait's arguments name (`E` of `impl<E: Default> IntoWeighted<E> for
// (u32, u32)`, petgraph's shape): `E::default()` in the impl's methods, with `E`'s class from a
// direct call's expected type or handed on through a `where I::Item: IntoWeighted<E>` bound.
trait IntoWeighted<E> { fn into_weighted(self) -> (u32, u32, E); }
impl<E: Default> IntoWeighted<E> for (u32, u32) {
    fn into_weighted(self) -> (u32, u32, E) { (self.0, self.1, E::default()) }
}
impl<E> IntoWeighted<E> for (u32, u32, E) {
    fn into_weighted(self) -> (u32, u32, E) { self }
}
fn edges<E, I: IntoIterator>(it: I) -> Vec<(u32, u32, E)> where I::Item: IntoWeighted<E> {
    it.into_iter().map(|x| x.into_weighted()).collect()
}
fn main_a() {
    let a: (u32, u32, f64) = (1, 2).into_weighted();
    let b: (u32, u32, String) = (3, 4).into_weighted();
    println!("{:?} {:?}", a, b);
    let v: Vec<(u32, u32, i32)> = edges(vec![(1, 2), (2, 3)]);
    println!("{:?}", v);
    let w: Vec<(u32, u32, String)> = edges(vec![(1, 2)]);
    println!("{:?}", w);
    let x: Vec<(u32, u32, i32)> = edges(vec![(1, 2, 7)]);
    println!("{:?}", x);
}
trait Make<A, B> { fn make(&self) -> (A, B); }
struct S(i32);
impl<B: Default + std::fmt::Debug> Make<i32, B> for S {
    fn make(&self) -> (i32, B) { (self.0, B::default()) }
}
impl<B: Default> Make<i32, B> for u8 {
    fn make(&self) -> (i32, B) { (*self as i32 * 10, B::default()) }
}
fn via<T: Make<i32, B>, B>(t: &T) -> (i32, B) { t.make() }
trait Zero<E> { fn zero_with(&self, n: usize) -> Vec<E>; }
impl<E: Default + Clone> Zero<E> for S {
    fn zero_with(&self, n: usize) -> Vec<E> { vec![E::default(); n] }
}
fn main_b() {
    let s = S(4);
    let a: (i32, String) = s.make();
    let b: (i32, Vec<u8>) = via(&s);
    let c: (i32, bool) = via(&3u8);
    println!("{:?} {:?} {:?}", a, b, c);
    let z: Vec<f64> = s.zero_with(2);
    println!("{:?}", z);
}
fn main() {
    main_a();
    main_b();
}
