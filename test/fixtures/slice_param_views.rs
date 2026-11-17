// `f(&v[a..b])` passes a view (no copy) when `f`'s `&[T]` parameter only reads a little of it;
// functions that loop over it by index, alias it, compare it or pass it on to such functions
// still get a copy. Either way the results are the same.
#[derive(Debug)]
struct Keep<'a> {
    part: &'a [i32],
}

fn sum(v: &[i64]) -> i64 {
    if v.is_empty() { 0 } else { v[0] + sum(&v[1..]) }
}

fn bsearch(v: &[u32], x: u32) -> Option<usize> {
    if v.is_empty() {
        return None;
    }
    let m = v.len() / 2;
    if v[m] == x {
        Some(m)
    } else if v[m] < x {
        bsearch(&v[m + 1..], x).map(|i| i + m + 1)
    } else {
        bsearch(&v[..m], x)
    }
}

fn head_tail(v: &[i32]) -> String {
    match v {
        [] => "empty".to_string(),
        [x] => format!("one {}", x),
        [first, .., last] => format!("{}..{} ({})", first, last, v.len()),
    }
}

fn indexed_total(v: &[i32]) -> i32 {
    let mut t = 0;
    for i in 0..v.len() {
        t += v[i] * (i as i32 + 1);
    }
    t
}

fn through(v: &[i32]) -> i32 {
    indexed_total(v) + v.len() as i32
}

fn passes_on(v: &[i32]) -> i64 {
    let w: Vec<i64> = v.iter().map(|x| *x as i64).collect();
    sum(&w[..]) + stats(v).0
}

fn stats(v: &[i32]) -> (i64, Option<i32>, Option<i32>, bool, usize) {
    (
        v.iter().map(|x| *x as i64).sum::<i64>(),
        v.first().copied(),
        v.last().copied(),
        v.contains(&7),
        v.windows(2).filter(|w| w[0] < w[1]).count(),
    )
}

fn alias(v: &[i32]) -> i32 {
    let w = v;
    let mut t = 0;
    for x in w {
        t += x;
    }
    t + w[0]
}

fn same(a: &[i32], b: &[i32]) -> bool {
    a == b
}

fn keep(v: &[i32]) -> Keep<'_> {
    Keep { part: v }
}

fn ident(v: &[i32]) -> &[i32] {
    v
}

fn with_closure(v: &[i32]) -> Vec<i32> {
    (0..v.len()).map(|i| v[i] + 1).collect()
}

fn mid<T: Copy>(v: &[T]) -> Option<T> {
    v.get(v.len() / 2).copied()
}

fn nested(v: &[i32], depth: u32) -> i32 {
    if v.len() < 2 || depth == 0 {
        return v.len() as i32;
    }
    nested(&v[1..], depth - 1) + nested(&v[..v.len() - 1], depth - 1)
}

fn strs(v: &[String]) -> String {
    if v.is_empty() { String::new() } else { format!("{}{}", v[0], strs(&v[1..])) }
}

fn prefix(s: &str) -> usize {
    if s.is_empty() { 0 } else { 1 + prefix(&s[1..]) }
}

fn rows(g: &[Vec<i32>]) -> i32 {
    if g.is_empty() { 0 } else { g[0].iter().sum::<i32>() + rows(&g[1..]) }
}

fn main() {
    // (Recursion depth stays under a browser worker's stack.)
    let v: Vec<i64> = (1..=1000).collect();
    println!("{} {} {}", sum(&v), sum(&v[400..]), sum(&v[..0]));
    let w: Vec<u32> = (0..50_000).map(|i| i * 2).collect();
    let mut found = 0;
    let mut where_ = 0usize;
    for x in 0..60_000 {
        if let Some(i) = bsearch(&w[..], x) {
            found += 1;
            where_ += i;
        }
    }
    println!("{} {} {:?} {:?}", found, where_, bsearch(&w[10..20], 24), bsearch(&w[10..20], 25));
    let a = vec![5, 7, 1, 9, 3, 8];
    println!("{} | {} | {} | {}", head_tail(&a[..]), head_tail(&a[2..3]), head_tail(&a[4..4]), head_tail(&a[1..]));
    println!("{} {} {}", indexed_total(&a[1..4]), through(&a[..5]), passes_on(&a[2..]));
    println!("{:?} {:?}", stats(&a[1..]), stats(&a[..1]));
    println!("{} {} {}", alias(&a[2..]), same(&a[..2], &a[..2]), same(&a[..2], &a[1..3]));
    let k = keep(&a[3..]);
    println!("{:?} {:?} {:?}", k, ident(&a[..2]), with_closure(&a[1..3]));
    println!("{:?} {:?} {:?}", mid(&a[..]), mid(&a[..0]), mid(&a[1..2]));
    println!("{} {}", nested(&a[..], 3), nested(&a[2..], 10));
    let names: Vec<String> = ["x", "y", "z"].iter().map(|s| s.to_string()).collect();
    println!("{} {} {}", strs(&names[..]), strs(&names[1..]), prefix(&"hello"[1..]));
    let grid = vec![vec![1, 2], vec![3], vec![4, 5, 6]];
    println!("{} {}", rows(&grid[..]), rows(&grid[1..]));
    // The base changes after the calls: nothing kept a view.
    let mut b = vec![1, 2, 3, 4];
    let s1 = sum(&b.iter().map(|x| *x as i64).collect::<Vec<_>>()[1..]);
    b.push(5);
    b[0] = 10;
    println!("{} {:?} {}", s1, b, indexed_total(&b[..]));
}
