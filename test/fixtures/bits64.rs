// Bitwise operators on 64-bit integers (`u64`, `i64`, `usize`, `isize`) beyond 32 bits, within
// the 2^53 a JS number holds exactly: masks, shifts (also compound `<<=`/`|=`), xor swaps, `!` on
// signed values, bit tricks used in contract solvers, and `from_str_radix` / `as_bytes`.
fn popcount(mut x: u64) -> u32 {
    let mut n = 0;
    while x != 0 {
        x &= x - 1;
        n += 1;
    }
    n
}

fn subsets(items: &[&str]) -> Vec<String> {
    (0..1usize << items.len())
        .map(|mask| {
            (0..items.len())
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| items[i])
                .collect::<Vec<_>>()
                .join("")
        })
        .collect()
}

fn main() {
    let big: u64 = 1 << 40;
    let mask: u64 = (1 << 45) - 1;
    println!("{} {} {} {}", big, mask, big | 7, mask & (big + 5));
    println!("{} {} {}", (big ^ mask), big >> 38, (mask << 3) >> 40);
    let mut flags: usize = 0;
    for b in [1, 33, 40, 47] {
        flags |= 1 << b;
    }
    println!("{} {} {}", flags, popcount(flags as u64), flags.count_ones());
    let mut x: i64 = -5_000_000_000;
    x <<= 2;
    let y = x >> 3;
    println!("{} {} {} {}", x, y, !x, x & 0xffff);
    let (mut a, mut b) = (3_000_000_000u64, 7u64);
    a ^= b;
    b ^= a;
    a ^= b;
    println!("{a} {b}");
    println!("{:?}", subsets(&["a", "b", "c"]));
    println!("{:?} {:?} {:?}", u64::from_str_radix("ffffffffff", 16), i32::from_str_radix("-zz", 36), u8::from_str_radix("256", 10));
    println!("{:?} {:?}", "hé".as_bytes(), String::from_utf8(vec![104, 105]));
    // Compound shifts wrap like their type (the shift amount's type doesn't matter).
    let mut small: u8 = 200;
    small <<= 1;
    let mut word: u16 = 0x8001;
    word <<= 4u32;
    let mut neg: i8 = -128;
    neg >>= 1;
    println!("{small} {word} {neg}");
}
