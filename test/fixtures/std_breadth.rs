// Breadth of the std surface against rustc: sort_unstable + binary_search, dedup / dedup_by_key,
// rposition, chunks_exact, char_indices, split_at, rsplit_once, memoized recursion through a
// nested fn with a HashMap, letter counts in an array, and a grid transposed into strings.
use std::collections::HashMap;
fn main() {
    let mut v = vec![5, 1, 4, 1, 5, 9, 2, 6, 5, 3];
    v.sort_unstable();
    println!("{:?} {:?} {:?}", v, v.binary_search(&4), v.binary_search(&7));
    v.dedup();
    println!("{:?} {:?}", v, v.iter().rposition(|&x| x < 5));
    let mut words = vec!["apple", "bob", "cat", "dove", "egg"];
    words.dedup_by_key(|w| w.len());
    println!("{:?}", words);
    let data = [1, 2, 3, 4, 5, 6, 7];
    let sums: Vec<i32> = data.chunks_exact(3).map(|c| c.iter().sum()).collect();
    println!("{:?} {}", sums, data.chunks_exact(3).count());
    let s = "The Quick brown fox";
    let caps: Vec<(usize, char)> = s.char_indices().filter(|(_, c)| c.is_uppercase()).collect();
    println!("{:?} {}", caps, s.split(' ').rev().collect::<Vec<_>>().join(" "));
    println!("{:?} {:?}", s.split_at(3), s.rsplit_once(' '));
    let mut memo: HashMap<u64, u64> = HashMap::new();
    fn collatz(n: u64, memo: &mut HashMap<u64, u64>) -> u64 {
        if n == 1 { return 0; }
        if let Some(&v) = memo.get(&n) { return v; }
        let r = 1 + if n % 2 == 0 { collatz(n / 2, memo) } else { collatz(3 * n + 1, memo) };
        memo.insert(n, r);
        r
    }
    let best = (1..30u64).max_by_key(|&n| collatz(n, &mut memo)).unwrap();
    println!("{} {}", best, collatz(best, &mut memo));
    let mut counts = [0usize; 26];
    for c in "hello world".chars().filter(|c| c.is_ascii_lowercase()) { counts[(c as u8 - b'a') as usize] += 1; }
    let top: Vec<(char, usize)> = counts.iter().enumerate().filter(|&(_, &n)| n > 1).map(|(i, &n)| ((b'a' + i as u8) as char, n)).collect();
    println!("{:?}", top);
    let mut grid = vec![vec!['.'; 4]; 3];
    grid[1][2] = '#';
    for row in &grid { println!("{}", row.iter().collect::<String>()); }
    let transposed: Vec<String> = (0..4).map(|c| grid.iter().map(|r| r[c]).collect()).collect();
    println!("{:?}", transposed);
}
