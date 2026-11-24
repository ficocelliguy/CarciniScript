// The program's own types named like std's error types (deranged's `ParseIntError`).
use std::str::FromStr;
#[derive(Debug)]
pub struct ParseIntError { neg: bool, inner: Option<std::num::ParseIntError> }
#[derive(Debug)]
pub struct TryFromIntError;
pub struct R(i32);
impl FromStr for R {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let v = s.parse::<i32>().map_err(|e| ParseIntError { neg: false, inner: Some(e) })?;
        if v < 0 { return Err(ParseIntError { neg: true, inner: None }); }
        Ok(R(v))
    }
}
impl TryFrom<i64> for R {
    type Error = TryFromIntError;
    fn try_from(v: i64) -> Result<Self, Self::Error> { if v < 100 { Ok(R(v as i32)) } else { Err(TryFromIntError) } }
}
fn std_one(s: &str) -> Result<u8, std::num::ParseIntError> { s.parse() }
fn main() {
    println!("{:?}", "12".parse::<R>().map(|r| r.0));
    println!("{:?}", "x".parse::<R>().map(|r| r.0));
    println!("{:?}", "-3".parse::<R>().map(|r| r.0));
    println!("{:?}", R::try_from(5i64).map(|r| r.0));
    println!("{:?}", R::try_from(500i64).map(|r| r.0));
    println!("{:?}", std_one("300"));
    let e: ParseIntError = ParseIntError { neg: true, inner: None };
    println!("{}", e.neg);
}
