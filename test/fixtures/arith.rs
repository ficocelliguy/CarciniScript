// Integer semantics that differ from naive JS: wrapping, truncating division, casts.
fn add_i32(a: i32, b: i32) -> i32 {
    a + b
}

fn mul_i32(a: i32, b: i32) -> i32 {
    a * b
}

fn sub_u32(a: u32, b: u32) -> u32 {
    a - b
}

fn add_u8(a: u8, b: u8) -> u8 {
    a + b
}

fn div(a: i32, b: i32) -> i32 {
    a / b
}

fn rem(a: i32, b: i32) -> i32 {
    a % b
}

fn main() {
    println!("{}", add_i32(i32::MAX, 1));
    println!("{}", mul_i32(65536, 65536));
    println!("{}", mul_i32(123456789, 987654321));
    println!("{}", sub_u32(0, 1));
    println!("{}", add_u8(250, 10));
    println!("{} {} {}", div(7, 2), div(-7, 2), rem(-7, 2));
    let x = 7.0 / 2.0;
    println!("{}", x);
    let big: i64 = 3_000_000_000;
    println!("{}", big * 2);
    println!("{}", -1i32 as u32);
    println!("{}", 300i32 as u8);
    println!("{}", -1.5f64 as u8);
    println!("{}", 1e10 as i32);
    println!("{}", 3.99 as i32);
    println!("{}", (255u8 as i8) as i32);
    let shifted: u32 = 1 << 31;
    println!("{} {}", shifted, shifted >> 4);
    let neg: i32 = -16;
    println!("{} {}", neg >> 2, (neg as u32) >> 28);
    println!("{} {} {}", 6 & 3, 6 | 3, 6 ^ 3);
    println!("{}", !0u8);
    println!("{}", 2i32.pow(10));
    println!("{}", 3i32.pow(21));
    println!("{}", (-7i32).rem_euclid(3));
    println!("{}", 2.5f64.round());
    println!("{}", (-2.5f64).round());
    let mut total = 0;
    let step: u64 = 1_000_000_000;
    for _ in 0..5 {
        total += step;
    }
    println!("{}", total);
    println!("{}", 'A' as u32);
    println!("{}", 97u8 as char);
    println!("{}", true as i32);
}
