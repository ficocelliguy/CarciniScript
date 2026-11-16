// The bundled `num-traits`: `Zero` / `One` (on concrete types and as bounds), `Signed`,
// `Bounded`, `ToPrimitive` / `FromPrimitive`, `pow`, `clamp`, `abs`.
use num_traits::{Bounded, FromPrimitive, One, Signed, ToPrimitive, Zero};

fn sum_all<T: Zero + Copy + std::ops::Add<Output = T>>(xs: &[T]) -> T {
    let mut total = T::zero();
    for &x in xs {
        total = total + x;
    }
    total
}

fn product<T: One + Copy + std::ops::Mul<Output = T>>(xs: &[T]) -> T {
    xs.iter().fold(T::one(), |a, &b| a * b)
}

fn top<T: Bounded>() -> T {
    T::max_value()
}

fn bottom<T: Bounded>() -> T {
    T::min_value()
}

fn main() {
    println!("{} {}", sum_all(&[1, 2, 3]), sum_all(&[0.5, 0.25]));
    println!("{} {}", product(&[2u64, 3, 7]), product(&[1.5f64, 2.0]));
    println!("{} {} {}", 0i32.is_zero(), 5u8.is_zero(), 1.0f64.is_one());
    println!("{} {}", i32::zero(), f64::one());
    println!("{} {} {}", (-7i64).is_negative(), (-3i32).signum(), (-2.5f64).is_negative());
    println!("{} {}", top::<u8>(), bottom::<i16>());
    println!("{:?} {:?} {:?}", 300i32.to_u8(), (-1i64).to_u32(), 2.5f64.to_i64());
    println!("{:?} {:?} {:?}", u8::from_i64(255), u8::from_i64(256), i32::from_f64(3.9));
    println!("{:?}", 42u32.to_f64());
    println!("{} {}", num_traits::pow(3u64, 4), num_traits::pow(1.5f64, 2));
    println!("{} {}", num_traits::clamp(15, 0, 10), num_traits::clamp(-0.5, 0.0, 1.0));
    println!("{}", num_traits::abs(-9i32));
    println!("{} {}", num_traits::zero::<i64>(), num_traits::one::<f32>());
}
