// `i128` / `u128` as BigInts: literals beyond 2^64, arithmetic that wraps at 128 bits, widening
// multiplies from `u64`, division and remainder, shifts and bit operations, `!`, casts both ways,
// `pow`, `abs`, `max`, `sum`/`product`, `parse`, comparisons, and formatting (`{}`, `{:x}`, `{:>40}`).
fn widening_mul(a: u64, b: u64) -> (u64, u64) {
    let p = a as u128 * b as u128;
    ((p >> 64) as u64, p as u64)
}

fn factorial(n: u32) -> u128 {
    (1..=n as u128).product()
}

fn main() {
    let big: u128 = 340_282_366_920_938_463_463_374_607_431_768_211_455;
    let x: i128 = -170_141_183_460_469_231_731_687_303_715_884_105_728;
    println!("{} {}", big, x);
    println!("{} {}", big / 7, x / 3);
    let (hi, lo) = widening_mul(1 << 40, 1 << 40);
    println!("{hi} {lo}");
    println!("{} {}", factorial(30), factorial(25) % 1_000_000_007);
    let a: i128 = 12345678901234567890123;
    let b: i128 = -98765432109876543210;
    println!("{} {} {} {}", a + b, a - b, a * 3, a % 97);
    println!("{} {} {}", a >> 10, (a << 3) & 0xffff, !b);
    println!("{} {} {}", b.abs(), 2i128.pow(100), a.max(b));
    println!("{:x} {:#x} {:>40}|", big, 255u128, a);
    println!("{:?} {:?}", "123456789012345678901234567890".parse::<u128>(), "-1".parse::<u128>().is_err());
    let v: Vec<i128> = vec![1, 2, 3];
    let s: i128 = v.iter().sum();
    println!("{} {} {}", s, a > b, (a as f64) > 1e22);
    let small = a as u8;
    let neg = b as i32;
    println!("{small} {neg} {}", (big as i128));
}
