// Impls of one trait for a map with a hasher and without (one type in JS: hashers don't exist
// there), whose bodies differ only in building it with the hasher (ahash's `HashMapExt`): one
// impl, std's default's. And `HashMap::new()` beside the trait's `new` is std's.
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

type Fast = BuildHasherDefault<DefaultHasher>;

pub trait HashMapExt {
    fn new() -> Self;
    fn with_capacity(n: usize) -> Self;
}
impl<K, V> HashMapExt for HashMap<K, V, Fast> {
    fn new() -> Self { Self::with_hasher(Default::default()) }
    fn with_capacity(n: usize) -> Self { Self::with_capacity_and_hasher(n, Fast::default()) }
}
impl<K, V> HashMapExt for HashMap<K, V> {
    fn new() -> Self { HashMap::new() }
    fn with_capacity(n: usize) -> Self { HashMap::with_capacity(n) }
}
fn main() {
    let mut a: HashMap<u8, u8, Fast> = HashMapExt::new();
    let mut b: HashMap<u8, u8> = HashMapExt::with_capacity(4);
    a.insert(1, 2);
    b.insert(3, 4);
    let mut c: HashMap<u8, u8> = HashMap::new();
    c.insert(5, 6);
    println!("{:?} {:?} {:?}", a.get(&1), b.get(&3), c.get(&5));
}
