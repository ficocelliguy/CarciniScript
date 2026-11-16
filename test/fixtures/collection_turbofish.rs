// Collections named with a turbofish: `HashMap::<K, V>::new()` and the others.
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque, BTreeSet};
fn main() {
    let h = HashMap::<u8, u8>::new();
    let s = HashSet::<char>::new();
    let q = VecDeque::<u8>::new();
    let b = BTreeMap::<u8, String>::new();
    let t = BTreeSet::<i64>::new();
    let v = Vec::<u8>::with_capacity(3);
    println!("{} {} {} {} {} {}", h.is_empty(), s.len(), q.len(), b.len(), t.len(), v.len());
}
