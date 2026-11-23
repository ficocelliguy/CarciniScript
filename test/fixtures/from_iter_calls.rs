// `X::from_iter(..)` called as a function, as `collect()`.
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};
use std::iter::FromIterator;

fn main() {
    let v = Vec::from_iter(1..4);
    let s = String::from_iter(['a', 'b']);
    let words = String::from_iter(vec!["x", "y"]);
    let h: HashSet<u8> = HashSet::from_iter(vec![1, 1, 2]);
    let b = BTreeSet::from_iter([3, 1, 2]);
    let m: HashMap<&str, u8> = HashMap::from_iter([("a", 1)]);
    let bm = BTreeMap::from_iter([(2, 'b'), (1, 'a')]);
    let d = VecDeque::from_iter(v.iter().copied());
    let heap = BinaryHeap::from_iter([1, 5, 3]);
    let v2: Vec<u8> = FromIterator::from_iter([7u8]);
    let v3 = <Vec<i32>>::from_iter(1..3);
    let v4 = Vec::<Vec<u8>>::from_iter([vec![1]],);
    let m2 = std::collections::HashMap::<u8, Vec<u8>>::from_iter([(1, vec![2])]);
    println!("{:?} {} {} {} {:?} {:?} {:?}", v, s, words, h.len(), b, m, bm);
    println!("{:?} {:?} {:?} {:?} {:?} {:?}", d, heap.peek(), v2, v3, v4, m2);
    println!("{:?}", BTreeSet::from_iter("hello".chars()));
}
