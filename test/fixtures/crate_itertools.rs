// The bundled `itertools` crate's `Itertools` methods, compared with the real crate.
use itertools::Itertools;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
struct Item {
    kind: char,
    n: i32,
}

fn main() {
    let v = vec![3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5];
    println!("{}", v.iter().join(", "));
    println!("{:?}", v.iter().sorted().collect_vec());
    println!("{:?}", v.iter().sorted_by(|a, b| b.cmp(a)).collect_vec());
    println!("{:?}", v.iter().sorted_by_key(|x| (*x % 3, **x)).collect_vec());
    println!("{:?}", v.iter().unique().collect_vec());
    println!("{:?}", v.iter().unique_by(|x| *x % 4).collect_vec());
    println!("{:?}", vec![1, 1, 2, 2, 2, 3, 1].into_iter().dedup().collect_vec());
    println!("{:?}", v.iter().duplicates().collect_vec());
    println!("{:?}", v.iter().tuple_windows().map(|(a, b)| b - a).collect_vec());
    for (a, b, c) in (1..6).tuple_windows() {
        print!("{}{}{} ", a, b, c);
    }
    println!();
    println!("{:?}", (1..8).tuples::<(_, _)>().collect_vec());
    println!("{:?}", (1..5).combinations(2).collect_vec());
    println!("{:?}", (1..4).permutations(2).collect_vec());
    println!("{:?}", (1..4).combinations_with_replacement(2).collect_vec());
    println!("{:?}", (1..4).powerset().collect_vec());
    println!("{:?}", (1..3).cartesian_product("ab".chars()).collect_vec());
    println!("{:?}", vec![vec![1, 2], vec![3, 4]].into_iter().multi_cartesian_product().collect_vec());
    println!("{:?}", (1..4).interleave(10..15).collect_vec());
    println!("{:?}", (1..4).intersperse(0).collect_vec());
    println!("{:?}", vec![1, 4, 7].into_iter().merge(vec![2, 3, 9]).collect_vec());
    println!("{:?}", (1..10).take_while_inclusive(|x| *x < 4).collect_vec());
    println!("{:?}", v.iter().positions(|x| *x == 5).collect_vec());
    let counts: HashMap<i32, usize> = v.iter().copied().counts();
    let mut cs = counts.into_iter().collect_vec();
    cs.sort();
    println!("{:?}", cs);
    println!("{} {} {}", [2, 2, 2].iter().all_equal(), v.iter().all_unique(), (1..5).all_unique());
    println!("{} {:?}", v.iter().contains(&9), v.iter().find_position(|x| **x > 4));
    println!("{:?} {:?}", v.iter().k_smallest(3).collect_vec(), v.iter().copied().k_largest(2).collect_vec());
    let items = vec![
        Item { kind: 'a', n: 1 },
        Item { kind: 'a', n: 2 },
        Item { kind: 'b', n: 3 },
        Item { kind: 'a', n: 4 },
    ];
    for (k, g) in &items.iter().chunk_by(|i| i.kind) {
        println!("{} {:?}", k, g.map(|i| i.n).collect_vec());
    }
    let gm = items.iter().map(|i| (i.kind, i.n)).into_group_map();
    let mut keys = gm.keys().copied().collect_vec();
    keys.sort();
    for k in keys {
        println!("{} {:?}", k, gm[&k]);
    }
    let by = items.iter().into_group_map_by(|i| i.n % 2);
    println!("{:?}", by[&0].iter().map(|i| i.n).collect_vec());
    let (a, b) = "x=5".split('=').collect_tuple().unwrap();
    println!("{} {}", a, b);
    println!("{:?}", "1 2 3".split(' ').collect_tuple::<(_, _)>());
    println!("{:?}", (1..10).dropping(7).collect_vec());
    println!("{:?}", itertools::repeat_n("ab", 3).collect_vec());
    println!("{}", (1..4).map(|x| x * x).format("-"));
}
