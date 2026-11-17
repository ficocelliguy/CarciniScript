// Closure adapters on `iter_mut()` over numbers, strings and bools (slot iterators): `filter`,
// `map`, `find` (also kept in a local), `enumerate().filter(..)`, `for` over a filtered one,
// `flat_map(|row| row.iter_mut())`, `max()`, `rev().skip_while(..)`, `sum` of a `map`.
fn main() {
    let mut v = vec![1, 5, 2, 8];
    v.iter_mut().filter(|x| **x > 2).for_each(|x| *x *= 10);
    println!("{v:?}");
    let mut names = vec![String::from("a"), String::from("bb")];
    for s in names.iter_mut().filter(|s| s.len() > 1) {
        s.push('!');
    }
    println!("{names:?}");
    let mut w = vec![1.5, 2.5];
    w.iter_mut().map(|x| { *x += 1.0; *x }).count();
    let big = v.iter_mut().find(|x| **x > 40);
    if let Some(b) = big {
        *b = 0;
    }
    let mut flags = vec![false, true, false];
    for (i, f) in flags.iter_mut().enumerate().filter(|(i, _)| i % 2 == 0) {
        *f = i == 0;
    }
    let total: i32 = v.iter_mut().map(|x| { *x += 1; *x }).sum();
    println!("{w:?} {v:?} {flags:?} {total}");
    let mut grid = vec![vec![0u8; 3]; 2];
    grid.iter_mut().flat_map(|row| row.iter_mut()).take(4).for_each(|c| *c = 7);
    println!("{grid:?}");
    let mut xs = [3, 1, 2];
    if let Some(m) = xs.iter_mut().max() {
        *m = 99;
    }
    xs.iter_mut().rev().skip_while(|x| **x < 3).for_each(|x| *x += 100);
    println!("{xs:?}");
    let mut v2 = vec![1, 2];
    let copies: Vec<i32> = v2.iter_mut().map(|x| *x).collect();
    let mut it = v2.iter_mut().filter(|x| **x > 1);
    if let Some(x) = it.next() {
        *x = 20;
    }
    println!("{copies:?} {v2:?}");
}
