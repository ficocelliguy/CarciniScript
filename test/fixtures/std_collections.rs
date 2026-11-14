// `VecDeque`, `BinaryHeap`, `BTreeMap` and `BTreeSet`: construction (`new`, `from`, `collect`,
// `Default`), each kind's own methods, the methods shared with `Vec` / `HashMap` / `HashSet`,
// iteration order (B-trees sorted, heaps in std's internal order), `{:?}`, `entry`, indexing,
// cloning, equality, and a BFS over a small graph like the ones Bitburner scripts scan.
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, VecDeque};

#[derive(Debug, Default)]
struct Inventory {
    counts: BTreeMap<String, u32>,
    tags: BTreeSet<char>,
    queue: VecDeque<i32>,
}

fn bfs(edges: &HashMap<&str, Vec<&str>>, start: &str) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(start.to_string());
    seen.insert(start.to_string());
    while let Some(host) = queue.pop_front() {
        order.push(host.clone());
        if let Some(next) = edges.get(host.as_str()) {
            for n in next {
                if seen.insert(n.to_string()) {
                    queue.push_back(n.to_string());
                }
            }
        }
    }
    order
}

fn main() {
    // VecDeque
    let mut dq: VecDeque<i32> = VecDeque::new();
    dq.push_back(2);
    dq.push_back(3);
    dq.push_front(1);
    dq.push_front(0);
    println!("{:?} len={} front={:?} back={:?}", dq, dq.len(), dq.front(), dq.back());
    println!("{:?} {:?} {}", dq.pop_front(), dq.pop_back(), dq[1]);
    dq.insert(1, 9);
    let (r0, r10) = (dq.remove(0), dq.remove(10));
    println!("{:?} remove={:?} remove_oob={:?}", dq, r0, r10);
    let mut other = VecDeque::from(vec![7, 8]);
    dq.append(&mut other);
    dq.retain(|x| *x != 8);
    println!("{:?} {:?} contains9={} sum={}", dq, other, dq.contains(&9), dq.iter().sum::<i32>());
    dq.make_contiguous().sort();
    let doubled: VecDeque<i32> = dq.iter().map(|x| x * 2).collect();
    println!("{:?} {:?} eq={}", dq, doubled, dq == dq.clone());
    let mut empty: VecDeque<String> = VecDeque::new();
    println!("{:?} {:?} {}", empty.pop_front(), empty.front(), empty.is_empty());

    // BinaryHeap: std's internal order shows in {:?} and iter().
    let mut heap = BinaryHeap::new();
    for x in [3, 1, 4, 1, 5, 9, 2, 6] {
        heap.push(x);
    }
    println!("{:?} peek={:?} len={}", heap, heap.peek(), heap.len());
    let mut popped = Vec::new();
    while let Some(x) = heap.pop() {
        popped.push(x);
        if popped.len() == 3 {
            println!("after 3 pops: {:?}", heap);
        }
    }
    println!("{:?}", popped);
    let mut h2 = BinaryHeap::from(vec![5, 3, 8, 1, 9, 2]);
    println!("{:?}", h2);
    h2.extend(vec![7, 4, 6]);
    println!("{:?}", h2);
    let mut h3: BinaryHeap<i32> = (10..14).collect();
    h2.append(&mut h3);
    println!("{:?} {:?}", h2, h3);
    println!("{:?}", h2.clone().into_sorted_vec());
    let words: BinaryHeap<String> = ["pear", "apple", "fig"].iter().map(|s| s.to_string()).collect();
    println!("{:?}", words.into_sorted_vec());

    // BTreeMap: sorted whatever the insertion order, entry slots, first/last/pop.
    let mut scores: BTreeMap<String, i32> = BTreeMap::new();
    for (name, s) in [("zed", 3), ("amy", 7), ("kim", 5), ("amy", 9)] {
        *scores.entry(name.to_string()).or_insert(0) += s;
    }
    println!("{:?}", scores);
    println!("{:?} {:?} {}", scores.first_key_value(), scores.last_key_value(), scores["kim"]);
    scores.insert("bob".to_string(), 1);
    scores.remove("zed");
    let names: Vec<&String> = scores.keys().collect();
    println!("{:?} {:?}", names, scores.values().collect::<Vec<_>>());
    for (k, v) in &scores {
        print!("{k}={v} ");
    }
    println!();
    println!("{:?} {:?}", scores.pop_first(), scores);
    let squares: BTreeMap<i32, i32> = (1..5).rev().map(|i| (i, i * i)).collect();
    println!("{:?} {}", squares, squares.contains_key(&3));
    let from = BTreeMap::from([(3, 'c'), (1, 'a'), (2, 'b')]);
    println!("{:?} {:?}", from, from.get(&2));

    // BTreeSet
    let mut set: BTreeSet<i32> = [5, 1, 9, 3].into_iter().collect();
    set.insert(4);
    println!("{:?} first={:?} last={:?}", set, set.first(), set.last());
    let other: BTreeSet<i32> = BTreeSet::from([3, 4, 10, 0]);
    println!("{:?}", set.union(&other).collect::<Vec<_>>());
    println!("{:?}", set.intersection(&other).collect::<Vec<_>>());
    println!("{:?}", set.difference(&other).collect::<Vec<_>>());
    println!("{:?} {:?} {:?}", set.pop_last(), set.pop_first(), set);
    let letters: BTreeSet<char> = "banana".chars().collect();
    println!("{:?} {}", letters, letters.iter().collect::<String>());

    // In a struct with Default
    let mut inv = Inventory::default();
    inv.counts.insert("ore".to_string(), 2);
    inv.tags.insert('z');
    inv.tags.insert('a');
    inv.queue.push_back(1);
    println!("{:?}", inv);

    let mut edges = HashMap::new();
    edges.insert("home", vec!["n00dles", "foodnstuff", "sigma"]);
    edges.insert("n00dles", vec!["home", "zer0"]);
    edges.insert("sigma", vec!["home", "max-hardware", "zer0"]);
    edges.insert("zer0", vec!["silver-helix"]);
    println!("{:?}", bfs(&edges, "home"));
}
