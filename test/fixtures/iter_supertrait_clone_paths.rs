// A trait of the file whose supertraits are iterator traits (`trait Sub: Iterator`, `trait
// PreciseIterator: Iterator + ExactSizeIterator {}`) as a bound or `dyn`: its values are
// iterators (`t.next()`, `it.len()`), and a blanket `impl<I: ExactSizeIterator> .. for I` gives it
// to std's. `T::clone(&x)` through `Deref`, and `Bx::clone(&b)` / `<Bx<P>>::clone(&b)` /
// `Bx::<T>::clone(x)` of a derived `Clone`.
use std::fmt;
use std::ops::Deref;

trait Sub: Iterator {}
impl<I: Iterator> Sub for I {}

trait PreciseIterator: Iterator + ExactSizeIterator {}
impl<I: ExactSizeIterator> PreciseIterator for I {}

trait Summed: Iterator<Item = u32> {
    fn summed(self) -> u32
    where
        Self: Sized,
    {
        self.sum()
    }
}
impl<I: Iterator<Item = u32>> Summed for I {}

fn first<T: Sub<Item = u8>>(mut t: T) -> Option<u8> {
    t.next()
}

fn total(t: &mut dyn Sub<Item = u8>) -> u32 {
    let mut s = 0;
    while let Some(x) = t.next() {
        s += x as u32;
    }
    s
}

fn count<T: PreciseIterator>(it: &T) -> usize {
    it.len()
}

fn count_dyn(it: &dyn PreciseIterator<Item = u8>) -> usize {
    it.len()
}

fn sum_all<T: Summed>(t: T) -> u32 {
    t.summed()
}

trait Named: fmt::Display {
    fn tag(&self) -> String {
        format!("<{}>", self)
    }
}

struct N(u8);

impl fmt::Display for N {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "n{}", self.0)
    }
}

impl Named for N {}

fn show(n: &dyn Named) -> String {
    format!("{} {}", n, n.tag())
}

#[derive(Debug, Clone)]
struct Bx<T>(T, u8);

impl<T> Deref for Bx<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

#[derive(Debug, Clone)]
struct P(u8);

fn inner<T: Clone>(x: Bx<T>) -> T {
    T::clone(&x)
}

fn inner_ref<T: Clone>(x: &Bx<T>) -> T {
    T::clone(x)
}

fn dup<T: Clone>(x: &Bx<T>) -> Bx<T> {
    Bx::<T>::clone(x)
}

fn main() {
    let v = vec![1u8, 2, 3];
    println!("{:?}", first(v.iter().copied()));
    println!("{}", total(&mut v.clone().into_iter()));
    println!("{} {}", count(&v.iter()), count_dyn(&v.clone().into_iter()));
    println!("{}", sum_all(vec![4u32, 5, 6].into_iter()));
    println!("{}", show(&N(4)));

    println!("{:?}", inner(Bx(vec![1], 9)));
    println!("{:?}", inner_ref(&Bx(P(2), 9)));
    let b = Bx(P(3), 1);
    let c = Bx::clone(&b);
    let d = <Bx<P>>::clone(&b);
    println!("{:?} {:?} {:?}", c, d, dup(&b));
}
