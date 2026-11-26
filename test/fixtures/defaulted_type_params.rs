// A struct's defaulted type parameter (`struct Map<K, S = RandomState>`): written `Map<K>` as a
// type (in `impl<K> Map<K>` or a `let`), it's the default, so `Default::default()` for its field
// knows what to make. `RandomState` is a `BuildHasher` (`build_hasher`, `hash_one`), also
// through an `S: BuildHasher` parameter.
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};

pub struct Map<K, S = RandomState> {
    keys: Vec<K>,
    hasher: S,
}

impl<K> Map<K> {
    pub fn new() -> Self {
        Map { keys: Vec::new(), hasher: Default::default() }
    }
}

impl<K: Hash, S: BuildHasher> Map<K, S> {
    pub fn push(&mut self, k: K) -> u64 {
        let mut h = self.hasher.build_hasher();
        k.hash(&mut h);
        self.keys.push(k);
        h.finish()
    }
    pub fn one(&self, k: &K) -> u64 {
        self.hasher.hash_one(k)
    }
    pub fn len(&self) -> usize {
        self.keys.len()
    }
}

#[derive(Debug)]
struct Counter<T = i32> {
    n: T,
}

impl Counter {
    fn bump(&mut self) {
        self.n += 1;
    }
}

#[derive(Debug, Default)]
struct Pair<A, B = A> {
    a: A,
    b: B,
}

fn count<S: BuildHasher>(m: &HashMap<&str, i32, S>) -> usize {
    m.len()
}

fn main() {
    let mut m: Map<&str> = Map::new();
    let a = m.push("x");
    let b = m.push("x");
    let c = m.push("y");
    println!("{} {} {}", m.len(), a == b, a == c);
    println!("{}", m.one(&"x") == m.one(&"x"));

    let mut c: Counter = Counter { n: Default::default() };
    c.bump();
    println!("{:?}", c);
    let p: Pair<u8> = Pair::default();
    println!("{:?}", p);

    let s = RandomState::new();
    let t: RandomState = Default::default();
    let u = t.clone();
    println!("{}", s.hash_one(5u32) == s.hash_one(5u32));
    println!("{}", u.hash_one("q") == u.hash_one("q"));
    let mut h = s.build_hasher();
    "x".hash(&mut h);
    let _ = h.finish();
    println!("{:?}", t);

    let mut hm: HashMap<&str, i32, RandomState> = HashMap::with_hasher(RandomState::new());
    hm.insert("a", 1);
    println!("{}", count(&hm));
}
