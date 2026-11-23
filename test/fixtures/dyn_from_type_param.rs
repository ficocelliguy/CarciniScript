// A type parameter coerced to a `dyn Trait` its bound names: `Box::new(val)` as `Box<dyn Tr>`,
// `Some(Box::new(val))`, `&T` as `&dyn Tr`.
trait Tr { fn n(&self) -> String; }
struct S(u8);
impl Tr for S { fn n(&self) -> String { format!("S{}", self.0) } }
impl Tr for u8 { fn n(&self) -> String { format!("u8 {}", self) } }
impl Tr for f64 { fn n(&self) -> String { format!("f64 {}", self) } }
impl Tr for String { fn n(&self) -> String { format!("str {}", self) } }
fn boxed<T: Tr + 'static>(val: T) -> Box<dyn Tr> { Box::new(val) }
fn opt<T: Tr + 'static>(val: T) -> Option<Box<dyn Tr>> { Some(Box::new(val)) }
fn r<T: Tr>(val: &T) -> &dyn Tr { val }
fn all<T>(v: Vec<T>) -> Vec<Box<dyn Tr>> where T: Tr + 'static {
    v.into_iter().map(|x| Box::new(x) as Box<dyn Tr>).collect()
}
fn main() {
    let v = vec![boxed(S(1)), boxed(3u8), boxed(2.5f64), boxed(String::from("a"))];
    for b in &v { println!("{}", b.n()); }
    println!("{} {}", opt(S(2)).unwrap().n(), r(&S(3)).n());
    for b in all(vec![S(4), S(5)]) { println!("{}", b.n()); }
}
