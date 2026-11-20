// `i128` / `u128` are exact over all 128 bits (BigInts): hashes and random number generators,
// wrapping / checked / saturating arithmetic at the edges, the integer methods, casts, parsing,
// formatting, patterns, collections and iterators.
use std::collections::{HashMap, HashSet};

fn xorshift(state: &mut u128) -> u128 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn splitmix(state: &mut u128) -> u128 {
    *state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn fnv1a(s: &str) -> u128 {
    let mut h: u128 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u128;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn classify(x: u128) -> &'static str {
    match x {
        0 => "zero",
        1..=9 => "digit",
        10 | 100 | 1000 => "round",
        x if x == u128::MAX => "max",
        _ => "other",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
struct Stamp {
    secs: u128,
    delta: i128,
}

fn main() {
    let mut s = 88172645463325252u128;
    let xs: Vec<u128> = (0..3).map(|_| xorshift(&mut s)).collect();
    println!("{:?}", xs);
    let mut sm = 42u128;
    println!("{} {}", splitmix(&mut sm), splitmix(&mut sm));
    println!("{:x} {:032X} {}", fnv1a("hello"), fnv1a(""), fnv1a("carcini") % 1000);

    // edges
    let max = u128::MAX;
    let imin = i128::MIN;
    println!("{} {} {} {}", max, imin, i128::MAX, max.wrapping_add(2));
    println!("{} {}", imin.wrapping_sub(1), (i128::MAX).wrapping_mul(3));
    println!("{:?} {:?} {:?}", max.checked_add(1), max.checked_sub(1), imin.checked_neg());
    println!("{:?} {:?} {:?}", imin.checked_div(-1), 7i128.checked_rem(0), imin.checked_abs());
    println!("{} {} {}", max.saturating_add(5), imin.saturating_sub(1), imin.saturating_abs());
    println!("{} {} {}", 3u128.wrapping_sub(5), imin.wrapping_abs(), imin.wrapping_neg());
    println!("{:?} {:?} {}", 3u128.checked_pow(80), 3u128.checked_pow(81), 3u128.wrapping_pow(81));
    println!("{} {}", 2i128.saturating_pow(130), (-2i128).saturating_pow(131));
    println!("{} {}", imin.wrapping_div(-1), imin.wrapping_rem(-1));

    // methods
    let v: i128 = -1234567890123;
    println!("{} {} {} {}", v.abs(), v.signum(), v.pow(1), (-3i128).pow(3));
    println!("{} {} {}", v.min(5), v.max(5), v.clamp(-10, 10));
    println!("{} {} {}", v.rem_euclid(7), v.div_euclid(7), v % 7);
    println!("{} {}", v.abs_diff(1000), 5u128.abs_diff(9));
    println!("{} {} {} {}", max.count_ones(), 12u128.count_zeros(), 1u128.leading_zeros(), 8i128.trailing_zeros());
    println!("{} {} {}", 0u128.leading_zeros(), (1u128 << 127).is_power_of_two(), 6u128.is_power_of_two());
    println!("{} {} {}", 1_000_000_000_000u128.isqrt(), max.ilog2(), 12345678901234u128.ilog10());
    println!("{} {}", v.is_negative(), v.is_positive());

    // shifts and bits
    let a: u128 = 0xF0F0_F0F0_F0F0_F0F0_F0F0_F0F0_F0F0_F0F0;
    println!("{} {} {} {}", a >> 4, a << 4, !a, a & 0xFF);
    println!("{} {} {}", (-8i128) >> 1, (1i128 << 126) << 1, -(i128::MAX));

    // casts
    let big = 1u128 << 100;
    println!("{} {} {}", big as f64 == 2f64.powi(100), big as u32, big as i32);
    println!("{} {} {}", max as i128, -1i128 as u128, max as u32);
    println!("{} {} {}", 3.9f64 as u128, -3.9f64 as u128, 1e40 as i128);
    println!("{} {} {}", f64::NAN as i128, 300i32 as u128, (big as usize) > 0);
    println!("{} {}", u128::from(7u8) + 1, i128::from(-5i32) * 2);
    let w: u128 = 9u32.into();
    println!("{} {}", w, (w as u8) as char);

    // parsing and formatting
    println!("{:?} {:?}", "340282366920938463463374607431768211455".parse::<u128>(), "340282366920938463463374607431768211456".parse::<u128>().is_err());
    println!("{:?} {:?}", "-170141183460469231731687303715884105728".parse::<i128>(), u128::from_str_radix("ffffffffffffffffffffffffffffffff", 16));
    println!("[{:>42}] [{:<5}] [{:+}] [{:#b}] [{:o}]", max, 7u128, 5i128, 5u128, 64u128);
    println!("{} {}", max.to_string().len(), format!("{:?}", Some(imin)));

    // patterns and comparisons
    for x in [0u128, 7, 100, 55, u128::MAX] {
        print!("{} ", classify(x));
    }
    println!();
    println!("{} {} {:?}", big > 1 << 99, big == 1267650600228229401496703205376, big.cmp(&max));
    println!("{} {}", std::cmp::max(3u128, 9), std::cmp::min(-3i128, 9));

    // collections and iterators
    let mut m: HashMap<u128, &str> = HashMap::new();
    m.insert(1 << 100, "big");
    m.insert(3, "small");
    println!("{:?} {:?}", m.get(&(1 << 100)), m.get(&4));
    let pairs: HashSet<(u128, i128)> = [(1, -1), (1, -1), (2, -2)].into_iter().collect();
    println!("{}", pairs.len());
    let mut vals = vec![5i128, -3, i128::MAX, 0, i128::MIN];
    vals.sort();
    println!("{:?}", vals);
    vals.sort_by(|a, b| b.cmp(a));
    println!("{:?} {:?} {:?}", vals.iter().max(), vals.iter().min(), vals.first());
    let total: u128 = (1..=30u128).product();
    let sum: i128 = (1..=100i128).sum();
    println!("{} {} {:?}", total, sum, (0..10u128).step_by(3).collect::<Vec<_>>());
    println!("{:?}", (0..5u128).rev().map(|x| x * x).collect::<Vec<_>>());
    let wrapped: u128 = [max, 2].iter().fold(0u128, |a, &b| a.wrapping_add(b));
    println!("{}", wrapped);

    let mut stamps = vec![Stamp { secs: 1 << 100, delta: -2 }, Stamp::default(), Stamp { secs: 5, delta: 9 }];
    stamps.sort();
    println!("{:?}", stamps);
    let seen: HashSet<Stamp> = stamps.iter().copied().collect();
    println!("{} {:?}", seen.len(), stamps.iter().map(|s| s.secs).max());
    let opt: Option<u128> = Some(0);
    println!("{:?} {}", opt.map(|x| x + 1), opt.unwrap_or(9));
}
