// More coding-contract solvers checked against rustc: IP addresses (`(1..=3).contains(..)`),
// Vigenere, sanitize parentheses (BFS over a BTreeSet), LZ decompression (`String::extend`),
// 2-coloring, stock trading DP, triangle path, coin change, array jumping. Integer variables
// typed by a later `.max(..)`.
use std::collections::{BTreeSet, HashMap};
fn ips(s: &str) -> Vec<String> {
    let mut out = vec![];
    let n = s.len();
    for a in 1..4 { for b in 1..4 { for c in 1..4 {
        let d = n as i32 - a - b - c;
        if !(1..=3).contains(&d) { continue; }
        let (a, b, c) = (a as usize, b as usize, c as usize);
        let parts = [&s[..a], &s[a..a + b], &s[a + b..a + b + c], &s[a + b + c..]];
        if parts.iter().all(|p| (p.len() == 1 || !p.starts_with('0')) && p.parse::<u32>().map_or(false, |v| v <= 255)) {
            out.push(parts.join("."));
        }
    } } }
    out
}
fn vigenere(text: &str, key: &str) -> String {
    let k: Vec<u8> = key.bytes().collect();
    text.bytes().enumerate().map(|(i, c)| (((c - b'A') + (k[i % k.len()] - b'A')) % 26 + b'A') as char).collect()
}
fn sanitize(s: &str) -> Vec<String> {
    fn valid(t: &str) -> bool { let mut d = 0; for c in t.chars() { if c == '(' { d += 1; } else if c == ')' { d -= 1; if d < 0 { return false; } } } d == 0 }
    let mut level: BTreeSet<String> = BTreeSet::new();
    level.insert(s.to_string());
    loop {
        let good: Vec<String> = level.iter().filter(|t| valid(t)).cloned().collect();
        if !good.is_empty() { return good; }
        let mut next = BTreeSet::new();
        for t in &level { for (i, c) in t.char_indices() { if c == '(' || c == ')' { let mut u = t.clone(); u.remove(i); next.insert(u); } } }
        level = next;
    }
}
fn lz_decompress(s: &str) -> String {
    let cs: Vec<char> = s.chars().collect();
    let (mut out, mut i, mut literal) = (String::new(), 0, true);
    while i < cs.len() {
        let len = cs[i].to_digit(10).unwrap() as usize; i += 1;
        if len == 0 { literal = !literal; continue; }
        if literal { out.extend(&cs[i..i + len]); i += len; }
        else { let back = cs[i].to_digit(10).unwrap() as usize; i += 1; for _ in 0..len { let ch = out.chars().nth(out.chars().count() - back).unwrap(); out.push(ch); } }
        literal = !literal;
    }
    out
}
fn two_color(n: usize, edges: &[(usize, usize)]) -> Vec<i32> {
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(a, b) in edges { adj.entry(a).or_default().push(b); adj.entry(b).or_default().push(a); }
    let mut color = vec![-1; n];
    for s in 0..n {
        if color[s] != -1 { continue; }
        color[s] = 0;
        let mut stack = vec![s];
        while let Some(u) = stack.pop() {
            for &v in adj.get(&u).map(|v| v.as_slice()).unwrap_or(&[]) {
                if color[v] == -1 { color[v] = 1 - color[u]; stack.push(v); } else if color[v] == color[u] { return vec![]; }
            }
        }
    }
    color
}
fn stock(prices: &[i32], k: usize) -> i32 {
    let mut buy = vec![i32::MIN / 2; k + 1];
    let mut sell = vec![0; k + 1];
    for &p in prices { for j in 1..=k { buy[j] = buy[j].max(sell[j - 1] - p); sell[j] = sell[j].max(buy[j] + p); } }
    sell[k]
}
fn triangle(t: &[Vec<i32>]) -> i32 {
    let mut row = t.last().unwrap().clone();
    for r in (0..t.len() - 1).rev() { for c in 0..=r { row[c] = t[r][c] + row[c].min(row[c + 1]); } }
    row[0]
}
fn coins(n: usize, coins: &[usize]) -> u64 {
    let mut dp = vec![0u64; n + 1]; dp[0] = 1;
    for &c in coins { for s in c..=n { dp[s] += dp[s - c]; } }
    dp[n]
}
fn jump(a: &[usize]) -> i32 {
    let (mut jumps, mut end, mut far) = (0, 0, 0);
    for i in 0..a.len() - 1 { far = far.max(i + a[i]); if i == end { if far == end { return 0; } jumps += 1; end = far; } }
    jumps
}
fn main() {
    println!("{:?}", ips("25525511135"));
    println!("{:?}", ips("1938718066"));
    println!("{}", vigenere("ATTACKATDAWN", "LEMON"));
    println!("{:?} {:?}", sanitize("()())()"), sanitize("(a)())()"));
    println!("{}", lz_decompress("5aaabb450723abb"));
    println!("{:?} {:?}", two_color(4, &[(0, 1), (1, 2), (2, 3)]), two_color(3, &[(0, 1), (1, 2), (2, 0)]));
    println!("{} {}", stock(&[3, 2, 6, 5, 0, 3], 2), stock(&[1, 2, 3, 4, 5], 1));
    println!("{}", triangle(&[vec![2], vec![3, 4], vec![6, 5, 7], vec![4, 1, 8, 3]]));
    println!("{} {}", coins(10, &[1, 2, 5]), jump(&[2, 3, 1, 1, 4]));
    println!("{}", extras());
}

#[allow(dead_code)]
fn extras() -> String {
    let mut s = String::from("ab");
    s.extend(["c", "d"]);
    s.extend(vec!['e', 'f']);
    let words = vec!["x".to_string(), "y".to_string()];
    s.extend(words);
    format!("{s} {} {} {} {}", (5..).contains(&9), (..3).contains(&3), ('a'..='f').contains(&'c'), (1.0..2.0).contains(&2.0))
}
