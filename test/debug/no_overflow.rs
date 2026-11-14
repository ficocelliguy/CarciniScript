// Debug-build overflow checks (`setOverflowChecks(true)`): arithmetic that stays in range runs as
// in a release build, across the integer kinds, compound assignments and shifts.
fn main() {
    let mut total: u8 = 0;
    for i in 0..20u8 {
        total += i;
    }
    let a: i32 = -7;
    let b: i64 = 4_000_000_000;
    let c: u64 = 18_000_000_000_000_000_000;
    let d: i128 = -170_000_000_000_000_000_000_000_000_000_000_000_000;
    println!("{} {} {} {} {}", total, a / 2, a % 3, -a, b * 2);
    println!("{} {} {}", c / 1000 + c / 9, d / 7, (d + 1) * -1 / 100);
    println!("{} {} {}", 1u64 << 40, -128i8 / 2, i16::MAX - 1);
}
