// The bundled rayon, run sequentially: the parallel iterators and their consumers, rayon's
// `reduce` / `fold`, parallel sorts and `rayon::join`.
use rayon::prelude::*;

fn fib(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let (a, b) = rayon::join(|| fib(n - 1), || fib(n - 2));
    a + b
}

fn total(it: impl ParallelIterator<Item = u64>) -> u64 {
    it.sum()
}

fn main() {
    let v: Vec<u64> = (1..=10).collect();
    let squares: Vec<u64> = v.par_iter().map(|x| x * x).collect();
    println!("{:?}", squares);
    let sum: u64 = v.par_iter().sum();
    println!("sum {}", sum);
    let evens = (0..100u32).into_par_iter().filter(|x| x % 2 == 0).count();
    println!("evens {}", evens);
    let r = v.par_iter().map(|&x| x).reduce(|| 0, |a, b| a + b);
    println!("reduce {}", r);
    let f: u64 = v
        .par_iter()
        .fold(|| 0u64, |acc, &x| acc + x)
        .sum();
    println!("fold {}", f);
    let m = v.par_iter().copied().reduce_with(|a, b| a.max(b));
    println!("max {:?}", m);
    let mut w = vec![5, 3, 9, 1, 7];
    w.par_sort();
    println!("{:?}", w);
    w.par_sort_by(|a, b| b.cmp(a));
    println!("{:?}", w);
    w.par_iter_mut().for_each(|x| *x *= 10);
    println!("{:?}", w);
    let words = vec!["apple", "bob", "kayak", "rust"];
    let found = words.par_iter().find_any(|w| w.len() == 5);
    println!("{:?}", found);
    let pos = words.par_iter().position_first(|w| w.starts_with('k'));
    println!("{:?}", pos);
    let lens: Vec<usize> = words.par_iter().with_min_len(2).map(|w| w.len()).collect();
    println!("{:?}", lens);
    let chunks: Vec<u64> = v.par_chunks(3).map(|c| c.iter().sum()).collect();
    println!("{:?}", chunks);
    let flat: Vec<u64> = v.par_iter().take(3).flat_map_iter(|&x| vec![x, x]).collect();
    println!("{:?}", flat);
    let text = "one two three";
    let n = text.par_split_whitespace().count();
    println!("{} words, any long: {}", n, text.par_split_whitespace().any(|w| w.len() > 4));
    let mut out: Vec<u64> = Vec::new();
    out.par_extend(v.par_iter().filter(|x| **x > 7));
    println!("{:?}", out);
    // (`par_bridge` doesn't keep the order: a sum does.)
    let bridged: u32 = (1..4u32).par_bridge().map(|x| x + 1).sum();
    println!("{}", bridged);
    println!("fib {}", fib(15));
    println!("total {}", total(v.clone().into_par_iter()));
    println!("threads {}", rayon::current_num_threads() > 0);
}
