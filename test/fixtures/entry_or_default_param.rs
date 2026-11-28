// `map.entry(k).or_default()` whose value is a `V: Default` type parameter: in a generic fn and in
// a method of `impl<V: Default + ..> Tr for HashMap<u8, V>`, the default by `V`'s class.
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::AddAssign;

fn bump<V: Default + AddAssign<u8>>(m: &mut HashMap<u8, V>) {
    *m.entry(1).or_default() += 1;
}

fn get_or_make<K: Hash + Eq, V: Default + Clone>(m: &mut HashMap<K, V>, k: K) -> V {
    m.entry(k).or_default().clone()
}

trait Tr {
    fn bump2(&mut self);
}
impl<V: Default + AddAssign<u8>> Tr for HashMap<u8, V> {
    fn bump2(&mut self) {
        *self.entry(2).or_default() += 3;
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
struct S {
    n: u8,
    name: String,
}

fn sorted<V: std::fmt::Debug + Clone>(m: &HashMap<u8, V>) -> Vec<(u8, V)> {
    let mut v: Vec<(u8, V)> = m.iter().map(|(k, v)| (*k, v.clone())).collect();
    v.sort_by_key(|p| p.0);
    v
}

fn main() {
    let mut m: HashMap<u8, u8> = HashMap::new();
    bump(&mut m);
    bump(&mut m);
    m.bump2();
    println!("{:?}", sorted(&m));
    let mut s: HashMap<u8, S> = HashMap::new();
    let made = get_or_make(&mut s, 7);
    println!("{:?} {}", made, s.len());
    let mut t: HashMap<&str, String> = HashMap::new();
    let e = get_or_make(&mut t, "a");
    println!("{:?} {:?}", e, t.get("a"));
    let mut u: HashMap<u8, Vec<u8>> = HashMap::new();
    let ev = get_or_make(&mut u, 1);
    println!("{:?} {}", ev, u.len());
}
