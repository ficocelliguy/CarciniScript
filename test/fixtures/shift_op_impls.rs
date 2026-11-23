// `impl Shl` / `Shr` / `ShlAssign` for your own types: by an integer amount or by a struct (picked
// by the right operand), through a generic `T: Shl<u32, Output = T>`, as `Shl::shl(..)` and on a
// Vec element.
use std::ops::{Shl, ShlAssign, Shr};
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bits(u32);
impl Shl<u32> for Bits {
    type Output = Bits;
    fn shl(self, n: u32) -> Bits {
        Bits(self.0 << n)
    }
}
impl Shr<u32> for Bits {
    type Output = u32;
    fn shr(self, n: u32) -> u32 {
        self.0 >> n
    }
}
impl Shl<Bits> for Bits {
    type Output = Bits;
    fn shl(self, n: Bits) -> Bits {
        Bits(self.0 << n.0)
    }
}
impl ShlAssign<usize> for Bits {
    fn shl_assign(&mut self, n: usize) {
        self.0 <<= n as u32;
    }
}
fn dbl<T: Shl<u32, Output = T>>(t: T) -> T {
    t << 1
}
fn main() {
    let b = Bits(3);
    println!("{:?}", b << 2u32);
    println!("{:?}", b << Bits(1));
    let r: u32 = Bits(64) >> 3;
    println!("{}", r + 1);
    println!("{:?}", dbl(Bits(5)));
    println!("{:?}", Shl::shl(b, 3u32));
    let mut c = Bits(1);
    c <<= 4usize;
    println!("{:?}", c);
    let mut v = vec![Bits(1), Bits(2)];
    v[1] <<= 3usize;
    println!("{:?}", v);
    more();
}

#[derive(Debug, Clone, Copy)]
struct Mask(u8);
impl std::ops::Shr<u8> for Mask {
    type Output = Mask;
    fn shr(self, n: u8) -> Mask {
        Mask(self.0 >> n)
    }
}
impl std::ops::ShrAssign<u8> for Mask {
    fn shr_assign(&mut self, n: u8) {
        self.0 >>= n;
    }
}

fn halve_all<T: std::ops::ShrAssign<u8> + Copy>(xs: &mut [T]) {
    for x in xs.iter_mut() {
        *x >>= 1;
    }
}

pub fn more() {
    let mut m = Mask(0b1100_0000);
    m >>= 2;
    println!("{:?} {:?}", m, m >> 4);
    let mut ms = [Mask(8), Mask(6)];
    halve_all(&mut ms);
    println!("{:?}", ms);
}
