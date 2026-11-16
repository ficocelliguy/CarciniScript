//! Finds every coding contract on the network and solves the ones we know.
//!
//! run contracts.rs [--dry-run]

mod net;

use std::cmp::{max, min, Ordering};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What `attempt` takes. Untagged so it serializes as the bare JS value the game expects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Answer {
    Int(i64),
    Text(String),
    Texts(Vec<String>),
    Ints(Vec<i64>),
    Pairs(Vec<[i64; 2]>),
}

impl fmt::Display for Answer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Answer::Int(n) => write!(f, "{n}"),
            Answer::Text(s) => write!(f, "{s:?}"),
            Answer::Texts(v) => write!(f, "{v:?}"),
            Answer::Ints(v) => write!(f, "{v:?}"),
            Answer::Pairs(v) => write!(f, "{v:?}"),
        }
    }
}

impl From<i64> for Answer {
    fn from(n: i64) -> Self {
        Answer::Int(n)
    }
}

impl From<String> for Answer {
    fn from(s: String) -> Self {
        Answer::Text(s)
    }
}

impl From<Vec<String>> for Answer {
    fn from(v: Vec<String>) -> Self {
        Answer::Texts(v)
    }
}

type Solver = fn(&Value) -> Option<Answer>;

/// Contract type → solver.
const SOLVERS: &[(&str, Solver)] = &[
    ("Find Largest Prime Factor", |d| largest_prime_factor(d.as_i64()?).map(Answer::Int)),
    ("Subarray with Maximum Sum", |d| Some(max_subarray(&ints(d)?).into())),
    ("Total Ways to Sum", |d| Some(ways_to_sum(d.as_u64()? as usize, &(1..d.as_u64()? as usize).collect::<Vec<_>>()).into())),
    ("Total Ways to Sum II", |d| {
        let n = d.get(0)?.as_u64()? as usize;
        let parts: Vec<usize> = ints(d.get(1)?)?.into_iter().map(|x| x as usize).collect();
        Some(ways_to_sum(n, &parts).into())
    }),
    ("Spiralize Matrix", |d| Some(Answer::Ints(spiral(&grid(d)?)))),
    ("Array Jumping Game", |d| Some(Answer::Int(min_jumps(&ints(d)?).is_some() as i64))),
    ("Array Jumping Game II", |d| Some(Answer::Int(min_jumps(&ints(d)?).unwrap_or(0) as i64))),
    ("Merge Overlapping Intervals", |d| Some(Answer::Pairs(merge_intervals(grid(d)?)))),
    ("Generate IP Addresses", |d| Some(ip_addresses(d.as_str()?).into())),
    ("Algorithmic Stock Trader I", |d| Some(stock_trader(1, &ints(d)?).into())),
    ("Algorithmic Stock Trader II", |d| Some(stock_trader(usize::MAX, &ints(d)?).into())),
    ("Algorithmic Stock Trader III", |d| Some(stock_trader(2, &ints(d)?).into())),
    ("Algorithmic Stock Trader IV", |d| {
        Some(stock_trader(d.get(0)?.as_u64()? as usize, &ints(d.get(1)?)?).into())
    }),
    ("Minimum Path Sum in a Triangle", |d| Some(triangle_path(&grid(d)?).into())),
    ("Unique Paths in a Grid I", |d| {
        let [r, c] = <[i64; 2]>::try_from(ints(d)?).ok()?;
        Some(Answer::Int(unique_paths(&vec![vec![0; c as usize]; r as usize])))
    }),
    ("Unique Paths in a Grid II", |d| Some(Answer::Int(unique_paths(&grid(d)?)))),
    ("Shortest Path in a Grid", |d| Some(shortest_path(&grid(d)?).unwrap_or_default().into())),
    ("Sanitize Parentheses in Expression", |d| Some(sanitize_parens(d.as_str()?).into())),
    ("Find All Valid Math Expressions", |d| {
        Some(math_expressions(d.get(0)?.as_str()?, d.get(1)?.as_i64()?).into())
    }),
    ("Proper 2-Coloring of a Graph", |d| {
        let n = d.get(0)?.as_u64()? as usize;
        let edges = grid(d.get(1)?)?;
        Some(Answer::Ints(two_color(n, &edges).unwrap_or_default()))
    }),
    ("Compression I: RLE Compression", |d| Some(rle(d.as_str()?).into())),
    ("Encryption I: Caesar Cipher", |d| {
        Some(caesar(d.get(0)?.as_str()?, d.get(1)?.as_u64()? as u8).into())
    }),
    ("Encryption II: Vigenère Cipher", |d| Some(vigenere(d.get(0)?.as_str()?, d.get(1)?.as_str()?).into())),
];

