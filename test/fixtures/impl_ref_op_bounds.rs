// Operator bounds on a reference to an impl's type parameter, in the impl's `where` clause:
// `for<'a, 'b> &'a I: BitAnd<&'b I, Output = I>` on a blanket impl (num-integer's `Average`)
// and an inherent one, and `where &'a T: Add` with the default right-hand side, used as an
// operator and as `.add(..)`.
use std::ops::{Add, BitAnd, BitOr, BitXor, Shr, Sub};

pub trait Average: Sized {
    fn average_ceil(&self, other: &Self) -> Self;
    fn average_floor(&self, other: &Self) -> Self;
}

impl<I> Average for I
where
    I: Add<Output = I> + Sub<Output = I> + Shr<usize, Output = I>,
    for<'a, 'b> &'a I:
        BitAnd<&'b I, Output = I> + BitOr<&'b I, Output = I> + BitXor<&'b I, Output = I>,
{
    fn average_floor(&self, other: &I) -> I {
        (self & other) + ((self ^ other) >> 1)
    }
    fn average_ceil(&self, other: &I) -> I {
        (self | other) - ((self ^ other) >> 1)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OF<T>(pub T);

impl<'a, T> Add<&'a T> for &'a OF<T>
where
    &'a T: Add,
{
    type Output = OF<<&'a T as Add>::Output>;
    fn add(self, other: &'a T) -> Self::Output {
        OF((self.0).add(other))
    }
}

impl<'a, T> Add<T> for &'a OF<T>
where
    &'a T: Add<T>,
{
    type Output = OF<<&'a T as Add<T>>::Output>;
    fn add(self, other: T) -> Self::Output {
        OF(&self.0 + other)
    }
}

struct W<I>(I);

impl<I> W<I>
where
    for<'a, 'b> &'a I: BitAnd<&'b I, Output = I>,
{
    fn masked(&self, o: &I) -> I {
        &self.0 & o
    }
}

fn main() {
    let (a, b) = (13u32, 6u32);
    println!("{} {}", a.average_floor(&b), a.average_ceil(&b));
    println!("{} {}", 200u8.average_floor(&255), (-7i64).average_ceil(&4));
    println!("{:?} {:?}", &OF(a) + &b, &OF(2.5f64) + 1.0);
    println!("{}", W(12u16).masked(&10));
}
