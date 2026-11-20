// Operators on type parameters: `/` and `%` (integer division truncates, floats don't), wrapping
// by the caller's type, user types' own operator impls, `+=`, unary `-`; `IntoIterator` bounds.
use std::fmt::Debug;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Rem, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Money(i64);

impl Add for Money {
    type Output = Money;
    fn add(self, o: Money) -> Money {
        Money(self.0 + o.0)
    }
}

impl Sub for Money {
    type Output = Money;
    fn sub(self, o: Money) -> Money {
        Money(self.0 - o.0)
    }
}

impl Div for Money {
    type Output = Money;
    fn div(self, o: Money) -> Money {
        Money(self.0 / o.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, o: Money) {
        self.0 += o.0;
    }
}

impl Neg for Money {
    type Output = Money;
    fn neg(self) -> Money {
        Money(-self.0)
    }
}

fn halve<T: Div<Output = T> + Copy>(x: T, two: T) -> T {
    x / two
}

fn split<T: Div<Output = T> + Rem<Output = T> + Copy>(x: T, d: T) -> (T, T) {
    (x / d, x % d)
}

fn sum_all<T: AddAssign + Copy + Default>(xs: &[T]) -> T {
    let mut t = T::default();
    for &x in xs {
        t += x;
    }
    t
}

fn mean<T: Copy + Add<Output = T> + Div<Output = T>>(a: T, b: T, two: T) -> T {
    (a + b) / two
}

fn poly<T: Copy + Mul<Output = T> + Sub<Output = T> + Neg<Output = T>>(x: T, y: T) -> T {
    -(x * x - y)
}

fn wrapped<T: Add<Output = T> + Mul<Output = T>>(a: T, b: T, c: T) -> T {
    a * b + c
}

fn gcd<T: Copy + PartialEq + Rem<Output = T> + Default>(a: T, b: T) -> T {
    if b == T::default() { a } else { gcd(b, a % b) }
}

fn total<I>(xs: I) -> i32
where
    I: IntoIterator<Item = i32>,
{
    xs.into_iter().sum()
}

fn show_all<I: IntoIterator>(xs: I) -> Vec<String>
where
    I::Item: Debug,
{
    xs.into_iter().map(|x| format!("{:?}", x)).collect()
}

fn main() {
    println!(
        "{} {} {} {}",
        halve(7, 2),
        halve(7.0, 2.0),
        halve(-7i8, 2),
        halve(9u64, 2)
    );
    println!(
        "{:?} {:?} {:?}",
        split(17, 5),
        split(-17, 5),
        split(7.5, 2.0)
    );
    println!("{:?} {:?}", split(17i64, 5), split(255u8, 10));
    println!(
        "{:?} {:?}",
        halve(Money(9), Money(2)),
        mean(Money(10), Money(20), Money(2))
    );
    println!(
        "{} {} {:?}",
        sum_all(&[1, 2, 3]),
        sum_all(&[0.5, 0.25]),
        sum_all(&[Money(1), Money(2)])
    );
    println!("{} {}", poly(3, 4), poly(1.5, 0.25));
    println!(
        "{} {} {}",
        wrapped(200u8, 2, 100),
        wrapped(100_000i32, 100_000, 1),
        wrapped(1i64 << 40, 2, 3)
    );
    println!("{} {}", mean(3, 4, 2), mean(3.0, 4.0, 2.0));
    println!("{} {} {}", gcd(48, 18), gcd(48u64, 180), gcd(17i8, 5));
    println!("{} {}", total(vec![1, 2, 3]), total((1..=4).map(|x| x * x)));
    println!(
        "{:?} {:?}",
        show_all(vec![Some(1), None]),
        show_all(["a", "b"])
    );
}
