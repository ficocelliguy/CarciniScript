// A new value's type argument settled by the one impl of a bound that only the inherent `impl`
// declares (anstream's `impl<S: RawStream> AutoStream<S> { fn always(raw: S) -> Self }`), inline
// or in its `where` clause.
trait RawStream { fn put(&mut self, b: u8); fn bytes(&self) -> usize; }
impl RawStream for Vec<u8> {
    fn put(&mut self, b: u8) { self.push(b) }
    fn bytes(&self) -> usize { self.len() }
}
struct Auto<S> { raw: S, n: u32 }
impl<S: RawStream> Auto<S> {
    fn always(raw: S) -> Self { Auto { raw, n: 0 } }
    fn put(&mut self, b: u8) { self.raw.put(b); self.n += 1; }
}
struct Pair<'a, T, U> { a: T, b: &'a U }
impl<'a, T, U> Pair<'a, T, U> where T: RawStream, U: std::fmt::Debug {
    fn new(a: T, b: &'a U) -> Self { Pair { a, b } }
}
fn main() {
    let mut a = Auto::always(Vec::new());
    a.put(3);
    a.put(4);
    println!("{:?} {} {}", a.raw, a.n, a.raw.bytes());
    let x = 5;
    let mut p = Pair::new(Vec::new(), &x);
    p.a.put(9);
    println!("{:?} {:?}", p.a, p.b);
}
