// 64- and 128-bit integer operations lowered inline: wrapping arithmetic, shifts by literals
// and variables, casts from the number kinds (negative into unsigned wraps), and `usize` /
// `isize` bit operations with values past 32 bits.
fn fnv(data: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &x in data {
        h ^= x;
        h = h.wrapping_mul(0x100000001b3);
        h = ((h << 5) | (h >> 59)) ^ (h >> 7);
    }
    h
}

fn mix(a: i64, b: i64) -> i64 {
    a.wrapping_mul(b).wrapping_add(a).wrapping_sub(b.wrapping_mul(7))
}

fn main() {
    let v: Vec<u64> = (0..50).map(|i| i * 2_654_435_761).collect();
    println!("{}", fnv(&v));
    println!("{} {}", mix(i64::MAX, 3), mix(-5, i64::MIN));
    let k: u64 = 1 << 63;
    println!("{} {} {}", k.wrapping_add(k), k.wrapping_sub(1), k.wrapping_mul(2));
    let shift = 65u32;
    let s2 = 3;
    println!("{} {} {}", 1u64 << (shift % 64), k >> s2, (k >> 60) << s2);
    let big: u128 = u128::MAX / 3;
    println!("{} {} {}", big.wrapping_mul(5), big << 100, big >> 100);
    let w: u128 = 1 << 127;
    println!("{} {}", w.wrapping_add(w), (w >> 126) << 1);
    // Casts into the BigInt kinds.
    let neg: i32 = -1;
    let small: u8 = 200;
    let idx: usize = 4_000_000_000;
    let off: isize = -3;
    let f = -2.7f64;
    let big_f = 1e30f64;
    println!("{} {} {} {}", neg as u64, neg as i64, small as i64, small as u128);
    println!("{} {} {} {}", idx as u64, idx as i64, off as u64, off as i128);
    println!("{} {} {} {}", f as u64, f as i64, big_f as u64, (-big_f) as i64);
    println!("{} {}", (idx * 3) as u64, (off - 1) as u128);
    println!("{} {}", true as u64, 'A' as u64);
    let i: u32 = u32::MAX;
    println!("{} {}", i as u64 + 1, (i as i64) * -1);
    // Sums wrap in release builds.
    let xs: Vec<u64> = vec![u64::MAX, 2, 3];
    println!("{} {}", xs.iter().sum::<u64>(), xs.iter().product::<u64>());
    // `usize` / `isize` bit operations and shifts.
    let mut idx2: usize = 0;
    let mut acc: usize = 0;
    for _ in 0..1000 {
        idx2 = (idx2 * 5 + 1) & 1023;
        acc ^= idx2 << 3 | (idx2 >> 2);
    }
    println!("{} {}", idx2, acc);
    let wide: usize = 1 << 40;
    let mask: usize = 0xffff_ffff;
    println!("{} {} {} {}", wide & 1023, wide | 5, wide & mask, (wide + 7) ^ mask);
    println!("{} {} {}", wide << 10, wide >> 10, (wide - 1) >> 3);
    let sneg: isize = -1 << 35;
    println!("{} {} {} {}", sneg & 1023, sneg >> 3, sneg << 2, (sneg + 1) & 0x7fff_ffff);
    println!("{} {}", (-7isize) >> 1, (-7isize) & 3);
    let m: usize = (1 << 52) - 1;
    println!("{} {}", m & 0x7fff_ffff, m >> 20);
}
