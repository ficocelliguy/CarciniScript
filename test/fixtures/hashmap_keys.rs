// Composite hash keys: tuples (with negatives, 64-bit integers, strings with quotes, chars,
// bools, nested tuples, unit, Options), Vecs of different lengths, and structs.
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct P {
    x: i32,
    y: i32,
}

fn main() {
    let mut seen: HashSet<(i32, i32)> = HashSet::new();
    let mut dup = 0;
    for x in -3..3 {
        for y in -3..3 {
            if !seen.insert((x, y)) {
                dup += 1;
            }
            if !seen.insert((y, x)) {
                dup += 1;
            }
        }
    }
    println!("{} {} {} {}", seen.len(), dup, seen.contains(&(-3, 2)), seen.contains(&(3, 2)));
    println!("{} {}", seen.contains(&(0 - 0, 0)), seen.remove(&(-1, -1)));
    let mut big: HashMap<(u64, i64), &str> = HashMap::new();
    big.insert((u64::MAX, i64::MIN), "edge");
    big.insert((1, -1), "one");
    big.insert((1, 1), "other");
    println!("{:?} {:?} {:?}", big.get(&(u64::MAX, i64::MIN)), big.get(&(1, -1)), big.get(&(1, 2)));
    let mut names: HashMap<(String, u8), usize> = HashMap::new();
    for key in [("a,b", 1u8), ("a", 1), ("\"q\"", 2), ("\\", 2), ("ü", 3), ("a,b", 1), ("", 0)] {
        *names.entry((key.0.to_string(), key.1)).or_insert(0) += 1;
    }
    let mut ks: Vec<_> = names.iter().map(|((s, n), c)| (s.clone(), *n, *c)).collect();
    ks.sort();
    println!("{:?}", ks);
    let mut nested: HashSet<((i32, bool), (char, &str))> = HashSet::new();
    nested.insert(((1, true), ('x', "y")));
    nested.insert(((1, true), ('x', "y")));
    nested.insert(((1, false), ('x', "y")));
    nested.insert(((1, true), ('\n', "y")));
    println!("{} {}", nested.len(), nested.contains(&((1, true), ('\n', "y"))));
    let mut opts: HashMap<Option<i32>, &str> = HashMap::new();
    opts.insert(None, "none");
    opts.insert(Some(0), "zero");
    opts.insert(Some(-1), "neg");
    println!("{:?} {:?} {:?} {}", opts.get(&None), opts.get(&Some(0)), opts.get(&Some(1)), opts.len());
    let mut oo: HashSet<Option<Option<u8>>> = HashSet::new();
    oo.insert(None);
    oo.insert(Some(None));
    oo.insert(Some(Some(1)));
    oo.insert(Some(None));
    println!("{} {} {}", oo.len(), oo.contains(&Some(None)), oo.contains(&Some(Some(2))));
    let mut tup_opt: HashSet<(Option<i32>, i32)> = HashSet::new();
    tup_opt.insert((None, 1));
    tup_opt.insert((Some(1), 1));
    tup_opt.insert((None, 1));
    println!("{} {}", tup_opt.len(), tup_opt.contains(&(Some(1), 1)));
    let mut units: HashMap<(i32, ()), i32> = HashMap::new();
    units.insert((1, ()), 1);
    units.insert((1, ()), 2);
    println!("{:?} {}", units.get(&(1, ())), units.len());
    let mut vecs: HashSet<Vec<i32>> = HashSet::new();
    vecs.insert(vec![]);
    vecs.insert(vec![1]);
    vecs.insert(vec![1, 2]);
    vecs.insert(vec![1, 2]);
    vecs.insert(vec![12]);
    println!("{} {} {}", vecs.len(), vecs.contains(&vec![1, 2]), vecs.contains(&vec![2, 1]));
    let mut ps: HashSet<P> = HashSet::new();
    let mut hits = 0;
    for i in 0..300 {
        if !ps.insert(P { x: i % 45, y: i % 33 }) {
            hits += 1;
        }
    }
    println!("{} {} {}", ps.len(), hits, ps.contains(&P { x: 1, y: 1 }));
    let mut chars: HashMap<(char, bool), i32> = HashMap::new();
    chars.insert(('a', true), 1);
    chars.insert(('a', false), 2);
    chars.insert(('é', true), 3);
    println!("{:?} {:?} {:?}", chars.get(&('a', true)), chars.get(&('é', true)), chars.get(&('b', true)));
    let mut bools: HashSet<(bool, bool)> = HashSet::new();
    for a in [true, false] {
        for b in [true, false] {
            bools.insert((a, b));
            bools.insert((a, b));
        }
    }
    println!("{}", bools.len());
    let mut strs: HashSet<(String, String)> = HashSet::new();
    strs.insert(("a,b".to_string(), "c".to_string()));
    strs.insert(("a".to_string(), "b,c".to_string()));
    strs.insert(("a\",\"b".to_string(), "c".to_string()));
    println!("{} {}", strs.len(), strs.contains(&("a".to_string(), "b,c".to_string())));
    let mut grid: HashMap<(i32, i32), u32> = HashMap::new();
    for x in 0..40 {
        for y in 0..40 {
            grid.insert((x, y), (x * y) as u32);
        }
    }
    let mut sum = 0u64;
    for x in 0..40 {
        for y in 0..40 {
            sum += grid[&(x, y)] as u64;
        }
    }
    println!("{} {} {:?}", grid.len(), sum, grid.get(&(40, 0)));
    let one: HashSet<(u8, u8)> = [(1, 2)].into_iter().collect();
    let two: HashSet<(u8, u8)> = [(1, 2), (2, 1)].into_iter().collect();
    println!("{} {} {}", one.is_subset(&two), one == two, one.intersection(&two).count());
}
