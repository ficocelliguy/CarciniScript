// Seeded generators give rand 0.8's numbers: `StdRng` (ChaCha12) and `SmallRng`
// (Xoshiro256++), by `seed_from_u64` and `from_seed`, through `gen`, `gen_range`, `gen_bool`,
// `fill`, `shuffle`, `choose`, `choose_multiple` and an iterator's `choose`.
use rand::rngs::{SmallRng, StdRng};
use rand::seq::{SliceRandom, IteratorRandom};
use rand::{Rng, RngCore, SeedableRng};

fn part1() {
    let mut rng = StdRng::seed_from_u64(42);
    println!("{}", rng.r#gen::<u32>());
    println!("{}", rng.gen_range(0..100));
    println!("{}", rng.gen_range(1..=6));
    println!("{}", rng.r#gen::<f64>());
    println!("{}", rng.gen_bool(0.5));
    let mut v: Vec<i32> = (1..=10).collect();
    v.shuffle(&mut rng);
    println!("{:?}", v);
    println!("{:?}", v.choose(&mut rng));
    println!("{}", rng.gen_range(0.0..1.0));
    let mut r2 = StdRng::from_seed([7u8; 32]);
    println!("{}", r2.r#gen::<i32>());
}

fn part2() {
    let mut rng = StdRng::seed_from_u64(12345);
    println!("{} {} {} {}", rng.r#gen::<u8>(), rng.r#gen::<i8>(), rng.r#gen::<u16>(), rng.r#gen::<i16>());
    println!("{} {} {:?}", rng.r#gen::<i32>(), rng.r#gen::<bool>(), rng.r#gen::<char>() as u32);
    println!("{} {}", rng.r#gen::<f32>(), rng.r#gen::<u128>());
    println!("{} {}", rng.r#gen::<i128>(), rng.r#gen::<f64>());
    for _ in 0..5 {
        print!("{} ", rng.gen_range(-5i8..5));
        print!("{} ", rng.gen_range(0u8..=255));
        print!("{} ", rng.gen_range(100u16..60000));
        print!("{} ", rng.gen_range(0usize..1000));
        print!("{} ", rng.gen_range(-1_000_000i64..=1_000_000));
        print!("{} ", rng.gen_range(0u32..7));
        print!("{} ", rng.gen_range(-3i128..300));
        print!("{} ", rng.gen_range(1.5f32..2.5));
        print!("{} ", rng.gen_range(-1.0..=1.0));
        println!("{}", rng.gen_range(i32::MIN..=i32::MAX));
    }
    let mut c = 0;
    for _ in 0..100 { if rng.gen_bool(0.3) { c += 1; } }
    for _ in 0..50 { if rng.gen_ratio(2, 7) { c += 1; } }
    println!("{} {}", c, rng.gen_bool(1.0));
    let mut b = [0u8; 7];
    rng.fill(&mut b);
    println!("{:?}", b);
    let mut w = [0u32; 3];
    rng.fill(&mut w[..]);
    println!("{:?} {}", w, rng.next_u32());
    let v: Vec<i32> = (0..100).collect();
    for n in [0usize, 3, 11, 12, 30, 60, 99, 100, 120] {
        let s: Vec<&i32> = v.choose_multiple(&mut rng, n).collect();
        println!("{} {:?}", n, &s[..s.len().min(6)]);
    }
    let big: Vec<u32> = (0..2000).collect();
    let s: Vec<&u32> = big.choose_multiple(&mut rng, 200).collect();
    println!("{:?}", &s[..5]);
    let s: Vec<&u32> = big.choose_multiple(&mut rng, 1000).collect();
    println!("{:?}", &s[..5]);
    println!("{:?}", (0..50).choose(&mut rng));
    println!("{:?}", v.iter().filter(|x| *x % 3 == 0).choose(&mut rng));
    println!("{:?}", "héllo wörld ✓ ok".chars().choose(&mut rng));
    println!("{:?}", v.iter().map(|x| x * 2).choose(&mut rng));
    println!("{:?}", v.iter().choose_multiple(&mut rng, 4));
    println!("{:?}", v.iter().filter(|x| **x > 50).choose_multiple(&mut rng, 3));
    let mut words = vec!["a", "b", "c", "d", "e", "f"];
    words.shuffle(&mut rng);
    println!("{:?} {:?}", words, words.choose(&mut rng));
    let e: Vec<i32> = vec![];
    println!("{:?}", e.choose(&mut rng));
    println!("{}", rng.gen_range(0..10));
}

fn part3() {
    let mut s = SmallRng::seed_from_u64(99);
    println!("{} {} {}", s.r#gen::<u32>(), s.gen_range(0..1000), s.r#gen::<f64>());
    let mut v: Vec<u8> = (0..20).collect();
    v.shuffle(&mut s);
    println!("{:?}", v);
    let mut b = [0u8; 13];
    s.fill(&mut b);
    println!("{:?}", b);
    let mut z = SmallRng::from_seed([0u8; 32]);
    println!("{}", z.gen_range(0..100));
    let mut seed = [0u8; 32];
    for (i, x) in seed.iter_mut().enumerate() { *x = i as u8 * 7; }
    let mut t = StdRng::from_seed(seed);
    println!("{} {}", t.gen_range(0..100), t.r#gen::<u16>());
    for k in 0..40 { let mut r = StdRng::seed_from_u64(k); print!("{} ", r.gen_range(0..10)); }
    println!();
}

fn main() {
    part1();
    part2();
    part3();
}
