// `match` arms coerced to an expected `dyn`: `Error::source` returning `Some(e)` of two error types,
// boxed shapes and closures, a `Box<dyn Error>` in `Err`.
use std::error::Error;
use std::fmt;
#[derive(Debug)] struct A; #[derive(Debug)] struct B;
impl fmt::Display for A { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "A") } }
impl fmt::Display for B { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "B") } }
impl Error for A {} impl Error for B {}
#[derive(Debug)] enum E { X(A), Y(B), Z }
impl fmt::Display for E { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "E") } }
impl Error for E {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self { E::X(e) => Some(e), E::Y(e) => Some(e), E::Z => None }
    }
}
fn main() {
    main2();
    for e in [E::X(A), E::Y(B), E::Z] { println!("{:?}", e.source().map(|s| s.to_string())); }
}
trait Shape { fn area(&self) -> f64; }
struct Sq(f64);
struct Ci(f64);
impl Shape for Sq { fn area(&self) -> f64 { self.0 * self.0 } }
impl Shape for Ci { fn area(&self) -> f64 { 3.0 * self.0 * self.0 } }
fn make(k: u8) -> Box<dyn Shape> {
    match k { 0 => Box::new(Sq(2.0)), _ => Box::new(Ci(1.0)) }
}
fn pick<'a>(k: u8, s: &'a Sq, c: &'a Ci) -> &'a dyn Shape {
    match k { 0 => s, _ => c }
}
fn op(k: u8) -> Box<dyn Fn(i32) -> i32> {
    match k { 0 => Box::new(|x| x + 1), 1 => Box::new(move |x| x * k as i32), _ => Box::new(|x| -x) }
}
fn err(k: u8) -> Result<(), Box<dyn Error>> {
    match k { 0 => Err(Box::new(A)), 1 => Err(Box::new(B)), _ => Ok(()) }
}
fn main2() {
    println!("{} {}", make(0).area(), make(1).area());
    println!("{}", pick(1, &Sq(1.0), &Ci(2.0)).area());
    println!("{} {} {}", op(0)(5), op(1)(5), op(2)(5));
    for k in 0..3 { println!("{:?}", err(k).map_err(|e| e.to_string())); }
    let n = match 3 { 1 => 1u8, _ => 2 };
    println!("{}", n);
}
