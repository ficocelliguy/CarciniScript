// `impl<S: Into<X>> From<S> for Y`: `Y::from(..)` / `.into()` take anything that converts into
// an `X`, and `o.into()` in the impl uses the impl's bound.
#[derive(Debug)]
enum Pred { Equals(String) }
impl<S: Into<String>> From<S> for Pred {
    fn from(o: S) -> Self { Pred::Equals(o.into()) }
}
#[derive(Debug)]
struct Big(i64);
impl<N> From<N> for Big where N: Into<i64> {
    fn from(n: N) -> Self { Big(n.into() * 2) }
}
fn main() {
    let y: Pred = "a".into();
    let z = Pred::from(String::from("b"));
    println!("{:?} {:?}", y, z);
    let b: Big = 4i32.into();
    println!("{:?} {:?}", b, Big::from(7u8));
}
