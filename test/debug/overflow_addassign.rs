// Debug-build overflow checks (`setOverflowChecks(true)`): this one panics like `rustc` without `-O`.
fn main() {
    let a: i32 = std::hint::black_box(2_000_000_000);
    let b: u8 = std::hint::black_box(200);
    let c: i64 = std::hint::black_box(9_000_000_000_000_000_000);
    let d: u32 = std::hint::black_box(3);
    println!("{} {} {} {}", a / 2 + 5, b - 100, c / 3, d * 7);
    println!("{} {}", (a as i64) * 4, (b as u16) + 300);
    let mut n: u8 = 250;
    for _ in 0..5 {
        n += 1;
    }
    println!("{}", n);
    let shift: u32 = std::hint::black_box(31);
    println!("{}", 1u32 << shift);

    n += 10; println!("{}", n);
}
