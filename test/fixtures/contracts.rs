// Bitburner coding-contract solvers checked against rustc: largest prime factor, max subarray,
// ways to sum (DP), spiral matrix, merge intervals (`last_mut` on a Vec of arrays), RLE, Caesar
// cipher, unique grid paths, and shortest grid path by BFS.
use std::collections::{HashSet, VecDeque};
fn largest_prime_factor(mut n: u64) -> u64 {
    let mut f = 2; let mut last = 1;
    while f * f <= n { while n % f == 0 { last = f; n /= f; } f += 1; }
    if n > 1 { n } else { last }
}
fn max_subarray(a: &[i64]) -> i64 {
    let (mut best, mut cur) = (a[0], 0i64);
    for &x in a { cur = (cur + x).max(x); best = best.max(cur); }
    best
}
fn ways_to_sum(n: usize) -> u64 {
    let mut dp = vec![0u64; n + 1]; dp[0] = 1;
    for k in 1..n { for s in k..=n { dp[s] += dp[s - k]; } }
    dp[n]
}
fn spiral(m: &[Vec<i32>]) -> Vec<i32> {
    let mut out = vec![];
    if m.is_empty() { return out; }
    let (mut top, mut bottom, mut left, mut right) = (0i32, m.len() as i32 - 1, 0i32, m[0].len() as i32 - 1);
    while top <= bottom && left <= right {
        for c in left..=right { out.push(m[top as usize][c as usize]); }
        top += 1;
        for r in top..=bottom { out.push(m[r as usize][right as usize]); }
        right -= 1;
        if top <= bottom { for c in (left..=right).rev() { out.push(m[bottom as usize][c as usize]); } bottom -= 1; }
        if left <= right { for r in (top..=bottom).rev() { out.push(m[r as usize][left as usize]); } left += 1; }
    }
    out
}
fn merge_intervals(mut v: Vec<[i32; 2]>) -> Vec<[i32; 2]> {
    v.sort_by_key(|x| x[0]);
    let mut out: Vec<[i32; 2]> = vec![];
    for iv in v {
        if let Some(last) = out.last_mut() { if iv[0] <= last[1] { last[1] = last[1].max(iv[1]); continue; } }
        out.push(iv);
    }
    out
}
fn rle(s: &str) -> String {
    let mut out = String::new();
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let mut j = i;
        while j < cs.len() && cs[j] == cs[i] && j - i < 9 { j += 1; }
        out.push_str(&format!("{}{}", j - i, cs[i]));
        i = j;
    }
    out
}
fn caesar(s: &str, k: u8) -> String {
    s.chars().map(|c| if c.is_ascii_uppercase() { (((c as u8 - b'A' + 26 - k % 26) % 26) + b'A') as char } else { c }).collect()
}
fn grid_paths(g: &[Vec<u8>]) -> u64 {
    let (h, w) = (g.len(), g[0].len());
    let mut dp = vec![vec![0u64; w]; h];
    for r in 0..h { for c in 0..w {
        if g[r][c] == 1 { continue; }
        dp[r][c] = if r == 0 && c == 0 { 1 } else { (if r > 0 { dp[r-1][c] } else { 0 }) + (if c > 0 { dp[r][c-1] } else { 0 }) };
    } }
    dp[h-1][w-1]
}
fn shortest(g: &[&str]) -> String {
    let h = g.len(); let w = g[0].len();
    let cells: Vec<Vec<char>> = g.iter().map(|r| r.chars().collect()).collect();
    let mut prev = vec![vec![None; w]; h];
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    q.push_back((0usize, 0usize)); seen.insert((0, 0));
    while let Some((r, c)) = q.pop_front() {
        if (r, c) == (h - 1, w - 1) { break; }
        let dirs: [(i32, i32, char); 4] = [(-1, 0, 'U'), (1, 0, 'D'), (0, -1, 'L'), (0, 1, 'R')];
        for (dr, dc, d) in dirs {
            let (nr, nc) = (r as i32 + dr, c as i32 + dc);
            if nr < 0 || nc < 0 || nr >= h as i32 || nc >= w as i32 { continue; }
            let (nr, nc) = (nr as usize, nc as usize);
            if cells[nr][nc] == '1' || !seen.insert((nr, nc)) { continue; }
            prev[nr][nc] = Some((r, c, d));
            q.push_back((nr, nc));
        }
    }
    let mut path = String::new();
    let mut cur = (h - 1, w - 1);
    while let Some((pr, pc, d)) = prev[cur.0][cur.1] { path.insert(0, d); cur = (pr, pc); }
    path
}
fn main() {
    println!("{} {}", largest_prime_factor(600851475143), largest_prime_factor(97));
    println!("{} {}", max_subarray(&[-2, 1, -3, 4, -1, 2, 1, -5, 4]), ways_to_sum(10));
    println!("{:?}", spiral(&[vec![1, 2, 3, 4], vec![5, 6, 7, 8], vec![9, 10, 11, 12]]));
    println!("{:?}", merge_intervals(vec![[1, 3], [8, 10], [2, 6], [10, 12]]));
    println!("{} {}", rle("aaaaaaaaaaaabbc"), caesar("HELLO WORLD", 3));
    println!("{}", grid_paths(&[vec![0, 0, 0], vec![0, 1, 0], vec![0, 0, 0]]));
    println!("{:?}", shortest(&["0010", "0000", "1101", "0000"]));
}