fn ints(v: &Value) -> Option<Vec<i64>> {
    v.as_array()?.iter().map(Value::as_i64).collect()
}

fn grid(v: &Value) -> Option<Vec<Vec<i64>>> {
    v.as_array()?.iter().map(ints).collect()
}

// ---------------------------------------------------------------------------------------

fn largest_prime_factor(mut n: i64) -> Option<i64> {
    let mut f = 2;
    let mut last = None;
    while f * f <= n {
        while n % f == 0 {
            n /= f;
            last = Some(f);
        }
        f += 1;
    }
    if n > 1 { Some(n) } else { last }
}

/// Kadane's algorithm.
fn max_subarray(xs: &[i64]) -> i64 {
    xs.iter()
        .scan(0i64, |run, &x| {
            *run = max(x, *run + x);
            Some(*run)
        })
        .max()
        .unwrap_or(0)
}

/// Partitions of `n` into parts from `parts` (each usable any number of times), excluding
/// the trivial one-part sum.
fn ways_to_sum(n: usize, parts: &[usize]) -> i64 {
    let mut ways = vec![0i64; n + 1];
    ways[0] = 1;
    for &p in parts.iter().filter(|&&p| p <= n) {
        for total in p..=n {
            ways[total] += ways[total - p];
        }
    }
    ways[n]
}

fn spiral(m: &[Vec<i64>]) -> Vec<i64> {
    let mut out = Vec::new();
    if m.is_empty() {
        return out;
    }
    let (mut top, mut left) = (0i64, 0i64);
    let (mut bottom, mut right) = (m.len() as i64 - 1, m[0].len() as i64 - 1);
    let at = |r: i64, c: i64| m[r as usize][c as usize];
    while top <= bottom && left <= right {
        out.extend((left..=right).map(|c| at(top, c)));
        top += 1;
        out.extend((top..=bottom).map(|r| at(r, right)));
        right -= 1;
        if top <= bottom {
            out.extend((left..=right).rev().map(|c| at(bottom, c)));
            bottom -= 1;
        }
        if left <= right {
            out.extend((top..=bottom).rev().map(|r| at(r, left)));
            left += 1;
        }
    }
    out
}

/// Greedy BFS over jump ranges; `None` if the end is unreachable.
fn min_jumps(xs: &[i64]) -> Option<usize> {
    let last = xs.len().checked_sub(1)?;
    let (mut jumps, mut reach, mut frontier) = (0, 0usize, 0usize);
    for (i, &x) in xs.iter().enumerate() {
        if i > reach {
            return None;
        }
        if i == last {
            return Some(jumps);
        }
        reach = reach.max(i + x as usize);
        if i == frontier {
            jumps += 1;
            frontier = reach;
        }
    }
    Some(jumps)
}

fn merge_intervals(mut v: Vec<Vec<i64>>) -> Vec<[i64; 2]> {
    v.sort_unstable_by_key(|iv| iv[0]);
    v.into_iter().fold(Vec::new(), |mut acc: Vec<[i64; 2]>, iv| {
        match acc.last_mut() {
            Some(last) if iv[0] <= last[1] => last[1] = last[1].max(iv[1]),
            _ => acc.push([iv[0], iv[1]]),
        }
        acc
    })
}

fn ip_addresses(s: &str) -> Vec<String> {
    fn valid(part: &str) -> bool {
        !part.is_empty()
            && part.len() <= 3
            && (part == "0" || !part.starts_with('0'))
            && part.parse::<u16>().is_ok_and(|n| n <= 255)
    }
    let n = s.len();
    let mut out = Vec::new();
    for a in 1..=3.min(n) {
        for b in a + 1..=(a + 3).min(n) {
            for c in b + 1..=(b + 3).min(n) {
                let parts = [&s[..a], &s[a..b], &s[b..c], &s[c..]];
                if parts.iter().all(|p| valid(p)) {
                    out.push(parts.join("."));
                }
            }
        }
    }
    out
}

/// Max profit with at most `k` transactions.
fn stock_trader(k: usize, prices: &[i64]) -> i64 {
    if prices.len() < 2 {
        return 0;
    }
    if k >= prices.len() / 2 {
        return prices.windows(2).map(|w| (w[1] - w[0]).max(0)).sum();
    }
    let mut hold = vec![i64::MIN / 2; k + 1];
    let mut free = vec![0i64; k + 1];
    for &p in prices {
        for t in (1..=k).rev() {
            free[t] = free[t].max(hold[t] + p);
            hold[t] = hold[t].max(free[t - 1] - p);
        }
    }
    free[k]
}

