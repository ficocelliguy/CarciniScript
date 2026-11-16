// edition: 2021
// Code written for the 2021 edition: `gen` is a name, not a keyword (`rng.gen()` from rand 0.8,
// a binding or function called `gen`). The output doesn't depend on the random values.
use rand::Rng;

fn gen(n: u32) -> u32 {
    n * 2
}

fn main() {
    let mut rng = rand::thread_rng();
    let x: f64 = rng.gen();
    let b: bool = rng.gen();
    let n = rng.gen::<u8>();
    let r = rng.gen_range(1..=6);
    let gen = gen(21);
    println!("{} {} {} {} {}", (0.0..1.0).contains(&x), b || !b, u32::from(n) < 256, (1..=6).contains(&r), gen);
}
