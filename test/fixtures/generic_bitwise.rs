// Bitwise operators on type parameters: `&`, `|`, `^`, `!`, `<<`, `>>` and their assignments by
// the caller's type (bools, every integer width, wrapping and masked shifts), and user types'
// own `BitOr` / `Not` impls.
use std::ops::{BitAnd, BitOr, BitXor, BitXorAssign, Not, Shl, Shr, BitAndAssign, BitOrAssign, ShlAssign, ShrAssign};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Flags(u8);

impl BitOr for Flags {
    type Output = Flags;
    fn bitor(self, o: Flags) -> Flags {
        Flags(self.0 | o.0)
    }
}

impl BitAnd for Flags {
    type Output = Flags;
    fn bitand(self, o: Flags) -> Flags {
        Flags(self.0 & o.0)
    }
}

impl BitXor for Flags {
    type Output = Flags;
    fn bitxor(self, o: Flags) -> Flags {
        Flags(self.0 ^ o.0)
    }
}

impl Not for Flags {
    type Output = Flags;
    fn not(self) -> Flags {
        Flags(!self.0)
    }
}

impl BitOrAssign for Flags {
    fn bitor_assign(&mut self, o: Flags) {
        self.0 |= o.0;
    }
}

fn mask<T: BitAnd<Output = T> + BitOr<Output = T> + BitXor<Output = T> + Copy>(a: T, b: T) -> (T, T, T) {
    (a & b, a | b, a ^ b)
}

fn flip<T: Not<Output = T>>(x: T) -> T {
    !x
}

fn shifts<T: Shl<u32, Output = T> + Shr<u32, Output = T> + Copy>(x: T, n: u32) -> (T, T) {
    (x << n, x >> n)
}

fn accumulate<T: BitOrAssign + BitAndAssign + BitXorAssign + Copy>(xs: &[T], start: T) -> T {
    let mut acc = start;
    for &x in xs {
        acc |= x;
    }
    acc &= xs[1];
    acc ^= xs[0];
    acc
}

fn merge<T: BitOrAssign + Copy>(xs: &[T]) -> T {
    let mut acc = xs[0];
    for &x in &xs[1..] {
        acc |= x;
    }
    acc
}

fn shift_assign<T: ShlAssign<u32> + ShrAssign<u32> + Copy>(mut x: T) -> T {
    x <<= 3;
    x >>= 1;
    x
}

fn clear<T: BitAnd<Output = T> + Not<Output = T>>(x: T, bit: T) -> T {
    x & !bit
}

fn main() {
    println!("{:?}", mask(12u8, 10u8));
    println!("{:?}", mask(-12i32, 10));
    println!("{:?}", mask(0xF0F0_F0F0u32, 0xFF00_FF00));
    println!("{:?}", mask(1u64 << 40 | 5, 7));
    println!("{:?}", mask(-3i64, 6));
    println!("{:?}", mask(0x1F_FFFF_FFFFusize, 0xFFFF_0000));
    println!("{:?}", mask(true, false));
    println!("{:?}", mask(Flags(3), Flags(6)).1);
    println!("{} {} {} {} {}", flip(5u8), flip(5i8), flip(0u32), flip(7i64), flip(-1i128));
    println!("{} {} {:?}", flip(true), flip(-6isize), flip(Flags(0x0F)));
    println!("{:?} {:?} {:?}", shifts(200u8, 2), shifts(-16i32, 2), shifts(1u64, 50));
    println!("{:?} {:?}", shifts(0x8000_0000u32, 4), shifts(-1i16, 15));
    println!("{:?} {:?}", shifts(3usize, 40), shifts(-1024isize, 3));
    println!("{}", accumulate(&[1u8, 2, 4], 8));
    println!("{}", accumulate(&[true, false], false));
    println!("{} {}", shift_assign(0x7fu8), shift_assign(-5i64));
    println!("{} {:?}", clear(0xFFu8, 0x10), clear(Flags(7), Flags(2)));
    let mut f = Flags(1);
    f |= Flags(4);
    println!("{:?} {:?} {}", f, merge(&[Flags(1), Flags(2), Flags(8)]), merge(&[1u16, 256]));
}
