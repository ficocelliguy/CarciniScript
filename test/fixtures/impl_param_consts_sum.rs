// Consts of generic impls that read the impl's type parameter (`const SIZE: usize = 8 * T::SIZE`
// in `impl<T: Schema> Schema for Vec<T>` / `Option<T>` / a struct's `W<T>`), read as
// `<W<u8>>::SIZE`, `W::<u32>::SIZE`, `<Vec<u8>>::SIZE`, `Vec::<u8>::SIZE` and through a type
// parameter; and `iter.sum::<S>()` of items of another type under `impl<A> Sum<A> for S where
// S: Add<A, Output = S>`.
use std::iter::Sum;
use std::ops::Add;

trait Schema {
    const SIZE: usize;
    fn name() -> String;
}

impl Schema for u8 {
    const SIZE: usize = 1;
    fn name() -> String {
        "u8".into()
    }
}

impl Schema for u32 {
    const SIZE: usize = 4;
    fn name() -> String {
        "u32".into()
    }
}

struct W<T>(T);

impl<T: Schema> Schema for W<T> {
    const SIZE: usize = 2 + T::SIZE;
    fn name() -> String {
        format!("W<{}>", T::name())
    }
}

impl<T: Schema> Schema for Vec<T> {
    const SIZE: usize = 8 * T::SIZE;
    fn name() -> String {
        format!("Vec<{}>", T::name())
    }
}

impl<T: Schema> Schema for Option<T> {
    const SIZE: usize = 1 + T::SIZE;
    fn name() -> String {
        format!("Option<{}>", T::name())
    }
}

fn size_of<S: Schema>() -> usize {
    S::SIZE
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Size(u32);

impl Add<u32> for Size {
    type Output = Size;
    fn add(self, r: u32) -> Size {
        Size(self.0 + r)
    }
}

impl Add for Size {
    type Output = Size;
    fn add(self, r: Size) -> Size {
        Size(self.0 + r.0)
    }
}

impl<A> Sum<A> for Size
where
    Size: Add<A, Output = Size>,
{
    fn sum<I: Iterator<Item = A>>(iter: I) -> Size {
        iter.fold(Size(0), |acc, x| acc + x)
    }
}

fn main() {
    println!("{} {} {}", <W<u8>>::SIZE, W::<u32>::SIZE, <W<u32> as Schema>::SIZE);
    println!("{} {} {}", <Vec<u8>>::SIZE, Vec::<u32>::SIZE, <Option<u32>>::SIZE);
    println!("{} {}", <Option<Vec<u32>>>::SIZE, <Option<Vec<u32>>>::name());
    println!(
        "{} {} {} {}",
        size_of::<W<u32>>(),
        size_of::<Option<u8>>(),
        size_of::<Vec<W<u8>>>(),
        size_of::<u8>()
    );

    let a: Size = vec![1u32, 2, 3].into_iter().sum();
    let b = [Size(4), Size(5)].iter().copied().sum::<Size>();
    let c = (1..=4u32).sum::<Size>();
    println!("{:?} {:?} {:?}", a, b, c);
}
