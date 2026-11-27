// `use std::fmt::Write as FmtWrite;` / `use std::io::Write as IoWrite;` (aliases that are also
// the preludes' own names) and other aliases, as bounds on type parameters.
use std::fmt::Write as FmtWrite;
use std::fmt::Write as W2;
use std::io::Write as IoWrite;

fn put<W: FmtWrite>(w: &mut W, s: &str) -> std::fmt::Result {
    w.write_str(s)?;
    w.write_char('!')
}

fn put2<W: W2>(w: &mut W) -> std::fmt::Result {
    write!(w, "<{}>", 5)
}

fn bytes<W: IoWrite>(w: &mut W, b: &[u8]) -> std::io::Result<usize> {
    w.write(b)
}

fn main() {
    let mut s = String::new();
    put(&mut s, "hi").unwrap();
    put2(&mut s).unwrap();
    println!("{s}");
    let mut v: Vec<u8> = Vec::new();
    let n = bytes(&mut v, b"abc").unwrap();
    println!("{n} {:?}", v);
}
