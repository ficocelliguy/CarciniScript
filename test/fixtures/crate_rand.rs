// The bundled `rand` crate: `thread_rng`, `gen` / `gen_range` / `gen_bool`, `shuffle` / `choose`, seeded
// `StdRng`, `random`. Only facts that hold for any random outcome are printed.
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

fn roll(rng: &mut impl Rng) -> u8 {
    rng.gen_range(1..=6)
}

fn pick<R: Rng>(rng: &mut R, xs: &[&'static str]) -> &'static str {
    xs[rng.gen_range(0..xs.len())]
}

fn main() {
    let mut rng = thread_rng();
    let mut ok = true;
    for _ in 0..1000 {
        let d = roll(&mut rng);
        ok &= (1..=6).contains(&d);
        let f: f64 = rng.r#gen();
        ok &= (0.0..1.0).contains(&f);
        let x = rng.gen_range(-5..5);
        ok &= (-5..5).contains(&x);
        let y = rng.gen_range(2.5..3.0);
        ok &= y >= 2.5 && y < 3.0;
        let big = rng.gen_range(0..u64::MAX);
        ok &= big < u64::MAX;
        let b: u8 = rng.r#gen();
        ok &= b as u32 <= 255;
    }
    println!("ranges ok: {}", ok);
    println!("{} {}", rng.gen_bool(0.0), rng.gen_bool(1.0));
    println!("{}", rng.gen_ratio(3, 3));

    let mut v: Vec<i32> = (1..=20).collect();
    v.shuffle(&mut rng);
    let mut sorted = v.clone();
    sorted.sort();
    println!("permutation: {}", sorted == (1..=20).collect::<Vec<_>>());
    let c = *v.choose(&mut rng).unwrap();
    println!("choose in range: {}", (1..=20).contains(&c));
    let empty: Vec<i32> = Vec::new();
    println!("{:?}", empty.choose(&mut rng));
    let mut three: Vec<i32> = v.choose_multiple(&mut rng, 3).cloned().collect();
    three.sort();
    three.dedup();
    println!("three distinct: {}", three.len());
    let it = (10..20).choose(&mut rng).unwrap();
    println!("iter choose: {}", (10..20).contains(&it));
    let name = pick(&mut rng, &["a", "b"]);
    println!("pick: {}", name == "a" || name == "b");

    let mut a = StdRng::seed_from_u64(42);
    let mut b = StdRng::seed_from_u64(42);
    let xs: Vec<u32> = (0..5).map(|_| a.gen_range(0..1000)).collect();
    let ys: Vec<u32> = (0..5).map(|_| b.gen_range(0..1000)).collect();
    println!("seeded repeat: {}", xs == ys);
    let mut buf = [0u32; 64];
    rng.fill(&mut buf);
    println!("filled: {}", buf.iter().any(|&x| x != 0));
    let r: f64 = random();
    let n = random::<u16>();
    println!("{} {}", r < 1.0, n as u32 <= 65535);
}
