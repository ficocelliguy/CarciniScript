// One trait implemented for several instantiations of a type (`W<u8>` / `W<u16>`, `Cell<u8>` /
// `Cell<u16>`, macro-stamped `UniformInt<$ty>`, `Wrapping<u32>` / `Wrapping<u64>`): each impl's
// methods, defaults, associated functions and consts, picked by the type arguments, also through
// a type parameter. And impls for types that are one type in JS (`[T]` / `[T; N]`, `X` / `&X`,
// `&T` / `&mut T`, `dyn Tr` / `dyn Tr + Send`, `Cow<str>` / `Cow<[T]>` of a marker trait) that do
// the same thing or hand calls on to one another.
use std::borrow::Cow;
use std::cell::Cell;
use std::num::Wrapping;

pub struct W<X>(pub X);
pub trait Get {
    const TAG: u32;
    fn get(&self) -> u32;
    fn make(v: u32) -> Self;
    fn twice(&self) -> u32 {
        self.get() * 2
    }
    fn tagged(&self) -> u32 {
        self.get() * 100 + Self::TAG
    }
}
impl Get for W<u8> {
    const TAG: u32 = 8;
    fn get(&self) -> u32 {
        self.0 as u32
    }
    fn make(v: u32) -> Self {
        W(v as u8)
    }
}
impl Get for W<u16> {
    const TAG: u32 = 16;
    fn get(&self) -> u32 {
        self.0 as u32 + 1000
    }
    fn make(v: u32) -> Self {
        W(v as u16)
    }
}

fn through<T: Get>(t: &T) -> u32 {
    t.get() + T::TAG
}
fn build<T: Get>(v: u32) -> T {
    T::make(v)
}

pub trait BitStore {
    const BITS: u32;
    fn load(&self) -> u32;
    fn bump(&self);
    fn both(&self) -> u32 {
        self.bump();
        self.load() + Self::BITS
    }
}
impl BitStore for Cell<u8> {
    const BITS: u32 = 8;
    fn load(&self) -> u32 {
        self.get() as u32
    }
    fn bump(&self) {
        self.set(self.get().wrapping_add(1));
    }
}
impl BitStore for Cell<u16> {
    const BITS: u32 = 16;
    fn load(&self) -> u32 {
        self.get() as u32 + 70000
    }
    fn bump(&self) {
        self.set(self.get().wrapping_add(2));
    }
}
fn total<S: BitStore>(s: &S) -> u32 {
    s.bump();
    s.load() * 10 + S::BITS
}

pub struct UniformInt<X> {
    low: X,
    range: X,
}
pub trait UniformSampler {
    type X;
    fn new(low: Self::X, high: Self::X) -> Self;
    fn sample(&self) -> Self::X;
}
macro_rules! uniform_int_impl {
    ($ty:ty) => {
        impl UniformSampler for UniformInt<$ty> {
            type X = $ty;
            fn new(low: $ty, high: $ty) -> Self {
                UniformInt { low, range: high - low }
            }
            fn sample(&self) -> $ty {
                self.low + self.range / 2
            }
        }
    };
}
uniform_int_impl!(i8);
uniform_int_impl!(u32);

pub trait Word: Copy {
    const ZERO: Self;
    type InputBytes;
    fn from_be_bytes(input: Self::InputBytes) -> Self;
    fn rotr(self, count: u32) -> Self;
}
impl Word for Wrapping<u32> {
    const ZERO: Self = Self(0);
    type InputBytes = [u8; 4];
    fn from_be_bytes(input: Self::InputBytes) -> Self {
        Self(u32::from_be_bytes(input))
    }
    fn rotr(self, count: u32) -> Self {
        Self(self.0.rotate_right(count))
    }
}
impl Word for Wrapping<u64> {
    const ZERO: Self = Self(0);
    type InputBytes = [u8; 8];
    fn from_be_bytes(input: Self::InputBytes) -> Self {
        Self(u64::from_be_bytes(input))
    }
    fn rotr(self, count: u32) -> Self {
        Self(self.0.rotate_right(count))
    }
}

// The same impl for a slice and an array, written by a macro (whose bindings are renamed).
pub trait Count {
    fn count(&self) -> usize;
}
impl Count for u32 {
    fn count(&self) -> usize {
        *self as usize
    }
}
macro_rules! count_impls {
    ($t:ty) => {
        impl<const N: usize> Count for [$t; N] {
            fn count(&self) -> usize {
                let mut n = 0;
                for v in &self[..] {
                    n += v.count();
                }
                n
            }
        }
        impl Count for [$t] {
            fn count(&self) -> usize {
                let mut n = 0;
                for v in self {
                    n += v.count();
                }
                n
            }
        }
    };
}
count_impls!(u32);

