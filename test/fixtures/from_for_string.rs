// `impl From<E> for String` (a std target): `?` into a `Result<_, String>` picks the impl by the
// error type, beside std's `&str` -> `String`; `String::from` as a function value
// (`.map_err(String::from)`) and `.into()`, and `impl From<A> for i64`.
use std::num::ParseIntError;
#[derive(Debug)]
struct A(i32);
#[derive(Debug)]
enum B {
    X,
    Y(String),
}
impl From<A> for String {
    fn from(a: A) -> String {
        format!("A{}", a.0)
    }
}
impl From<B> for String {
    fn from(b: B) -> String {
        match b {
            B::X => "bx".to_string(),
            B::Y(s) => format!("by:{s}"),
        }
    }
}
impl From<A> for i64 {
    fn from(a: A) -> i64 {
        a.0 as i64 * 100
    }
}
fn fa(n: i32) -> Result<i32, A> {
    if n < 0 {
        Err(A(n))
    } else {
        Ok(n)
    }
}
fn fb(n: i32) -> Result<i32, B> {
    match n {
        0 => Err(B::X),
        1 => Err(B::Y("one".into())),
        _ => Ok(n),
    }
}
fn fs(n: i32) -> Result<i32, &'static str> {
    if n == 9 {
        Err("nine")
    } else {
        Ok(n)
    }
}
fn both(a: i32, b: i32) -> Result<i32, String> {
    let x = fa(a)?;
    let y = fb(b)?;
    let z = fs(y)?;
    let w: i32 = "12".parse().map_err(|e: ParseIntError| e.to_string())?;
    Ok(x + z + w)
}
fn num(a: i32) -> Result<i32, i64> {
    let v = fa(a)?;
    Ok(v)
}
fn mapped(a: i32) -> Result<i32, String> {
    fa(a).map_err(String::from)
}
fn main() {
    for (a, b) in [(1, 2), (-3, 2), (1, 0), (1, 1), (1, 9)] {
        println!("{:?}", both(a, b));
    }
    println!("{:?} {:?}", num(4), num(-2));
    println!("{:?} {:?}", mapped(4), mapped(-5));
    let s: String = B::Y("z".into()).into();
    println!("{s}");
    let v: Vec<String> = ["a", "b"].iter().map(|s| *s).map(String::from).collect();
    let w: Vec<String> = vec![A(1), A(2)].into_iter().map(String::from).collect();
    println!("{v:?} {w:?}");
}
