// Reading through a `&mut &[u8]` (`fn take(r: &mut &[u8])`, `take(&mut s)`): the reads advance
// the caller's `s`, and `*r = &r[n..]` moves it on, without touching the `Vec` it was sliced
// from. Also an `R: Read` taken as `r: &mut R`, and a `&mut &str`.
use std::io::Read;

fn take(r: &mut &[u8]) -> u8 {
    let mut b = [0u8; 1];
    r.read_exact(&mut b).unwrap();
    b[0]
}

fn skip(r: &mut &[u8], n: usize) {
    *r = &r[n..];
}

fn first(r: &mut &[u8]) -> u8 {
    let b = r[0];
    *r = &r[1..];
    b
}

fn get<R: Read>(r: &mut R) -> u8 {
    let mut b = [0u8; 1];
    r.read_exact(&mut b).unwrap();
    b[0]
}

fn rest<R: Read>(r: &mut R) -> Vec<u8> {
    let mut v = Vec::new();
    r.read_to_end(&mut v).unwrap();
    v
}

fn adv(r: &mut &str) -> char {
    let c = r.chars().next().unwrap();
    *r = &r[1..];
    c
}

fn whole(mut r: impl Read) -> Vec<u8> {
    let mut v = Vec::new();
    r.read_to_end(&mut v).unwrap();
    v
}

fn main() {
    let v = vec![1u8, 2, 3, 4, 5, 6, 7];
    let mut s: &[u8] = &v;
    let a = take(&mut s);
    skip(&mut s, 1);
    let b = take(&mut s);
    let c = first(&mut s);
    println!("{} {} {} {:?} {:?}", a, b, c, s, v);

    let t = s;
    let d = get(&mut s);
    let e = rest(&mut s);
    println!("{} {:?} {:?} {:?} {:?}", d, e, s, t, v);

    let owned = String::from("hello");
    let mut w: &str = &owned;
    let ch = adv(&mut w);
    println!("{} {} {}", ch, w, owned);

    println!("{:?} {:?}", whole(&v[..]), whole(&v[5..]));
}
