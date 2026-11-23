// `fmt::Error` written in a type (`-> Result<(), fmt::Error>`), also next to a type of the
// file's own named `Error`.
use std::fmt;
use std::fmt::Write;

#[derive(Debug)]
struct Error(u8);

fn render(out: &mut String, n: u8) -> Result<(), fmt::Error> {
    write!(out, "n={}", n)?;
    if n > 9 {
        return Err(fmt::Error);
    }
    Ok(())
}

fn full() -> Result<u8, std::fmt::Error> {
    Err(std::fmt::Error)
}

fn own(n: u8) -> Result<u8, Error> {
    if n == 0 { Err(Error(n)) } else { Ok(n) }
}

fn main() {
    let mut s = String::new();
    println!("{:?} {:?} {}", render(&mut s, 1), render(&mut s, 12), s);
    println!("{:?} {:?} {:?}", full(), own(0), own(4));
    let e: fmt::Error = fmt::Error;
    println!("{} {:?}", e, e);
}
