// Functions returning `impl Iterator<Item = T>` (also borrowing, with `+ '_`), taking
// `impl Iterator` and returning `Box<dyn Iterator>`; `FnMut` parameters, `where` clauses, closure
// composition returning `impl Fn`, and lifetimes in a grouping function.
use std::collections::HashMap;
fn evens(v: &[i32]) -> impl Iterator<Item = &i32> + '_ { v.iter().filter(|x| *x % 2 == 0) }
fn squares(n: u32) -> impl Iterator<Item = u32> { (1..=n).map(|x| x * x) }
fn apply_n<F: FnMut() -> i32>(mut f: F, n: usize) -> Vec<i32> { (0..n).map(|_| f()).collect() }
fn sum_by<T, F>(items: &[T], key: F) -> i64 where F: Fn(&T) -> i64 { items.iter().map(|x| key(x)).sum() }
fn compose<A, B, C>(f: impl Fn(A) -> B, g: impl Fn(B) -> C) -> impl Fn(A) -> C { move |x| g(f(x)) }
fn group<'a>(words: &[&'a str]) -> HashMap<usize, Vec<&'a str>> {
    let mut m: HashMap<usize, Vec<&str>> = HashMap::new();
    for w in words { m.entry(w.len()).or_default().push(w); }
    m
}
fn sum_all(it: impl Iterator<Item = u32>) -> u32 {
    it.sum()
}
fn pick(odd: bool) -> Box<dyn Iterator<Item = u32>> {
    if odd { Box::new((1..10).step_by(2)) } else { Box::new((0..10).step_by(2)) }
}
fn main() {
    let v = vec![1, 2, 3, 4, 5, 6];
    println!("{:?} {:?}", evens(&v).collect::<Vec<_>>(), squares(4).collect::<Vec<_>>());
    let mut n = 0;
    println!("{:?}", apply_n(|| { n += 3; n }, 4));
    println!("{}", sum_by(&["aa", "b", "cccc"], |s| s.len() as i64));
    let f = compose(|x: i32| x + 1, |y: i32| y * 10);
    println!("{} {}", f(1), f(5));
    let g = group(&["hi", "yo", "hey", "a"]);
    let mut keys: Vec<_> = g.keys().copied().collect();
    keys.sort();
    for k in keys { println!("{} {:?}", k, g[&k]); }
    let total: u32 = squares(10).filter(|x| x % 2 == 1).sum();
    println!("{}", total);
    println!("{} {:?} {:?}", sum_all(squares(3)), pick(true).collect::<Vec<_>>(), pick(false).take(2).collect::<Vec<_>>());
}
