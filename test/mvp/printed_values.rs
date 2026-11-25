// Bit sets over u64, and a few float edge cases a stats printout runs into.
use std::f64::consts;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Flags(u64);

impl Flags {
    fn set(self, bit: u32) -> Flags {
        Flags(self.0 | 1u64 << bit)
    }
    fn has(self, bit: u32) -> bool {
        self.0 & (1 << bit) != 0
    }
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn main() {
    let big = 1u64 << 60;
    println!("{}", big);
    println!("{} {}", 1u64 << 63, (1u64 << 62) + (1 << 61));
    println!("{:?}", Flags(0).set(50).set(59));
    let f = Flags(0).set(55).set(40);
    println!("{} {} {}", f.0, f.has(55), f.has(41));
    println!("{:#x} {:b}", 1u64 << 60, 1u64 << 54);
    println!("{}", 1i64 << 58);
    println!("{}", -(1i64 << 57));
    println!("{}", u64::pow(2, 55));
    let mask: u64 = (1 << 56) | (1 << 40);
    println!("{mask} {:>22}|", mask);

    // Floats.
    println!("{:e} {:e} {:e}", f64::INFINITY, f64::NEG_INFINITY, f64::NAN);
    println!("{:.3} {:.1}", 1e21, 2.5e22);
    println!("{:.2} {:e} {:.0}", -0.0, -0.0, -0.0f64);
    println!("{}", f64::MAX);
    println!("{:?}", 1e300 * 10.0);
    let empty: [f64; 0] = [];
    println!("{} {}", mean(&empty), mean(&[1.0, 2.0, 4.0]));

    // The constants.
    println!("{} {} {}", f64::EPSILON, f64::MIN_POSITIVE, f64::MIN);
    println!("{} {}", f32::EPSILON, f32::MIN_POSITIVE);
    println!("{} {} {}", f32::MAX, f32::MIN, f64::MAX_EXP);
    println!("{} {} {}", f64::MIN_EXP, f64::DIGITS, f64::MANTISSA_DIGITS);
    println!("{} {} {}", f64::MAX_10_EXP, f64::MIN_10_EXP, f64::RADIX);
    println!("{} {} {}", f32::DIGITS, f32::MANTISSA_DIGITS, f32::MAX_EXP);
    println!("{} {} {} {}", consts::PI, consts::TAU, consts::E, consts::SQRT_2);
    println!("{} {} {}", consts::FRAC_PI_2, consts::FRAC_PI_3, consts::FRAC_PI_4);
    println!("{} {} {}", consts::FRAC_PI_6, consts::FRAC_PI_8, consts::FRAC_1_PI);
    println!("{} {} {}", consts::FRAC_2_PI, consts::FRAC_2_SQRT_PI, consts::FRAC_1_SQRT_2);
    println!("{} {} {}", consts::LN_2, consts::LN_10, consts::LOG2_E);
    println!("{} {} {}", consts::LOG10_E, consts::LOG2_10, consts::LOG10_2);
    println!("{} {}", std::f32::consts::PI, std::f32::consts::E);
    println!("{:.3e} {:e}", f64::MIN_POSITIVE, f64::EPSILON);
}
