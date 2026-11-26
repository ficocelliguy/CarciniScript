// std's `impl Read for &[u8]`: `read` / `read_exact` / `read_to_end` / `read_to_string` on a slice
// binding (a local or a field) take bytes off its front, leaving what it was sliced from as it was.
use std::io::Read;

struct Parser<'a> {
    input: &'a [u8],
}

impl<'a> Parser<'a> {
    fn byte(&mut self) -> u8 {
        let mut b = [0u8; 1];
        self.input.read_exact(&mut b).unwrap();
        b[0]
    }
}

fn main() {
    let data = b"hello world".to_vec();
    let mut s: &[u8] = &data;
    let mut buf = [0u8; 5];
    let n = s.read(&mut buf).unwrap();
    println!("{} {:?} {:?}", n, buf, s);
    let mut big = [0u8; 20];
    let n = s.read(&mut big).unwrap();
    println!("{} {:?} {}", n, &big[..n], s.len());
    let mut s2: &[u8] = b"abcdef";
    let mut rest = Vec::new();
    let mut two = [0u8; 2];
    s2.read_exact(&mut two).unwrap();
    s2.read_to_end(&mut rest).unwrap();
    println!("{:?} {:?} {}", two, rest, s2.is_empty());
    let mut s3 = &b"xyz"[..];
    let mut text = String::new();
    s3.read_to_string(&mut text).unwrap();
    println!("{} {}", text, s3.len());
    let mut s4: &[u8] = b"q";
    println!("{:?}", s4.read_exact(&mut two).is_err());
    let mut p = Parser { input: b"\x01\x02\x03" };
    println!("{} {} {:?}", p.byte(), p.byte(), p.input);
    aliasing();
}

// The slice a read advances is the binding's own: what it was sliced from stays.
fn aliasing() {
    let data = vec![1u8, 2, 3, 4];
    let mut s: &[u8] = &data;
    let t = s;
    let mut b = [0u8; 3];
    s.read(&mut b).unwrap();
    println!("{:?} {:?} {:?} {:?}", s, t, data, b);
    let mut u = &data[1..];
    u.read(&mut b[..1]).unwrap();
    println!("{:?} {:?} {:?}", u, data, b);
}
