// `try_into()` from a slice or `Vec` to an array checks the length the code around it writes:
// a `let` annotation, a parameter, a return type, `from_le_bytes`, a `const` length.
use std::convert::TryInto;

fn part1() {
    let v = vec![1, 2, 3];
    let a: Result<[i32; 3], _> = v.as_slice().try_into();
    println!("{:?}", a.is_ok());
    let b: Result<[i32; 2], _> = v.as_slice().try_into();
    println!("{:?}", b.is_ok());
    let s = &v[..];
    let c: Result<&[i32; 3], _> = s.try_into();
    println!("{:?}", c);
    let d: Result<[i32; 4], _> = v.clone().try_into();
    println!("{:?}", d);
    let e: [i32; 3] = v.clone().try_into().unwrap();
    println!("{:?}", e);
    fn f(x: [u8; 2]) -> u8 { x[0] + x[1] }
    let bytes = [1u8, 2, 3, 4];
    println!("{}", f(bytes[1..3].try_into().unwrap()));
    match <[u8; 3]>::try_from(&bytes[..2]) { Ok(_) => println!("ok"), Err(e) => println!("err {}", e) }
    let r: Result<[u8; 3], _> = bytes[..2].try_into();
    if let Err(e) = r { println!("{}", e); }
}

struct P;
impl P { fn take(&self, a: [i32; 2]) -> i32 { a[0] - a[1] } }
fn last4(s: &[u8]) -> [u8; 4] { s[s.len() - 4..].try_into().unwrap() }
fn head(s: &[u8]) -> Option<[u8; 2]> { s.get(..3)?.try_into().ok() }
fn first3(v: Vec<i32>) -> Result<[i32; 3], Vec<i32>> { v.try_into() }
fn part2() {
    let b = [1u8, 2, 3, 4, 5, 6, 7, 8, 9];
    println!("{}", u32::from_le_bytes(b[0..4].try_into().unwrap()));
    println!("{:?}", last4(&b));
    println!("{:?}", head(&b));
    println!("{:?} {:?}", first3(vec![1, 2, 3]), first3(vec![1]));
    let p = P;
    println!("{}", p.take([5, 1][..].try_into().unwrap()));
    let o: Option<[u8; 5]> = b[..3].try_into().ok();
    println!("{:?}", o);
    let c: [u8; 3] = b[..3].try_into().expect("three");
    println!("{:?}", c);
    more();
}
const N: usize = 3;
fn more() {
    let v = vec![1, 2];
    let r: Result<[i32; N], _> = v.as_slice().try_into();
    println!("{:?}", r.is_err());
}

fn main() {
    part1();
    part2();
}
