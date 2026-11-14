// `std::cmp::Reverse`: a min-heap from `BinaryHeap`, destructured in `while let`, compared,
// printed and used inside tuples, as in Dijkstra's shortest paths.
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

fn dijkstra(edges: &HashMap<u32, Vec<(u32, u64)>>, start: u32) -> Vec<(u32, u64)> {
    let mut dist: HashMap<u32, u64> = HashMap::new();
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0u64, start)));
    while let Some(Reverse((d, node))) = heap.pop() {
        if dist.contains_key(&node) {
            continue;
        }
        dist.insert(node, d);
        if let Some(next) = edges.get(&node) {
            for &(to, w) in next {
                if !dist.contains_key(&to) {
                    heap.push(Reverse((d + w, to)));
                }
            }
        }
    }
    let mut out: Vec<(u32, u64)> = dist.into_iter().collect();
    out.sort();
    out
}

fn main() {
    let mut mins = BinaryHeap::new();
    for x in [5, 1, 8, 3, 2] {
        mins.push(Reverse(x));
    }
    println!("{:?} peek={:?}", mins, mins.peek());
    let mut order = Vec::new();
    while let Some(Reverse(x)) = mins.pop() {
        order.push(x);
    }
    println!("{:?}", order);
    println!("{} {} {:?}", Reverse(1) > Reverse(2), Reverse("a") == Reverse("a"), Reverse(3).0);
    let mut words = vec![Reverse("pear"), Reverse("apple"), Reverse("fig")];
    words.sort();
    println!("{:?}", words);

    let mut edges = HashMap::new();
    edges.insert(1, vec![(2, 7), (3, 9), (6, 14)]);
    edges.insert(2, vec![(3, 10), (4, 15)]);
    edges.insert(3, vec![(4, 11), (6, 2)]);
    edges.insert(4, vec![(5, 6)]);
    edges.insert(6, vec![(5, 9)]);
    println!("{:?}", dijkstra(&edges, 1));
}
