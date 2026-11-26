// Operators on `Self` (in a trait's default methods) and on a `T: Tr` whose bound comes through
// a supertrait: `trait Float: PartialOrd + Mul<Output = Self>` gives `self < o`, `self * self`;
// `self / two` in the copy for `i32` truncates.
use std::ops::{Add, BitOr, Div, Mul};

trait Zero: Sized + Add<Output = Self> {
    const ZERO: Self;
}
impl Zero for f64 {
    const ZERO: f64 = 0.0;
}
trait Float: Zero + Copy + PartialOrd + Mul<Output = Self> + Div<Output = Self> {
    fn is_nan(self) -> bool {
        self != self
    }
    fn is_positive(self) -> bool {
        self > Self::ZERO
    }
    fn square(self) -> Self {
        self * self
    }
    fn half_sum(self, o: Self, two: Self) -> Self {
        (self + o) / two
    }
    fn smaller(self, o: Self) -> Self {
        if self < o { self } else { o }
    }
}
impl Float for f64 {}

trait Flags: Copy + BitOr<Output = Self> + PartialEq {
    fn union(self, o: Self) -> Self {
        self | o
    }
    fn same(self, o: Self) -> bool {
        self == o
    }
}
impl Flags for u8 {}

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
struct M(i32);
impl Add for M {
    type Output = M;
    fn add(self, o: M) -> M {
        M(self.0 + o.0)
    }
}
trait Monoid: Add<Output = Self> + Copy + PartialOrd {
    fn twice(self) -> Self {
        self + self
    }
    fn max_of(self, o: Self) -> Self {
        if self >= o { self } else { o }
    }
}
impl Monoid for M {}

fn bigger<T: Float>(a: T, b: T) -> T {
    if a > b { a } else { a * b }
}
fn sum3<T: Monoid>(a: T, b: T, c: T) -> T {
    a + b + c
}
fn div_int<T: Num>(a: T, b: T) -> T {
    a / b
}
trait Num: Copy + Div<Output = Self> {
    fn halve(self, two: Self) -> Self {
        self / two
    }
}
impl Num for i32 {}
fn main() {
    println!("{} {:?} {}", bigger(2.0, 3.0), sum3(M(1), M(2), M(3)), div_int(7, 2));
    println!("{}", 7.halve(2));
    println!("{} {} {}", 2.0f64.is_nan(), f64::NAN.is_nan(), 2.0f64.is_positive());
    println!("{} {} {}", 3.0f64.square(), 1.0f64.half_sum(2.0, 2.0), 4.0f64.smaller(3.0));
    println!("{} {} {}", 1u8.union(4), 3u8.same(3), 3u8.same(4));
    println!("{:?} {:?}", M(2).twice(), M(2).max_of(M(5)));
}
