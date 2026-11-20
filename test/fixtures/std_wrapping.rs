// `std::num::Wrapping`: arithmetic that wraps around (a hash, a checksum, an LCG), `{}` / `{:?}`,
// comparisons, the assignment operators, and your own trait on `Wrapping<u32>`.
use std::num::Wrapping;

trait Mix {
    fn mix(self, k: u32) -> Self;
}

impl Mix for Wrapping<u32> {
    fn mix(self, k: u32) -> Self {
        (self ^ Wrapping(k)) * Wrapping(0x9E37_79B9)
    }
}

fn fnv1a(s: &str) -> Wrapping<u32> {
    let mut h = Wrapping(0x811c_9dc5u32);
    for b in s.bytes() {
        h ^= Wrapping(b as u32);
        h *= Wrapping(0x0100_0193);
    }
    h
}

struct Lcg(Wrapping<u32>);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0 * Wrapping(1664525) + Wrapping(1013904223);
        self.0 .0 >> 16
    }
}

fn main() {
    let a = Wrapping(250u8);
    let b = Wrapping(10u8);
    println!("{} {} {:?} {}", a + b, b - a, a * b, a.0);
    let c = Wrapping(i32::MIN);
    println!("{} {} {}", -c, c - Wrapping(1), !Wrapping(0i8));
    println!("{} {}", fnv1a("n00dles"), fnv1a("joesguns"));
    let mut rng = Lcg(Wrapping(42));
    println!("{} {} {}", rng.next(), rng.next(), rng.next());
    let mut total = Wrapping(u16::MAX);
    total += Wrapping(2);
    total -= Wrapping(5);
    println!("{total} {} {}", total < Wrapping(10), Wrapping(7u32).mix(3));
}
