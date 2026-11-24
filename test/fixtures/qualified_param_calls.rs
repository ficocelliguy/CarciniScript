// `<T as Tr>::f()` and `Tr::f()` where a `T` is expected, on a type parameter `T: Tr` (num-traits'
// `<T as Bounded>::min_value()`, `NumCast::from(n)`), and `T::name(t)` / `Tr::name(t)` /
// `<T as Tr>::name(t)` on a `T` that is a number with your impl.
trait Bounded { fn min_value() -> Self; fn name(&self) -> String; }
impl Bounded for i32 { fn min_value() -> Self { -5 } fn name(&self) -> String { format!("i{}", self) } }
impl Bounded for u8 { fn min_value() -> Self { 1 } fn name(&self) -> String { format!("u{}", self) } }
#[derive(Debug)]
struct Pt(i32);
impl Bounded for Pt { fn min_value() -> Self { Pt(-1) } fn name(&self) -> String { format!("P{}", self.0) } }
trait Lower { fn lowest() -> Self; }
impl<T: Bounded> Lower for T { fn lowest() -> T { Bounded::min_value() } }
trait NumCast: Sized { fn from(n: i64) -> Option<Self>; }
impl NumCast for u8 { fn from(n: i64) -> Option<u8> { if (0..256).contains(&n) { Some(n as u8) } else { None } } }
impl NumCast for i32 { fn from(n: i64) -> Option<i32> { Some(n as i32) } }
fn cast<U: NumCast>(n: i64) -> Option<U> { NumCast::from(n) }
fn lo<T: Bounded>() -> T { <T as Bounded>::min_value() }
fn lo2<T: Bounded>() -> T { Bounded::min_value() }
fn pair<T: Bounded + std::fmt::Debug>() -> String { format!("{:?}", (lo::<T>(), lo2::<T>())) }
fn n1<T: Bounded>(t: &T) -> String { <T as Bounded>::name(t) }
fn n2<T: Bounded>(t: &T) -> String { T::name(t) }
fn n3<T: Bounded>(t: &T) -> String { Bounded::name(t) }
fn main() {
    let a: i32 = lo();
    let b: u8 = lo2();
    println!("{} {} {:?} {}", a, b, lo::<Pt>(), pair::<u8>());
    println!("{:?} {:?} {:?}", cast::<u8>(300), cast::<u8>(7), cast::<i32>(-3));
    println!("{} {} {}", n1(&3u8), n2(&4i32), n3(&Pt(2)));
    println!("{} {} {}", n1(&Pt(5)), n2(&9u8), n3(&-1i32));
    println!("{} {}", i32::lowest(), u8::lowest());
}
