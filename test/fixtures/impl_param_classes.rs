// A type parameter's class where it used to be out of reach: `T::..` in a method with `self` of
// `impl<T: Tr> Tr for Vec<T>` (handed over by the caller that knows the `Vec`'s type), an
// associated const of a generic impl that reads `T` (read through a class view carrying `T`'s
// class), trait consts by path (`i32::ZERO`, `<P as Tr>::ZERO`, `Tr::ZERO`), and std bounds on an
// associated type (`type Seed: Default + AsMut<[u8]>`).
use std::num::Wrapping;

trait Codec {
    const SIZE_LEN: usize;
    fn encode(&self, out: &mut Vec<u8>);
    fn label() -> String;
}
impl Codec for u8 {
    const SIZE_LEN: usize = 1;
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(*self)
    }
    fn label() -> String {
        "u8".into()
    }
}
impl Codec for u16 {
    const SIZE_LEN: usize = 2;
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes())
    }
    fn label() -> String {
        "u16".into()
    }
}
impl<T: Codec> Codec for Vec<T> {
    const SIZE_LEN: usize = 0;
    fn encode(&self, out: &mut Vec<u8>) {
        out.push((self.len() * T::SIZE_LEN) as u8);
        out.push(T::label().len() as u8);
        for x in self {
            x.encode(out)
        }
    }
    fn label() -> String {
        format!("Vec<{}>", T::label())
    }
}

fn encode_all<C: Codec>(c: &C) -> Vec<u8> {
    let mut o = Vec::new();
    c.encode(&mut o);
    o
}

trait ConstZero {
    const ZERO: Self;
}
impl ConstZero for i32 {
    const ZERO: i32 = 0;
}
impl ConstZero for f64 {
    const ZERO: f64 = 0.5;
}
impl<T: ConstZero> ConstZero for Wrapping<T> {
    const ZERO: Self = Wrapping(T::ZERO);
}
#[derive(Debug, Clone, Copy)]
struct Pair<T>(T, T);
impl<T: ConstZero + Copy> ConstZero for Pair<T> {
    const ZERO: Self = Pair(T::ZERO, T::ZERO);
}
#[derive(Debug)]
struct Unit(i32);
impl ConstZero for Unit {
    const ZERO: Self = Unit(i32::ZERO + 1);
}

fn zero<T: ConstZero>() -> T {
    T::ZERO
}

trait SeedableRng: Sized {
    type Seed: Default + AsMut<[u8]>;
    fn from_seed(seed: Self::Seed) -> Self;
    fn seed_from_u32(mut state: u32) -> Self {
        let mut seed = Self::Seed::default();
        for b in seed.as_mut().iter_mut() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            *b = (state >> 24) as u8;
        }
        Self::from_seed(seed)
    }
}
struct Four {
    s: [u8; 4],
}
impl SeedableRng for Four {
    type Seed = [u8; 4];
    fn from_seed(seed: [u8; 4]) -> Four {
        Four { s: seed }
    }
}
struct Two {
    s: Vec<u8>,
}
impl SeedableRng for Two {
    type Seed = [u8; 2];
    fn from_seed(seed: [u8; 2]) -> Two {
        Two { s: seed.to_vec() }
    }
}

fn main() {
    let mut o = Vec::new();
    vec![1u8, 2].encode(&mut o);
    let w: Vec<u16> = vec![3, 4];
    w.encode(&mut o);
    println!("{:?}", o);
    println!("{} {} {}", <Vec<u16>>::label(), <Vec<u8> as Codec>::label(), Vec::<Vec<u8>>::label());
    println!("{:?} {:?}", encode_all(&vec![5u16]), encode_all(&vec![vec![6u8]]));

    let w: Wrapping<f64> = ConstZero::ZERO;
    let p: Pair<f64> = zero();
    let q = Pair::<i32>::ZERO;
    println!("{} {} {:?} {:?}", w.0, <Wrapping<i32> as ConstZero>::ZERO.0, p, q);
    let u: Unit = ConstZero::ZERO;
    println!("{:?} {:?} {} {}", u, <Unit as ConstZero>::ZERO, i32::ZERO, zero::<Wrapping<f64>>().0);

    println!("{:?} {:?}", Four::seed_from_u32(7).s, Two::seed_from_u32(9).s);
}
