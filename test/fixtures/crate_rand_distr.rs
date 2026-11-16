// The bundled `rand::distributions` and `rand_distr`: Uniform, WeightedIndex, Bernoulli,
// Alphanumeric, Standard, Normal and friends, `rng.sample(d)`, `sample_iter`, a user's
// `impl Distribution<T> for Standard`, the constructors' errors. Only facts that hold for any
// random outcome are printed.
use rand::distributions::{Alphanumeric, Bernoulli, Distribution, Standard, Uniform, WeightedIndex};
use rand::Rng;
use rand_distr::{Binomial, Exp, Gamma, LogNormal, Normal, NormalError, Poisson, StandardNormal, Triangular};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Dir {
    North,
    East,
    South,
    West,
}

impl Distribution<Dir> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Dir {
        match rng.gen_range(0..4) {
            0 => Dir::North,
            1 => Dir::East,
            2 => Dir::South,
            _ => Dir::West,
        }
    }
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn noisy(d: &Normal<f64>, rng: &mut impl Rng) -> f64 {
    d.sample(rng)
}

fn main() {
    let mut rng = rand::thread_rng();

    let die = Uniform::new_inclusive(1, 6);
    let rolls: Vec<u32> = (0..600).map(|_| die.sample(&mut rng)).collect();
    println!("dice in range: {}", rolls.iter().all(|r| (1..=6).contains(r)));
    println!("all faces: {}", (1..=6).all(|f| rolls.contains(&f)));
    let unit = Uniform::new(0.0, 1.0);
    let us: Vec<f64> = unit.sample_iter(&mut rng).take(100).collect();
    println!("unit floats: {} {}", us.len(), us.iter().all(|u| *u >= 0.0 && *u < 1.0));
    let big = Uniform::new(-5i64, 5);
    println!("i64: {}", (-5..5).contains(&rng.sample(big)));

    let weights = [0, 3, 0, 1];
    let wi = WeightedIndex::new(&weights).unwrap();
    let picks: Vec<usize> = (0..200).map(|_| wi.sample(&mut rng)).collect();
    println!("weighted: {}", picks.iter().all(|i| *i == 1 || *i == 3));
    let items = [("a", 1.5), ("b", 0.0), ("c", 2.5)];
    let wi2 = WeightedIndex::new(items.iter().map(|it| it.1)).unwrap();
    println!("weighted name: {}", items[wi2.sample(&mut rng)].0 != "b");
    println!("{:?}", WeightedIndex::new(Vec::<f64>::new()).err());
    println!("{:?}", WeightedIndex::new(&[0, 0]).err());
    match WeightedIndex::new(&[1.0, -1.0]) {
        Ok(_) => println!("ok?"),
        Err(e) => println!("error: {}", e),
    }

    let coin = Bernoulli::new(0.0).unwrap();
    println!("never: {}", (0..50).all(|_| !coin.sample(&mut rng)));
    println!("always: {}", Bernoulli::from_ratio(3, 3).unwrap().sample(&mut rng));
    println!("{:?}", Bernoulli::new(1.5).map(|_| ()));

    let s: String = rand::thread_rng().sample_iter(&Alphanumeric).take(16).map(char::from).collect();
    println!("alnum: {} {}", s.len(), s.chars().all(|c| c.is_ascii_alphanumeric()));
    let b: u8 = rng.sample(Alphanumeric);
    println!("alnum byte: {}", b.is_ascii_alphanumeric());

    let f: f64 = rng.sample(Standard);
    let x: u8 = Standard.sample(&mut rng);
    let bools: Vec<bool> = Standard.sample_iter(&mut rng).take(3).collect();
    println!("standard: {} {} {}", f < 1.0, x as u32 <= 255, bools.len());
    let d: Dir = rng.r#gen();
    let d2: Dir = rng.sample(Standard);
    println!("dirs: {} {}", d != d2 || d == d2, matches!(d, Dir::North | Dir::East | Dir::South | Dir::West));

    let normal = Normal::new(10.0, 2.0).unwrap();
    let xs: Vec<f64> = (0..4000).map(|_| normal.sample(&mut rng)).collect();
    println!("normal mean ~10: {}", (mean(&xs) - 10.0).abs() < 0.3);
    println!("normal params: {} {}", normal.mean(), normal.std_dev());
    println!("zscore: {}", normal.from_zscore(1.5));
    println!("noisy finite: {}", noisy(&normal, &mut rng).is_finite());
    let z: f64 = rng.sample(StandardNormal);
    println!("standard normal: {}", z.is_finite());
    let bad: Result<Normal<f64>, NormalError> = Normal::new(0.0, f64::INFINITY);
    println!("{:?}", bad);
    if let Err(e) = Normal::new(1.0, f64::NAN) {
        println!("{}", e);
    }
    let ln = LogNormal::new(0.0, 0.5).unwrap();
    println!("lognormal positive: {}", (0..100).all(|_| ln.sample(&mut rng) > 0.0));
    let ln2 = LogNormal::from_mean_cv(3.0, 0.0).unwrap();
    println!("lognormal cv 0: {:.6}", ln2.sample(&mut rng));

    let e = Exp::new(2.0).unwrap();
    let es: Vec<f64> = e.sample_iter(&mut rng).take(4000).collect();
    println!("exp mean ~0.5: {}", (mean(&es) - 0.5).abs() < 0.05);
    println!("{:?}", Exp::new(-1.0).err());

    for lambda in [3.0, 50.0] {
        let p = Poisson::new(lambda).unwrap();
        let ps: Vec<f64> = (0..4000).map(|_| p.sample(&mut rng)).collect();
        let whole = ps.iter().all(|x| x.fract() == 0.0 && *x >= 0.0);
        println!("poisson {}: whole {} mean ok {}", lambda, whole, (mean(&ps) - lambda).abs() < lambda * 0.05 + 0.1);
    }
    println!("{}", Poisson::new(0.0).unwrap_err());

    for (n, p) in [(10u64, 0.3), (1000, 0.5), (500, 0.9)] {
        let bin = Binomial::new(n, p).unwrap();
        let bs: Vec<f64> = (0..3000).map(|_| bin.sample(&mut rng) as f64).collect();
        let in_range = bs.iter().all(|x| *x <= n as f64);
        println!("binomial {} {}: {} {}", n, p, in_range, (mean(&bs) - n as f64 * p).abs() < n as f64 * 0.02 + 0.2);
    }
    println!("{:?}", Binomial::new(5, 1.5).err());

    let g = Gamma::new(2.0, 3.0).unwrap();
    let gs: Vec<f64> = (0..4000).map(|_| g.sample(&mut rng)).collect();
    println!("gamma mean ~6: {}", (mean(&gs) - 6.0).abs() < 0.4);
    let g2 = Gamma::new(0.5, 1.0).unwrap();
    println!("gamma small shape positive: {}", (0..200).all(|_| g2.sample(&mut rng) >= 0.0));

    let t = Triangular::new(0.0, 10.0, 2.0).unwrap();
    println!("triangular: {}", (0..500).all(|_| {
        let v = t.sample(&mut rng);
        (0.0..=10.0).contains(&v)
    }));
    println!("{}", Triangular::new(0.0, 1.0, 5.0).unwrap_err());
}
