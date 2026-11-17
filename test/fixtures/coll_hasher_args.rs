use indexmap::IndexSet;
use std::collections::hash_map::RandomState;
type Set<K> = IndexSet<K, RandomState>;
fn main() {
    let mut s: Set<&str> = Set::default();
    s.insert("a");
    s.insert("b");
    s.insert("a");
    println!("{:?} {}", s, s.len());
}
