// `S::default()` / `Default::default()` for the hasher parameter of an impl for `HashMap<K, V, S>`
// / `HashSet<T, S>` (winnow's `Accumulate`): the map types leave `S` out, and a map ignores its
// hasher, so it is `RandomState::default()`.
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hash};

trait Accumulate<T>: Sized {
    fn initial(capacity: Option<usize>) -> Self;
    fn accumulate(&mut self, acc: T);
}

impl<K, V, S> Accumulate<(K, V)> for HashMap<K, V, S>
where
    K: Eq + Hash,
    S: BuildHasher + Default,
{
    fn initial(capacity: Option<usize>) -> Self {
        let h = S::default();
        match capacity {
            Some(capacity) => HashMap::with_capacity_and_hasher(capacity, h),
            None => HashMap::with_hasher(h),
        }
    }
    fn accumulate(&mut self, (k, v): (K, V)) {
        self.insert(k, v);
    }
}

fn collect<A: Accumulate<(String, u32)>>(items: &[(&str, u32)]) -> A {
    let mut a = A::initial(Some(items.len()));
    for (k, v) in items {
        a.accumulate((k.to_string(), *v));
    }
    a
}

trait Make: Sized {
    fn make() -> Self;
    fn make2() -> Self;
}
impl<T: Eq + Hash, S: BuildHasher + Default> Make for HashSet<T, S> {
    fn make() -> Self {
        HashSet::with_hasher(S::default())
    }
    fn make2() -> Self {
        let s: S = Default::default();
        HashSet::with_capacity_and_hasher(2, s)
    }
}
fn main() {
    let mut m: HashMap<String, u32> = Accumulate::initial(None);
    m.accumulate(("a".to_string(), 1));
    m.accumulate(("b".to_string(), 2));
    let mut keys: Vec<_> = m.iter().collect();
    keys.sort();
    println!("{:?}", keys);
    let m2: HashMap<String, u32> = collect(&[("x", 9), ("y", 8)]);
    let mut v: Vec<_> = m2.into_iter().collect();
    v.sort();
    println!("{:?}", v);
    let m3 = <HashMap<u8, u8> as Accumulate<(u8, u8)>>::initial(Some(4));
    println!("{}", m3.len());
    let mut s: HashSet<u8> = Make::make();
    s.insert(3);
    let mut t = <HashSet<&str> as Make>::make2();
    t.insert("x");
    println!("{} {}", s.len(), t.len());
}
