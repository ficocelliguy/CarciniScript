// Irrefutable patterns: `Ok(..)` over a `Result` whose error type is uninhabited, and an array
// pattern in a tuple pattern over a tuple local declared elsewhere.
use std::convert::Infallible;
use std::str::FromStr;
enum Never {}
struct Name(String);
impl FromStr for Name {
    type Err = Infallible;
    fn from_str(s: &str) -> Result<Self, Infallible> { Ok(Name(s.to_uppercase())) }
}
fn get() -> Result<u8, Infallible> { Ok(4) }
fn pair(t: ([u8; 2], char)) -> u8 {
    let ([x, y], _) = t;
    x + y
}
fn main() {
    let Ok(p): Result<_, Infallible> = get();
    let Ok(Name(n)) = "bo".parse::<Name>();
    let q = match get() { Ok(q) => q };
    let r: Result<u8, Never> = Ok(2);
    let Ok(s) = r;
    println!("{} {} {} {}", p, n, q, s);
    let t = ([1, 2], 3);
    let ([a, b], c) = t;
    let u = t;
    let ([d, ..], _) = u;
    println!("{} {} {} {} {}", a, b, c, d, pair(([5, 6], 'z')));
}
