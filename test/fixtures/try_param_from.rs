// `?` converting through a type parameter's `From` bound: `fn f<E: From<E1>>() -> Result<_, E>`
// (also `where`, two bounds, closures, a method's and an impl's parameter), and `E::from` with
// two bounds.
use std::marker::PhantomData;
use std::num::ParseIntError;
#[derive(Debug)]
struct E1(u8);
#[derive(Debug)]
struct E4(char);
#[derive(Debug)]
enum E2 {
    One(u8),
    Four(char),
    Parse(String),
}
impl From<E1> for E2 {
    fn from(e: E1) -> E2 {
        E2::One(e.0)
    }
}
impl From<E4> for E2 {
    fn from(e: E4) -> E2 {
        E2::Four(e.0)
    }
}
impl From<ParseIntError> for E2 {
    fn from(e: ParseIntError) -> E2 {
        E2::Parse(e.to_string())
    }
}
#[derive(Debug)]
struct E3(String);
impl From<E1> for E3 {
    fn from(e: E1) -> E3 {
        E3(format!("<{}>", e.0))
    }
}
fn inner(x: u8) -> Result<u8, E1> {
    if x > 3 { Err(E1(x)) } else { Ok(x) }
}
fn inner4(c: char) -> Result<char, E4> {
    if c == 'z' { Err(E4(c)) } else { Ok(c) }
}
fn conv<E: From<E1>>(x: u8) -> Result<u8, E> {
    let v = inner(x)?;
    Ok(v + 1)
}
fn parse<E>(s: &str) -> Result<i32, E>
where
    E: From<ParseIntError>,
{
    let n: i32 = s.trim().parse()?;
    Ok(n * 2)
}
fn both<E: From<E1> + From<E4>>(x: u8, c: char) -> Result<String, E> {
    let a = inner(x)?;
    let b = inner4(c)?;
    Ok(format!("{a}{b}"))
}
fn both_from<E: From<E1> + From<E4>>(x: u8) -> E {
    if x > 0 { E::from(E1(x)) } else { E::from(E4('q')) }
}
fn closure<E: From<E1> + std::fmt::Debug>(xs: &[u8]) -> Vec<Result<u8, E>> {
    let f = |x: u8| -> Result<u8, E> { Ok(inner(x)? * 3) };
    xs.iter().map(|&x| f(x)).collect()
}
fn try_all<T, E, F>(xs: &[u8], f: F) -> Result<Vec<T>, E>
where
    F: Fn(u8) -> Result<T, E1>,
    E: From<E1>,
{
    let mut out = vec![];
    for &x in xs {
        out.push(f(x)?);
    }
    Ok(out)
}
fn fold_all<T, E, E2, B, I, F>(it: &mut I, init: B, mut f: F) -> Result<B, E2>
where
    I: Iterator<Item = Result<T, E>>,
    E2: From<E>,
    F: FnMut(B, T) -> Result<B, E2>,
{
    it.try_fold(init, |acc, v| f(acc, v?))
}
struct Runner {
    base: u8,
}
impl Runner {
    fn run<E: From<E1>>(&self, x: u8) -> Result<u8, E> {
        let a = inner(x)?;
        let b = inner(self.base)?;
        Ok(a + b)
    }
}
struct P<E> {
    _e: PhantomData<E>,
}
impl<E: From<E1>> P<E> {
    fn go(x: u8) -> Result<u8, E> {
        Ok(inner(x)? + 100)
    }
    fn go_self(&self, x: u8) -> Result<u8, E> {
        Ok(inner(x)? + 50)
    }
}
fn main() {
    let r: Result<u8, E2> = conv(2);
    println!("{:?}", r);
    let r: Result<u8, E3> = conv(9);
    println!("{:?}", r);
    println!("{:?} {:?}", parse::<E2>("21"), parse::<E2>("x1"));
    println!("{:?}", both::<E2>(1, 'a'));
    println!("{:?}", both::<E2>(9, 'a'));
    println!("{:?}", both::<E2>(1, 'z'));
    println!("{:?} {:?}", both_from::<E2>(4), both_from::<E2>(0));
    let r: Vec<Result<u8, E3>> = closure(&[1, 5]);
    println!("{:?}", r);
    let r: Vec<Result<u8, E2>> = closure(&[2, 9]);
    println!("{:?}", r);
    let r: Result<Vec<u8>, E2> = try_all(&[1, 2, 7], inner);
    println!("{:?}", r);
    let r: Result<Vec<u8>, E2> = try_all(&[1, 2], inner);
    println!("{:?}", r);
    let rn = Runner { base: 2 };
    println!("{:?} {:?}", rn.run::<E2>(1), rn.run::<E3>(8));
    let bad = Runner { base: 7 };
    let r: Result<u8, E2> = bad.run(1);
    println!("{:?}", r);
    println!("{:?} {:?}", P::<E2>::go(2), P::<E3>::go(5));
    let p: P<E2> = P { _e: PhantomData };
    println!("{:?}", p.go_self(6));
    let mut it = vec![Ok::<u8, E1>(1), Ok(2)].into_iter();
    let r: Result<u8, E2> = fold_all(&mut it, 0u8, |a, b| Ok(a + b));
    println!("{:?}", r);
    let mut it = vec![Ok::<u8, E1>(1), Err(E1(7)), Ok(2)].into_iter();
    let r: Result<u8, E3> = fold_all(&mut it, 0u8, |a, b| Ok(a + b));
    println!("{:?}", r);
}