fn triangle_path(rows: &[Vec<i64>]) -> i64 {
    let Some((last, above)) = rows.split_last() else { return 0 };
    let mut best = last.clone();
    for row in above.iter().rev() {
        best = row
            .iter()
            .enumerate()
            .map(|(i, &v)| v + min(best[i], best[i + 1]))
            .collect();
    }
    best[0]
}

/// Paths from top-left to bottom-right moving right/down, avoiding 1s.
fn unique_paths(g: &[Vec<i64>]) -> i64 {
    let cols = g.first().map_or(0, Vec::len);
    let mut row = vec![0i64; cols];
    if cols > 0 {
        row[0] = 1;
    }
    for r in g {
        for c in 0..cols {
            if r[c] == 1 {
                row[c] = 0;
            } else if c > 0 {
                row[c] += row[c - 1];
            }
        }
    }
    row.last().copied().unwrap_or(0)
}

fn shortest_path(g: &[Vec<i64>]) -> Option<String> {
    let (rows, cols) = (g.len(), g.first()?.len());
    let goal = (rows - 1, cols - 1);
    let mut prev: HashMap<(usize, usize), ((usize, usize), char)> = HashMap::new();
    let mut queue = VecDeque::from([(0usize, 0usize)]);
    let moves = [(-1i32, 0i32, 'U'), (1, 0, 'D'), (0, -1, 'L'), (0, 1, 'R')];
    while let Some(cell @ (r, c)) = queue.pop_front() {
        if cell == goal {
            let mut path = Vec::new();
            let mut at = cell;
            while let Some(&(from, dir)) = prev.get(&at) {
                path.push(dir);
                at = from;
            }
            return Some(path.iter().rev().collect());
        }
        for (dr, dc, dir) in moves {
            let (Some(nr), Some(nc)) = (r.checked_add_signed(dr as isize), c.checked_add_signed(dc as isize)) else {
                continue;
            };
            if nr >= rows || nc >= cols || g[nr][nc] == 1 || (nr, nc) == (0, 0) {
                continue;
            }
            if let std::collections::hash_map::Entry::Vacant(e) = prev.entry((nr, nc)) {
                e.insert((cell, dir));
                queue.push_back((nr, nc));
            }
        }
    }
    None
}

/// BFS over removals; the first level with any balanced strings is the answer.
fn sanitize_parens(s: &str) -> Vec<String> {
    fn balanced(s: &str) -> bool {
        s.chars()
            .try_fold(0i32, |depth, c| match c {
                '(' => Some(depth + 1),
                ')' => (depth > 0).then(|| depth - 1),
                _ => Some(depth),
            })
            == Some(0)
    }
    let mut level: BTreeSet<String> = BTreeSet::from([s.to_owned()]);
    while !level.is_empty() {
        let found: Vec<String> = level.iter().filter(|s| balanced(s)).cloned().collect();
        if !found.is_empty() {
            return found;
        }
        level = level
            .iter()
            .flat_map(|s| {
                s.char_indices()
                    .filter(|(_, c)| matches!(c, '(' | ')'))
                    .map(move |(i, _)| format!("{}{}", &s[..i], &s[i + 1..]))
            })
            .collect();
    }
    vec![String::new()]
}

fn math_expressions(digits: &str, target: i64) -> Vec<String> {
    fn go(digits: &[u8], target: i64, pos: usize, expr: &mut String, value: i64, last: i64, out: &mut Vec<String>) {
        if pos == digits.len() {
            if value == target {
                out.push(expr.clone());
            }
            return;
        }
        for end in pos + 1..=digits.len() {
            if end > pos + 1 && digits[pos] == b'0' {
                break;
            }
            let Ok(n) = std::str::from_utf8(&digits[pos..end]).unwrap().parse::<i64>() else { break };
            let len = expr.len();
            if pos == 0 {
                expr.push_str(&n.to_string());
                go(digits, target, end, expr, n, n, out);
                expr.truncate(len);
                continue;
            }
            for (op, v, l) in [('+', value + n, n), ('-', value - n, -n), ('*', value - last + last * n, last * n)] {
                expr.push(op);
                expr.push_str(&n.to_string());
                go(digits, target, end, expr, v, l, out);
                expr.truncate(len);
            }
        }
    }
    let mut out = Vec::new();
    go(digits.as_bytes(), target, 0, &mut String::new(), 0, 0, &mut out);
    out
}

