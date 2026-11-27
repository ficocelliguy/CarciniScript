// `f64::classify` / `f32::classify` and `std::num::FpCategory`.
use std::num::FpCategory;

fn describe(x: f64) -> &'static str {
    match x.classify() {
        FpCategory::Nan => "nan",
        FpCategory::Infinite => "inf",
        FpCategory::Zero => "zero",
        FpCategory::Subnormal => "sub",
        FpCategory::Normal => "normal",
    }
}

fn main() {
    for x in [1.5, 0.0, -0.0, f64::NAN, f64::INFINITY, -f64::INFINITY, 1e-310, f64::MIN_POSITIVE] {
        println!("{} {}", describe(x), x.classify() == FpCategory::Normal);
    }
    let small: f32 = 1e-40;
    println!("{:?} {:?} {:?}", small.classify(), 1e-30f32.classify(), f32::NAN.classify());
    println!("{:?}", f64::classify(2.0));
    let c = 3.0f64.classify();
    let d = c;
    println!("{:?} {}", d, matches!(c, FpCategory::Normal | FpCategory::Subnormal));
}
