// Exhaustiveness over integer ranges: literals and ranges (`1..=127`, `1..`, `i32::MIN..=-1`,
// `128..=u8::MAX`) that cover the whole type need no `_` arm, also inside tuples.
fn class(b: u8) -> &'static str {
    match b {
        0 => "zero",
        1..=127 => "low",
        128..=u8::MAX => "high",
    }
}

fn sign(x: i32) -> i32 {
    match x {
        i32::MIN..=-1 => -1,
        0 => 0,
        1.. => 1,
    }
}

fn bucket(n: u32, flag: bool) -> u32 {
    match (n, flag) {
        (0..=9, true) => 1,
        (0..=9, false) => 2,
        (10.., _) => 3,
    }
}

fn main() {
    println!("{} {} {}", class(0), class(5), class(200));
    println!("{} {} {}", sign(-7), sign(0), sign(9));
    println!("{} {} {}", bucket(3, true), bucket(3, false), bucket(30, true));
}
