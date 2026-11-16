// The bundled `rustc_hash`: `FxHashMap` / `FxHashSet` are std's `HashMap` / `HashSet` (made with
// `default()`, `with_capacity_and_hasher` or `with_hasher(FxBuildHasher)`), as in puzzle-style
// code: BFS over a grid and memoized recursion. Output doesn't depend on iteration order.
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::collections::VecDeque;

fn ways(n: u64, memo: &mut FxHashMap<u64, u64>) -> u64 {
    if n <= 1 {
        return 1;
    }
    if let Some(&v) = memo.get(&n) {
        return v;
    }
    let v = ways(n - 1, memo) + ways(n - 2, memo);
    memo.insert(n, v);
    v
}

fn bfs(grid: &[&str]) -> Option<usize> {
    let h = grid.len() as i32;
    let w = grid[0].len() as i32;
    let mut seen: FxHashSet<(i32, i32)> = FxHashSet::default();
    let mut q = VecDeque::new();
    q.push_back(((0, 0), 0));
    seen.insert((0, 0));
    while let Some(((r, c), d)) = q.pop_front() {
        if (r, c) == (h - 1, w - 1) {
            return Some(d);
        }
        for (dr, dc) in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
            let (nr, nc) = (r + dr, c + dc);
            if nr < 0 || nc < 0 || nr >= h || nc >= w {
                continue;
            }
            if grid[nr as usize].as_bytes()[nc as usize] == b'#' {
                continue;
            }
            if seen.insert((nr, nc)) {
                q.push_back(((nr, nc), d + 1));
            }
        }
    }
    None
}

fn main() {
    let mut memo = FxHashMap::default();
    println!("{} {}", ways(50, &mut memo), memo.len());

    println!("{:?}", bfs(&["..#.", "#...", "..#.", "...."]));

    let mut counts: FxHashMap<&str, usize> = FxHashMap::with_capacity_and_hasher(8, Default::default());
    for w in "a b a c b a".split(' ') {
        *counts.entry(w).or_insert(0) += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort();
    println!("{v:?}");

    let mut s = FxHashSet::with_hasher(FxBuildHasher);
    s.extend([3, 1, 3, 2]);
    let mut items: Vec<_> = s.iter().copied().collect();
    items.sort();
    println!("{items:?} {}", s.contains(&2));
}
