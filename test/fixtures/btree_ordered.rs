// `BTreeMap` / `BTreeSet` keep a sorted array next to the hash map: single inserts go in by
// binary search, bulk loads are sorted at the next ordered read, `pop_first` moves a head index.
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let mut m: BTreeMap<u32, &str> = BTreeMap::new();
    for (k, v) in [(5, "e"), (1, "a"), (3, "c"), (9, "i"), (7, "g")] {
        m.insert(k, v);
    }
    println!("{:?}", m);
    println!("{:?} {:?}", m.first_key_value(), m.last_key_value());
    println!("{:?} {:?}", m.pop_first(), m.pop_last());
    m.insert(0, "zero");
    m.insert(4, "d");
    m.insert(3, "C");
    println!("{:?} {}", m, m.len());
    println!("{:?}", m.range(2..5).collect::<Vec<_>>());
    println!("{:?}", m.range(3..=7).map(|(k, _)| *k).collect::<Vec<_>>());
    println!("{:?}", m.range(4..).count());
    println!("{:?}", m.range(..4).map(|(_, v)| *v).collect::<Vec<_>>());
    println!("{:?}", m.range(6..6).count());
    *m.entry(4).or_insert("x") = "D";
    m.entry(8).or_insert("h");
    m.entry(2).and_modify(|v| *v = "no").or_insert("b");
    println!("{:?}", m.keys().collect::<Vec<_>>());
    println!("{:?}", m.values().collect::<Vec<_>>());
    println!("{:?} {:?} {:?}", m.remove(&3), m.remove(&3), m.get(&4));
    m.retain(|k, _| k % 2 == 0);
    println!("{:?}", m);
    let mut other = BTreeMap::from([(1, "one"), (8, "eight"), (6, "six")]);
    m.append(&mut other);
    println!("{:?} {:?}", m, other);
    let c = m.clone();
    m.insert(100, "big");
    println!("{:?} {} {}", c, c == m, c.len());
    for (k, v) in &m {
        print!("{}={} ", k, v);
    }
    println!();
    for v in m.values_mut() {
        *v = "w";
    }
    println!("{:?}", m.iter().next());
    let mut it = m.iter();
    it.next();
    println!("{:?}", it.next());
    while let Some((k, v)) = m.pop_first() {
        print!("{}{} ", k, v);
    }
    println!("{} {:?}", m.len(), m.first_key_value());
    println!("{:?}", m.pop_last());

    // A bulk load in descending order (past the binary-insert budget), then ordered reads and
    // more single inserts.
    let mut bulk: BTreeMap<i32, i32> = BTreeMap::new();
    for i in (0..2000).rev() {
        bulk.insert(i * 3, i);
    }
    println!("{:?} {:?} {}", bulk.first_key_value(), bulk.last_key_value(), bulk.len());
    bulk.insert(-1, -1);
    bulk.insert(10, 10);
    bulk.insert(5998, 0);
    println!("{:?}", bulk.range(-2..12).collect::<Vec<_>>());
    let mut acc = 0i64;
    for _ in 0..1500 {
        let (k, v) = bulk.pop_first().unwrap();
        acc += (k as i64) * (v as i64);
    }
    println!("{} {:?} {}", acc, bulk.first_key_value(), bulk.len());
    bulk.insert(1, 1);
    bulk.insert(1500 * 3 + 1, 2);
    println!("{:?}", bulk.iter().take(3).collect::<Vec<_>>());
    let keys: Vec<i32> = bulk.keys().copied().collect();
    println!("{} {} {}", keys.len(), keys.windows(2).all(|w| w[0] < w[1]), keys[keys.len() - 1]);
    let collected: BTreeMap<String, usize> = ["pear", "apple", "fig", "apple"]
        .iter()
        .map(|s| (s.to_string(), s.len()))
        .collect();
    println!("{:?}", collected);
    let mut counts: BTreeMap<char, u32> = BTreeMap::new();
    for ch in "hello world".chars() {
        *counts.entry(ch).or_insert(0) += 1;
    }
    println!("{:?}", counts);
    println!("{:?}", counts.range('e'..'m').collect::<Vec<_>>());

    let mut s: BTreeSet<i32> = BTreeSet::new();
    for x in [5, -2, 9, 0, 5, 3] {
        print!("{} ", s.insert(x));
    }
    println!("{:?} {}", s, s.len());
    println!("{:?} {:?}", s.first(), s.last());
    println!("{:?} {:?} {:?}", s.pop_first(), s.pop_last(), s);
    println!("{} {} {:?}", s.remove(&0), s.remove(&0), s);
    s.insert(0);
    s.insert(-5);
    println!("{:?}", s.range(-1..=3).collect::<Vec<_>>());
    println!("{:?}", s.iter().rev().collect::<Vec<_>>());
    let t = BTreeSet::from([3, 4, 100]);
    println!("{:?}", s.union(&t).collect::<Vec<_>>());
    println!("{:?}", s.intersection(&t).collect::<Vec<_>>());
    println!("{:?}", s.difference(&t).collect::<Vec<_>>());
    println!("{} {}", s.is_subset(&t), BTreeSet::from([3]).is_subset(&t));
    s.retain(|x| *x >= 0);
    println!("{:?} {} {}", s, s.contains(&3), s.contains(&4));
    let mut words: BTreeSet<String> = ["delta", "alpha", "charlie"].iter().map(|s| s.to_string()).collect();
    words.insert("bravo".to_string());
    println!("{:?} {:?}", words, words.first());
    let pairs: BTreeSet<(i32, i32)> = [(2, 1), (1, 9), (1, 2), (2, 0)].into_iter().collect();
    println!("{:?} {:?}", pairs, pairs.range((1, 5)..(2, 1)).collect::<Vec<_>>());
    // Insert + pop_first churn with a growing set (a scheduler).
    let mut sched: BTreeSet<(u64, u32)> = BTreeSet::new();
    let mut done = 0u64;
    for i in 0..3000u32 {
        sched.insert(((i as u64 * 7919) % 1000, i));
        sched.insert(((i as u64 * 104729) % 1000 + 5000, i));
        let (t, id) = sched.pop_first().unwrap();
        done += t + id as u64;
    }
    println!("{} {} {:?} {:?}", done, sched.len(), sched.first(), sched.last());
    let mut empty: BTreeSet<u8> = BTreeSet::new();
    println!("{:?}", empty.pop_first());
    println!("{:?} {:?}", empty.first(), empty.range(1..3).next());
    println!("{}", empty.remove(&1));
    empty.insert(7);
    empty.clear();
    empty.insert(2);
    empty.insert(1);
    println!("{:?}", empty);
}
