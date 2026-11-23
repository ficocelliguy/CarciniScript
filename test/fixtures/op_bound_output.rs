// `L::Output` of a std operator bound with a right-hand side (`L: Add<R>`), in a function and
// in an impl, and `T::Output` of a plain `T: Add`.
use std::ops::{Add, Mul};

#[derive(Debug, Clone, Copy)]
struct Complex<T> {
    re: T,
    im: T,
}

impl<L, R> Add<Complex<R>> for Complex<L>
where
    L: Add<R>,
{
    type Output = Complex<L::Output>;
    fn add(self, rhs: Complex<R>) -> Self::Output {
        Complex {
            re: self.re + rhs.re,
            im: self.im + rhs.im,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct V2(f64, f64);

impl Mul<f64> for V2 {
    type Output = V2;
    fn mul(self, k: f64) -> V2 {
        V2(self.0 * k, self.1 * k)
    }
}

fn add<L: Add<R>, R>(a: L, b: R) -> L::Output {
    a + b
}

fn scale<V: Mul<f64>>(v: V, k: f64) -> V::Output {
    v * k
}

fn twice<T: Add + Copy>(x: T) -> T::Output {
    x + x
}

fn main() {
    let c = Complex { re: 1.5, im: 2.0 } + Complex { re: 0.5, im: 1.0 };
    println!("{:?}", c);
    let n = Complex { re: 1, im: 2 } + Complex { re: 3, im: 4 };
    println!("{:?}", n);
    println!("{} {} {:?}", add(2, 3), add(1.5, 2.25), add(c, c));
    println!("{:?} {}", scale(V2(1.0, 2.5), 2.0), scale(3.0, 0.5));
    println!("{} {}", twice(21), twice(0.25));
}