fn two_color(n: usize, edges: &[Vec<i64>]) -> Option<Vec<i64>> {
    let mut adj = vec![Vec::new(); n];
    for e in edges {
        let (a, b) = (e[0] as usize, e[1] as usize);
        adj[a].push(b);
        adj[b].push(a);
    }
    let mut color: Vec<Option<i64>> = vec![None; n];
    for start in 0..n {
        if color[start].is_some() {
            continue;
        }
        color[start] = Some(0);
        let mut stack = vec![start];
        while let Some(u) = stack.pop() {
            let cu = color[u]?;
            for &v in &adj[u] {
                match color[v] {
                    None => {
                        color[v] = Some(1 - cu);
                        stack.push(v);
                    }
                    Some(cv) if cv == cu => return None,
                    Some(_) => {}
                }
            }
        }
    }
    color.into_iter().collect()
}

fn rle(s: &str) -> String {
    use itertools::Itertools;
    s.chars()
        .chunk_by(|&c| c)
        .into_iter()
        .flat_map(|(c, run)| {
            let n = run.count();
            let full = std::iter::repeat_n(format!("9{c}"), n / 9);
            let rest = (n % 9 > 0).then(|| format!("{}{c}", n % 9));
            full.chain(rest)
        })
        .collect()
}

fn shift(c: char, by: u8) -> char {
    if c.is_ascii_uppercase() {
        (((c as u8 - b'A' + by) % 26) + b'A') as char
    } else {
        c
    }
}

fn caesar(text: &str, left: u8) -> String {
    text.chars().map(|c| shift(c, 26 - left % 26)).collect()
}

fn vigenere(text: &str, key: &str) -> String {
    text.chars()
        .zip(key.bytes().cycle())
        .map(|(c, k)| shift(c, k - b'A'))
        .collect()
}

// ---------------------------------------------------------------------------------------

#[derive(Debug)]
struct Contract {
    host: String,
    file: String,
    kind: String,
}

impl PartialEq for Contract {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Contract {}
impl PartialOrd for Contract {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Contract {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind
            .cmp(&other.kind)
            .then_with(|| self.host.cmp(&other.host))
            .then_with(|| self.file.cmp(&other.file))
    }
}

pub async fn main(ns: NS) {
    let dry_run = ns.args.iter().any(|a| a.as_string().as_deref() == Some("--dry-run"));
    let solvers: HashMap<&str, Solver> = SOLVERS.iter().copied().collect();

    let mut contracts: Vec<Contract> = net::all_hosts(&ns)
        .into_iter()
        .flat_map(|host| {
            ns.ls(&host, ".cct")
                .into_iter()
                .map(move |file| (host.clone(), file))
                .collect::<Vec<_>>()
        })
        .map(|(host, file)| Contract {
            kind: ns.codingcontract.get_contract_type(&file, &host),
            host: host.0,
            file,
        })
        .collect();
    contracts.sort();

    let mut unknown: HashSet<String> = HashSet::new();
    let (mut solved, mut failed) = (0, 0);
    for c in &contracts {
        let Some(solve) = solvers.get(c.kind.as_str()) else {
            unknown.insert(c.kind.clone());
            continue;
        };
        let raw = ns.codingcontract.get_data(&c.file, &c.host);
        let data: Value = match serde_wasm_bindgen::from_value(raw) {
            Ok(v) => v,
            Err(e) => {
                ns.print(&format!("{}: unreadable data: {e}", c.file));
                continue;
            }
        };
        let Some(answer) = solve(&data) else {
            ns.print(&format!("{} ({}): solver gave up", c.file, c.kind));
            continue;
        };
        if dry_run {
            ns.tprint(&format!("{:<40} {:<18} {answer}", c.kind, c.host));
            continue;
        }
        let reward = ns.codingcontract.attempt(
            serde_wasm_bindgen::to_value(&answer).expect("answer converts"),
            &c.file,
            &c.host,
        );
        if reward.is_empty() {
            failed += 1;
            ns.tprint(&format!(
                "FAIL {} on {} ({}): {answer}, {} tries left",
                c.file,
                c.host,
                c.kind,
                ns.codingcontract.get_num_tries_remaining(&c.file, &c.host)
            ));
        } else {
            solved += 1;
            ns.tprint(&format!("OK {} on {}: {reward}", c.kind, c.host));
        }
    }
    ns.tprint(&format!("solved {solved}, failed {failed}, {} total", contracts.len()));
    if !unknown.is_empty() {
        let mut kinds: Vec<_> = unknown.into_iter().collect();
        kinds.sort_unstable();
        ns.tprint(&format!("no solver for: {}", kinds.join(", ")));
    }
}
