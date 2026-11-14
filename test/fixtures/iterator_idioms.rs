// Everyday iterator and closure idioms checked against rustc: sort_by, filter/map/collect,
// partition, windows, chunks, stateful map, skip/step_by, product, enumerate + max_by_key,
// flatten, dedup, joins, zips, FnMut counters and boxed closures cast to `Box<dyn Fn>`.
fn main() {
    let v = vec![5, 3, 8, 1, 9, 2];
    let mut sorted = v.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    println!("{:?} {:?}", v, sorted);
    let evens: Vec<_> = v.iter().filter(|&&x| x % 2 == 0).map(|x| x * 10).collect();
    let (small, big): (Vec<i32>, Vec<i32>) = v.iter().partition(|&&x| x < 5);
    println!("{:?} {:?} {:?}", evens, small, big);
    let windows: Vec<i32> = v.windows(2).map(|w| w[1] - w[0]).collect();
    let chunks: Vec<Vec<i32>> = v.chunks(4).map(|c| c.to_vec()).collect();
    println!("{:?} {:?}", windows, chunks);
    let mut acc = 0;
    let running: Vec<i32> = v.iter().map(|x| { acc += x; acc }).collect();
    println!("{:?} {}", running, acc);
    let s: i32 = v.iter().skip(1).step_by(2).sum();
    let p: i64 = v.iter().map(|&x| x as i64).product();
    println!("{} {}", s, p);
    let mx = v.iter().enumerate().max_by_key(|&(_, &x)| x).unwrap();
    println!("{:?} {:?}", mx, v.iter().rev().take(2).collect::<Vec<_>>());
    let nested = vec![vec![1, 2], vec![], vec![3]];
    let flat: Vec<i32> = nested.into_iter().flatten().collect();
    println!("{:?} {}", flat, flat.iter().any(|&x| x > 2));
    let mut dedup = vec![1, 1, 2, 2, 2, 3, 1];
    dedup.dedup();
    println!("{:?}", dedup);
    let total: f64 = [1.5, 2.25].iter().sum();
    println!("{total}");
    let joined = vec!["a", "b", "c"].join("-");
    let zipped: Vec<(i32, char)> = (1..4).zip("xyz".chars()).collect();
    println!("{joined} {:?} {:?}", zipped, (1..=4).rev().collect::<Vec<_>>());
    let mut counter = 0;
    let mut inc = || { counter += 2; counter };
    inc();
    inc();
    println!("{}", counter);
    let adders: Vec<Box<dyn Fn(i32) -> i32>> = (1..=3).map(|n| Box::new(move |x| x + n) as Box<dyn Fn(i32) -> i32>).collect();
    println!("{:?}", adders.iter().map(|f| f(10)).collect::<Vec<_>>());
}