// An array's impls handing each call on to its slice's.
pub trait Fill {
    fn fill_from(&mut self, x: u8);
}
impl Fill for [u8] {
    fn fill_from(&mut self, x: u8) {
        for (i, v) in self.iter_mut().enumerate() {
            *v = x + i as u8;
        }
    }
}
impl<T> Fill for [T; 3]
where
    [T]: Fill,
{
    fn fill_from(&mut self, x: u8) {
        self[..].fill_from(x)
    }
}
impl<T> Fill for [T; 4]
where
    [T]: Fill,
{
    fn fill_from(&mut self, x: u8) {
        self[..].fill_from(x)
    }
}

// `X` and `&X`, one handing a method on to the other.
pub struct Pipe(Vec<u8>);
pub trait Sink {
    fn put(&mut self, b: u8) -> usize;
    fn flush(&mut self) -> usize;
}
impl Sink for Pipe {
    fn put(&mut self, b: u8) -> usize {
        b as usize + self.0.len()
    }
    fn flush(&mut self) -> usize {
        <&Pipe as Sink>::flush(&mut &*self)
    }
}
impl Sink for &Pipe {
    fn put(&mut self, b: u8) -> usize {
        b as usize + self.0.len()
    }
    fn flush(&mut self) -> usize {
        self.0.len() * 10
    }
}

// `&T` and `&mut T`, alike.
pub trait Addr {
    fn tag(self) -> String;
}
impl<T: std::fmt::Debug> Addr for &T {
    fn tag(self) -> String {
        format!("at {:?}", self)
    }
}
impl<T: std::fmt::Debug> Addr for &mut T {
    fn tag(self) -> String {
        format!("at {:?}", self)
    }
}

// A marker trait for types that are one type in JS.
pub trait Marker {}
impl<'a> Marker for Cow<'a, str> {}
impl<'a, T: Clone> Marker for Cow<'a, [T]> {}
fn marked<M: Marker + ?Sized>(_m: &M) -> &'static str {
    "marked"
}

// Inherent impls for `dyn Tr` and `dyn Tr + Send`, alike.
pub trait Subscriber {
    fn id(&self) -> u32;
}
impl dyn Subscriber {
    pub fn is_one(&self) -> bool {
        self.id() == 1
    }
}
impl dyn Subscriber + Send {
    pub fn is_one(&self) -> bool {
        self.id() == 1
    }
}
struct One;
impl Subscriber for One {
    fn id(&self) -> u32 {
        1
    }
}

fn main() {
    let a = W(5u8);
    let b = W(7u16);
    println!("{} {} {} {}", a.get(), b.get(), a.twice(), b.twice());
    println!("{} {}", through(&W(1u8)), through(&W(2u16)));
    let c: W<u8> = build(300);
    let d: W<u16> = build(300);
    println!("{} {} {} {}", c.0, d.0, c.tagged(), d.tagged());
    println!("{} {}", W::<u8>::TAG, <W<u16> as Get>::TAG);

    let x = Cell::new(255u8);
    let y = Cell::new(3u16);
    println!("{} {}", x.load(), y.load());
    println!("{} {}", total(&x), total(&y));
    println!("{} {}", x.both(), y.both());
    println!("{} {}", <Cell<u8> as BitStore>::BITS, <Cell<u16>>::BITS);

    let u = UniformInt::<i8>::new(-10, 10);
    let v: UniformInt<u32> = UniformSampler::new(100, 200);
    let w = <UniformInt<u32> as UniformSampler>::new(1, 5);
    println!("{} {} {}", u.sample(), v.sample(), w.sample());

    let p = Wrapping::<u32>::from_be_bytes([0, 0, 1, 0]);
    let q = <Wrapping<u64> as Word>::from_be_bytes([0, 0, 0, 0, 0, 0, 1, 0]);
    println!("{} {}", p.rotr(4), q.rotr(4));
    println!("{} {}", <Wrapping<u32> as Word>::ZERO, <Wrapping<u64> as Word>::ZERO);

    let arr = [1u32, 2, 3];
    let v = vec![4u32, 5];
    println!("{} {}", arr.count(), v[..].count());

    let mut three = [0u8; 3];
    let mut four = [0u8; 4];
    three.fill_from(10);
    four.fill_from(20);
    println!("{:?} {:?}", three, four);

    let mut pipe = Pipe(vec![1, 2]);
    let r = &pipe;
    let mut rr = r;
    println!("{} {}", rr.put(3), rr.flush());
    println!("{} {}", pipe.put(4), pipe.flush());

    let mut n = 7u8;
    println!("{} {}", (&n).tag(), (&mut n).tag());

    let s: Cow<str> = Cow::Borrowed("x");
    println!("{}", marked(&s));

    let one: Box<dyn Subscriber> = Box::new(One);
    let sent: Box<dyn Subscriber + Send> = Box::new(One);
    println!("{} {}", one.is_one(), sent.is_one());
}
