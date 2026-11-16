// The bundled `indexmap`: `IndexMap` / `IndexSet` keep insertion order (so a stats table prints the
// same way every run), with positions (`get_index`, `get_index_of`, `get_full`, `insert_full`),
// `swap_remove` / `shift_remove` (and `remove`, which is `swap_remove`), `first` / `last` /
// `pop`, sorting, `reverse`, `truncate`, `entry`, collecting, `indexmap! {..}` / `indexset! {..}`
// and `{:?}`.
use indexmap::{indexmap, indexset, IndexMap, IndexSet};

fn main() {
    let mut money: IndexMap<&str, f64> = IndexMap::new();
    for (host, m) in [("n00dles", 1.5), ("joesguns", 30.0), ("foodnstuff", 2.25), ("sigma-cosmetics", 12.0)] {
        money.insert(host, m);
    }
    money.insert("n00dles", 1.75);
    println!("{:?}", money);
    println!("{:?} {:?} {:?}", money.get_index(1), money.get_index_of("foodnstuff"), money.get_full("joesguns"));
    println!("{:?} {:?}", money.first(), money.last());
    let (i, old) = money.insert_full("harakiri-sushi", 8.0);
    println!("{i} {old:?} {:?}", money.insert_full("joesguns", 31.0));

    let mut a = money.clone();
    let r = a.swap_remove("n00dles");
    println!("{:?} {:?}", r, a.keys().collect::<Vec<_>>());
    let mut b = money.clone();
    let r = b.shift_remove("n00dles");
    println!("{:?} {:?}", r, b.keys().collect::<Vec<_>>());
    let mut c = money.clone();
    println!("{:?} {:?} {:?}", c.swap_remove_index(0), c.shift_remove_index(1), c.pop());
    println!("{:?}", c);

    money.sort_by(|_, v1, _, v2| v2.partial_cmp(v1).unwrap());
    println!("{:?}", money.iter().map(|(h, m)| format!("{h}={m}")).collect::<Vec<_>>());
    money.sort_keys();
    println!("{:?}", money.keys().collect::<Vec<_>>());
    money.reverse();
    money.truncate(3);
    println!("{:?} {}", money, money.len());

    let mut counts: IndexMap<char, usize> = IndexMap::new();
    for ch in "hello world".chars().filter(|c| c.is_alphabetic()) {
        *counts.entry(ch).or_insert(0) += 1;
    }
    println!("{:?}", counts);
    let squares: IndexMap<u32, u32> = (1..=4).rev().map(|x| (x, x * x)).collect();
    println!("{:?} {:?}", squares, squares.values().sum::<u32>());

    let mut seen: IndexSet<&str> = IndexSet::new();
    for s in ["b", "a", "c", "a", "d"] {
        seen.insert(s);
    }
    println!("{:?} {:?} {:?}", seen, seen.get_index(2), seen.get_index_of("c"));
    println!("{:?}", seen.insert_full("a"));
    let mut s2 = seen.clone();
    let r = s2.swap_remove("b");
    println!("{} {:?}", r, s2);
    let mut s3 = seen.clone();
    let r = s3.shift_remove("b");
    let p = s3.pop();
    println!("{} {:?} {:?}", r, s3, p);
    s3.sort();
    println!("{:?} {:?} {:?}", s3, s3.first(), s3.last());
    let set: IndexSet<i32> = [5, 3, 5, 1].into_iter().collect();
    println!("{:?} {}", set, set.contains(&3));
    let lit = indexmap! { "z" => 1, "a" => 2, "m" => (3 + 4) };
    let empty: IndexMap<u8, u8> = indexmap! {};
    let tags = indexset! { "x", "y", "x" };
    println!("{:?} {:?} {:?} {:?}", lit, lit.get_index(2), empty, tags);
}
