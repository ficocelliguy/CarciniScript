// `i64` / `u64` are exact over all 64 bits (BigInts): hashes and random number generators,
// wrapping / checked / saturating arithmetic at the edges, the integer methods, casts, parsing,
// formatting, patterns, collections and iterators.
use std::collections::{HashMap, HashSet};

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn classify(x: u64) -> &'static str {
    match x {
        0 => "zero",
        1..=9 => "digit",
        10 | 100 | 1000 => "round",
        x if x == u64::MAX => "max",
        _ => "other",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
struct Stamp {
    secs: u64,
    delta: i64,
}

fn main() {
    let mut s = 88172645463325252u64;
    let xs: Vec<u64> = (0..3).map(|_| xorshift(&mut s)).collect();
    println!("{:?}", xs);
    let mut sm = 42u64;
    println!("{} {}", splitmix(&mut sm), splitmix(&mut sm));
    println!("{:x} {:016X} {}", fnv1a("hello"), fnv1a(""), fnv1a("carcini") % 1000);

    // edges
    let max = u64::MAX;
    let imin = i64::MIN;
    println!("{} {} {} {}", max, imin, i64::MAX, max.wrapping_add(2));
    println!("{} {}", imin.wrapping_sub(1), (i64::MAX).wrapping_mul(3));
    println!("{:?} {:?} {:?}", max.checked_add(1), max.checked_sub(1), imin.checked_neg());
    println!("{:?} {:?} {:?}", imin.checked_div(-1), 7i64.checked_rem(0), imin.checked_abs());
    println!("{} {} {}", max.saturating_add(5), imin.saturating_sub(1), imin.saturating_abs());
    println!("{} {} {}", 3u64.wrapping_sub(5), imin.wrapping_abs(), imin.wrapping_neg());
    println!("{:?} {:?} {}", 3u64.checked_pow(40), 3u64.checked_pow(41), 3u64.wrapping_pow(41));
    println!("{} {}", 2i64.saturating_pow(70), (-2i64).saturating_pow(71));
    println!("{} {}", imin.wrapping_div(-1), imin.wrapping_rem(-1));

    // methods
    let v: i64 = -1234567890123;
    println!("{} {} {} {}", v.abs(), v.signum(), v.pow(1), (-3i64).pow(3));
    println!("{} {} {}", v.min(5), v.max(5), v.clamp(-10, 10));
    println!("{} {} {}", v.rem_euclid(7), v.div_euclid(7), v % 7);
    println!("{} {}", v.abs_diff(1000), 5u64.abs_diff(9));
    println!("{} {} {} {}", max.count_ones(), 12u64.count_zeros(), 1u64.leading_zeros(), 8i64.trailing_zeros());
    println!("{} {} {}", 0u64.leading_zeros(), (1u64 << 63).is_power_of_two(), 6u64.is_power_of_two());
    println!("{} {} {}", 1_000_000_000_000u64.isqrt(), max.ilog2(), 12345678901234u64.ilog10());
    println!("{} {}", v.is_negative(), v.is_positive());

    // shifts and bits
    let a: u64 = 0xF0F0_F0F0_F0F0_F0F0;
    println!("{} {} {} {}", a >> 4, a << 4, !a, a & 0xFF);
    println!("{} {} {}", (-8i64) >> 1, (1i64 << 62) << 1, -(i64::MAX));

    // casts
    let big = 1u64 << 60;
    println!("{} {} {}", big as f64, big as u32, big as i32);
    println!("{} {} {}", max as i64, -1i64 as u64, (max as u128) + 1);
    println!("{} {} {}", 3.9f64 as u64, -3.9f64 as u64, 1e30 as i64);
    println!("{} {} {}", f64::NAN as i64, 300i32 as u64, (big as usize) > 0);
    println!("{} {}", u64::from(7u8) + 1, i64::from(-5i32) * 2);
    let w: u64 = 9u32.into();
    println!("{} {}", w, (w as u8) as char);
    println!("{} {}", 1.5f64.to_bits(), f64::from_bits(4609434218613702656));

    // parsing and formatting
    println!("{:?} {:?}", "18446744073709551615".parse::<u64>(), "18446744073709551616".parse::<u64>().is_err());
    println!("{:?} {:?}", "-9223372036854775808".parse::<i64>(), u64::from_str_radix("ffffffffffffffff", 16));
    println!("[{:>22}] [{:<5}] [{:+}] [{:#b}] [{:o}]", max, 7u64, 5i64, 5u64, 64u64);
    println!("{} {}", max.to_string().len(), format!("{:?}", Some(imin)));

    // patterns and comparisons
    for x in [0u64, 7, 100, 55, u64::MAX] {
        print!("{} ", classify(x));
    }
    println!();
    println!("{} {} {:?}", big > 1 << 59, big == 1152921504606846976, big.cmp(&max));
    println!("{} {}", std::cmp::max(3u64, 9), std::cmp::min(-3i64, 9));

    // collections and iterators
    let mut m: HashMap<u64, &str> = HashMap::new();
    m.insert(1 << 62, "big");
    m.insert(3, "small");
    println!("{:?} {:?}", m.get(&(1 << 62)), m.get(&4));
    let pairs: HashSet<(u64, i64)> = [(1, -1), (1, -1), (2, -2)].into_iter().collect();
    println!("{}", pairs.len());
    let mut vals = vec![5i64, -3, i64::MAX, 0, i64::MIN];
    vals.sort();
    println!("{:?}", vals);
    vals.sort_by(|a, b| b.cmp(a));
    println!("{:?} {:?} {:?}", vals.iter().max(), vals.iter().min(), vals.first());
    let total: u64 = (1..=20u64).product();
    let sum: i64 = (1..=100i64).sum();
    println!("{} {} {:?}", total, sum, (0..10u64).step_by(3).collect::<Vec<_>>());
    println!("{:?}", (0..5u64).rev().map(|x| x * x).collect::<Vec<_>>());
    let wrapped: u64 = [max, 2].iter().fold(0u64, |a, &b| a.wrapping_add(b));
    println!("{}", wrapped);

    let mut stamps = vec![Stamp { secs: 1 << 40, delta: -2 }, Stamp::default(), Stamp { secs: 5, delta: 9 }];
    stamps.sort();
    println!("{:?}", stamps);
    let seen: HashSet<Stamp> = stamps.iter().copied().collect();
    println!("{} {:?}", seen.len(), stamps.iter().map(|s| s.secs).max());
    let opt: Option<u64> = Some(0);
    println!("{:?} {}", opt.map(|x| x + 1), opt.unwrap_or(9));
}
