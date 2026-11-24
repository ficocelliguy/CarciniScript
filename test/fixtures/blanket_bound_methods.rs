// A trait a blanket impl gives a type parameter through its bounds (`impl<T: Bounded> Lower
// for T`, then `T::low()` and `x.describe()` in `fn g<T: Bounded>`).
use std::fmt::Debug;
trait Bounded { fn lowest() -> Self; }
impl Bounded for i32 { fn lowest() -> i32 { i32::MIN } }
impl Bounded for u8 { fn lowest() -> u8 { 0 } }
trait Lower { fn low() -> Self; fn describe(&self) -> String; }
impl<T: Bounded + Debug> Lower for T {
    fn low() -> T { T::lowest() }
    fn describe(&self) -> String { format!("<{:?}>", self) }
}
fn g<T: Bounded + Debug>() -> T { T::low() }
fn h<T: Bounded + Debug>(x: T) -> String { x.describe() + &T::low().describe() }
fn w<T>(x: T) -> String where T: Bounded + Debug { x.describe() }
fn main() {
    let a: i32 = g();
    let b = g::<u8>();
    println!("{} {} {} {}", a, b, h(5u8), w(-3));
}
