#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Comma;

macro_rules! Token {
    [,] => { Comma };
}

#[derive(Debug, Default)]
struct Punct {
    sep: Comma,
    n: u8,
}

fn main() {
    let c = <Token![,]>::default();
    let d = <(Comma)>::default();
    let p = <(Punct)>::default();
    let w = <Vec<u8>>::with_capacity(2);
    println!("{:?} {} {:?} {}", c, c == d, p, w.len());
}
