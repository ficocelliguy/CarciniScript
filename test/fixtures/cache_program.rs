// A small LRU-style cache through `&mut self` methods, a `&mut` to a nested field, a `Display`
// impl with `?`, memoized recursion, a generic `largest`, `?` on `Option` in a parser, labeled
// breaks over a 2-D array, shadowing, and `while let` over a stack that grows.
use std::fmt;
#[derive(Debug, Clone, Default)]
struct Stats { hits: u32, misses: u32 }
#[derive(Debug, Default)]
struct Cache { entries: Vec<(String, i64)>, stats: Stats, cap: usize }
impl Cache {
    fn new(cap: usize) -> Self { Cache { cap, ..Default::default() } }
    fn get(&mut self, k: &str) -> Option<i64> {
        match self.entries.iter().position(|(key, _)| key == k) {
            Some(i) => { self.stats.hits += 1; let e = self.entries.remove(i); let v = e.1; self.entries.push(e); Some(v) }
            None => { self.stats.misses += 1; None }
        }
    }
    fn put(&mut self, k: &str, v: i64) {
        if self.entries.len() == self.cap { self.entries.remove(0); }
        self.entries.push((k.to_string(), v));
    }
    fn stats_mut(&mut self) -> &mut Stats { &mut self.stats }
}
impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let total = self.hits + self.misses;
        write!(f, "{}/{} hits", self.hits, total)?;
        if total > 0 { write!(f, " ({:.1}%)", 100.0 * self.hits as f64 / total as f64)?; }
        Ok(())
    }
}
fn fib(n: u64, memo: &mut Vec<Option<u64>>) -> u64 {
    if n < 2 { return n; }
    if let Some(v) = memo[n as usize] { return v; }
    let r = fib(n - 1, memo) + fib(n - 2, memo);
    memo[n as usize] = Some(r);
    r
}
fn largest<T: PartialOrd + Copy>(xs: &[T]) -> T { let mut m = xs[0]; for &x in xs { if x > m { m = x; } } m }
fn parse_kv(line: &str) -> Option<(String, i32)> {
    let (k, v) = line.split_once('=')?;
    let v: i32 = v.trim().parse().ok()?;
    Some((k.trim().to_string(), v))
}
fn main() {
    let mut c = Cache::new(2);
    c.put("a", 1); c.put("b", 2);
    println!("{:?} {:?}", c.get("a"), c.get("z"));
    c.put("c", 3);
    println!("{:?} {:?}", c.get("b"), c.entries);
    c.stats_mut().hits += 10;
    println!("{} | {}", c.stats, Stats::default());
    let mut memo = vec![None; 60];
    println!("{} {} {:.2}", fib(50, &mut memo), largest(&[3, 9, 2]), largest(&[1.5, -2.0]));
    for line in ["x = 5", "bad", "y= -3", "z = q"] { println!("{:?}", parse_kv(line)); }
    let mut grid = [[0u8; 3]; 3];
    'outer: for i in 0..3 { for j in 0..3 { if i * j == 2 { break 'outer; } grid[i][j] = (i * 3 + j) as u8; } }
    println!("{:?}", grid);
    let x = 5; let x = x * 2; { let x = "shadow"; println!("{x}"); } println!("{x}");
    let mut stack = vec![1, 2, 3];
    while let Some(top) = stack.pop() { if top == 2 { stack.push(10); stack.push(20); } print!("{top} "); }
    println!();
}
