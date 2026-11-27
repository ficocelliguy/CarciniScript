// Your own `impl Index<Range<usize>>` / `Index<RangeFrom<usize>>` / `Index<RangeTo<usize>>`
// used with a range expression: `s[a..b]`, `s[a..]`, `s[..b]`, beside `Index<usize>`.
use std::ops::{Index, Range, RangeFrom, RangeTo};

struct Buf {
    v: Vec<u8>,
}

impl Index<usize> for Buf {
    type Output = u8;
    fn index(&self, i: usize) -> &u8 {
        &self.v[i]
    }
}

impl Index<Range<usize>> for Buf {
    type Output = [u8];
    fn index(&self, r: Range<usize>) -> &[u8] {
        &self.v[r]
    }
}

impl Index<RangeFrom<usize>> for Buf {
    type Output = [u8];
    fn index(&self, r: RangeFrom<usize>) -> &[u8] {
        &self.v[r]
    }
}

impl std::ops::Index<RangeTo<usize>> for Buf {
    type Output = [u8];
    fn index(&self, r: RangeTo<usize>) -> &[u8] {
        &self.v[r]
    }
}

fn main() {
    let b = Buf { v: vec![10, 20, 30, 40, 50] };
    let i = 1;
    println!("{}", b[i]);
    println!("{:?}", &b[i..i + 2]);
    println!("{:?} {}", &b[2..], b[2..].len());
    println!("{:?}", &b[..3]);
    let total: u32 = b[1..4].iter().map(|&x| x as u32).sum();
    println!("{total}");
}
