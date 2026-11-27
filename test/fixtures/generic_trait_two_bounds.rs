// One type parameter bounded by one generic trait twice (`C: Conv<u8> + Conv<u16>`): each call
// takes the bound its use settles (an annotation, a later use, a literal's kind, or the trait
// arguments written on a qualified call), and passes that bound's classes to pick the impl.
trait Conv<T> { fn conv(&self) -> T; }
struct M(i32);
impl Conv<u8> for M { fn conv(&self) -> u8 { self.0 as u8 } }
impl Conv<u16> for M { fn conv(&self) -> u16 { (self.0 * 1000) as u16 } }

trait Scale<R> { fn scale(&self, r: R) -> String; }
impl Scale<f64> for M { fn scale(&self, r: f64) -> String { format!("f64 {}", self.0 as f64 * r) } }
impl Scale<i32> for M { fn scale(&self, r: i32) -> String { format!("i32 {}", self.0 * r) } }

fn both<C>(c: &C) -> u32 where C: Conv<u8> + Conv<u16> {
    let a: u8 = c.conv();
    let b: u16 = c.conv();
    a as u32 + b as u32
}
fn ufcs<C: Conv<u8> + Conv<u16>>(c: &C) -> (u8, u16) {
    (Conv::<u8>::conv(c), <C as Conv<u16>>::conv(c))
}
fn later<C: Conv<u16> + Conv<u8>>(c: &C) -> u8 {
    let x = c.conv();
    let y: u8 = x;
    y
}
fn one<C: Conv<u16>>(c: &C) -> (u16, u16, u16) {
    (Conv::conv(c), C::conv(c), <C as Conv<u16>>::conv(c))
}
fn sc<S: Scale<f64> + Scale<i32>>(s: &S) -> String {
    format!("{} | {} | {}", s.scale(1.5), s.scale(2), s.scale(2.0))
}

fn main() {
    let m = M(3);
    println!("{}", both(&m));
    println!("{:?}", ufcs(&m));
    println!("{}", later(&m));
    println!("{:?}", one(&m));
    println!("{}", sc(&m));
}
