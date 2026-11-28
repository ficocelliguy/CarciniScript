// Bounds on operator projections: `where <T as Shr<u32>>::Output: PartialEq<T>`,
// `where &'a T: Add<T>` with `-> <&'a T as Add<T>>::Output`, `where u8: TryFrom<<T as
// BitAnd>::Output>`, in free functions and in methods of a generic impl.
use std::ops::{Add, BitAnd, Mul, Shr, Sub};

fn halves<T>(x: T, y: T) -> bool
where
    T: Shr<u32> + Copy,
    <T as Shr<u32>>::Output: PartialEq<T>,
{
    (x >> 1) == y
}

fn add_ref<'a, T>(a: &'a T, b: T) -> <&'a T as Add<T>>::Output
where
    &'a T: Add<T>,
{
    a + b
}

fn sum_refs<T>(xs: &[T], zero: T) -> T
where
    T: Copy,
    for<'x> &'x T: Add<T, Output = T>,
{
    let mut acc = zero;
    for x in xs {
        acc = x + acc;
    }
    acc
}

fn low_byte<T: BitAnd<Output = T> + Copy>(x: T, mask: T) -> u8
where
    u8: TryFrom<<T as BitAnd>::Output>,
{
    u8::try_from(x & mask).ok().unwrap_or(255)
}

fn ordered_diff<T>(a: T, b: T) -> bool
where
    T: Sub + Copy,
    <T as Sub>::Output: PartialOrd<T>,
{
    (a - b) < a
}

struct Scale<T> {
    k: T,
}

impl<T: Copy> Scale<T> {
    fn apply<'a>(&'a self, x: T) -> <&'a T as Mul<T>>::Output
    where
        &'a T: Mul<T>,
    {
        &self.k * x
    }
}

fn main() {
    println!("{} {} {}", halves(8u32, 4), halves(9i64, 5), halves(-6i32, -3));
    println!("{} {}", add_ref(&3i32, 4), add_ref(&1.5f64, 2.25));
    println!("{} {}", sum_refs(&[1u64, 2, 3], 10), sum_refs(&[0.5f32, 0.25], 0.0));
    println!("{} {} {}", low_byte(0x1234u32, 0xff), low_byte(300i32, 0xfff), low_byte(-1i64, 0x7f));
    println!("{} {}", ordered_diff(10i32, 3), ordered_diff(2.0f64, -1.0));
    let s = Scale { k: 3i32 };
    println!("{} {}", s.apply(7), Scale { k: 0.5f64 }.apply(5.0));
}
