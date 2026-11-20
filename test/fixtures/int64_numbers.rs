// `i64` / `u64` are JS numbers like `usize` / `isize`: exact to 2^53, where arithmetic, the
// integer methods, casts both ways, parsing, formatting, patterns, map keys, sorting and ranges
// agree with rustc; the speed of a plain number is the point (hashes that need all 64 bits are
// `u128`'s job: `int128_methods.rs`).
use std::collections::{HashMap, HashSet};

fn lcg(state: &mut u64) -> u64 {
    // (Small constants keep every intermediate under 2^53.)
    *state = (*state * 48271 + 11) % 2147483647;
    *state
}

fn classify(x: u64) -> &'static str {
    match x {
        0 => "zero",
        1..=9 => "digit",
        10 | 100 | 1000 => "round",
        x if x > 1 << 50 => "huge",
        _ => "other",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
struct Stamp {
    secs: u64,
    delta: i64,
}

fn main() {
    let mut s = 42u64;
    println!("{:?}", (0..3).map(|_| lcg(&mut s)).collect::<Vec<_>>());
    let money: u64 = 1_250_000_000_000;
    let threads: i64 = 1 << 40;
    println!("{} {} {} {}", money * 7, money / 3, money % 1000, threads - 1);
    println!("{:?} {} {}", money.checked_add(1), money.checked_mul(10).is_some(), money.saturating_sub(money + 1));
    println!("{} {} {}", 3u64.wrapping_add(4), threads.wrapping_mul(3), (-5i64).wrapping_sub(7));
    println!("{:?} {:?}", 7i64.checked_rem(0), 7i64.checked_div(2));
    println!("{} {}", 2i64.pow(40), 10u64.pow(15));

    // methods
    let v: i64 = -1234567890123;
    println!("{} {} {} {}", v.abs(), v.signum(), v.pow(1), (-3i64).pow(3));
    println!("{} {} {}", v.min(5), v.max(5), v.clamp(-10, 10));
    println!("{} {} {}", v.rem_euclid(7), v.div_euclid(7), v % 7);
    println!("{} {}", v.abs_diff(1000), 5u64.abs_diff(9));
    println!("{} {} {} {}", 255u64.count_ones(), 12u64.count_zeros(), 1u64.leading_zeros(), 8i64.trailing_zeros());
    println!("{} {} {}", (1u64 << 50).is_power_of_two(), 6u64.is_power_of_two(), 1_000_000_000_000u64.isqrt());
    println!("{} {} {}", (1u64 << 50).ilog2(), 12345678901234u64.ilog10(), v.is_negative());

    // shifts and bits (exact through BigInt past 32 bits)
    let a: u64 = 0xF0F0_F0F0_F0F0;
    println!("{} {} {} {}", a >> 4, a << 4, a & 0xFF, a | 1);
    println!("{} {} {}", a ^ (a >> 8), (-8i64) >> 1, (1i64 << 50) << 1);
    println!("{} {}", !5i64, (1u64 << 52) - 1);

    // casts
    let big = 1u64 << 50;
    println!("{} {} {}", big as f64, big as u32, big as i32);
    println!("{} {} {}", 3.9f64 as u64, -3.9f64 as i64, 1e15 as i64);
    println!("{} {} {}", 300i32 as u64, -1i32 as i64, big as usize);
    println!("{} {}", u64::from(7u8) + 1, i64::from(-5i32) * 2);
    let w: u64 = 9u32.into();
    println!("{} {}", w, (w as u8) as char);
    println!("{}", 1.5f64.to_bits() == 4609434218613702656);

    // parsing and formatting
    println!("{:?} {:?}", "9007199254740991".parse::<u64>(), "-12".parse::<i64>());
    println!("{:?} {:?}", "x".parse::<u64>().is_err(), u64::from_str_radix("ffffffffff", 16));
    println!("[{:>18}] [{:<5}] [{:+}] [{:#b}] [{:o}] [{:x}]", big, 7u64, 5i64, 5u64, 64u64, 1u64 << 40);
    println!("{} {}", big.to_string().len(), format!("{:?}", Some(-(big as i64))));

    // patterns and comparisons
    for x in [0u64, 7, 100, 55, 1 << 51] {
        print!("{} ", classify(x));
    }
    println!();
    println!("{} {} {:?}", big > 1 << 49, big == 1125899906842624, big.cmp(&3));
    println!("{} {}", std::cmp::max(3u64, 9), std::cmp::min(-3i64, 9));

    // collections and iterators
    let mut m: HashMap<u64, &str> = HashMap::new();
    m.insert(1 << 50, "big");
    m.insert(3, "small");
    println!("{:?} {:?}", m.get(&(1 << 50)), m.get(&4));
    let pairs: HashSet<(u64, i64)> = [(1, -1), (1, -1), (2, -2)].into_iter().collect();
    println!("{}", pairs.len());
    let mut vals = vec![5i64, -3, 1 << 52, 0, -(1 << 52)];
    vals.sort();
    println!("{:?}", vals);
    vals.sort_by(|a, b| b.cmp(a));
    println!("{:?} {:?} {:?}", vals.iter().max(), vals.iter().min(), vals.first());
    let total: u64 = (1..=18u64).product();
    let sum: i64 = (1..=100i64).sum();
    println!("{} {} {:?}", total, sum, (0..10u64).step_by(3).collect::<Vec<_>>());
    println!("{:?}", (0..5u64).rev().map(|x| x * x).collect::<Vec<_>>());
    let mut n = 0u64;
    for i in 0..1_000_000u64 {
        n += i & 1;
    }
    println!("{}", n);

    let mut stamps = vec![Stamp { secs: 1 << 40, delta: -2 }, Stamp::default(), Stamp { secs: 5, delta: 9 }];
    stamps.sort();
    println!("{:?}", stamps);
    let seen: HashSet<Stamp> = stamps.iter().copied().collect();
    println!("{} {:?}", seen.len(), stamps.iter().map(|s| s.secs).max());
    let opt: Option<u64> = Some(0);
    println!("{:?} {}", opt.map(|x| x + 1), opt.unwrap_or(9));
}
