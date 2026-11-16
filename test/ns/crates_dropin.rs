// The bundled drop-ins that have no native counterpart in the test cache: `hashbrown` (std's maps
// and sets) and `lazy_static!` (a `LazyLock` static).
use hashbrown::{HashMap, HashSet};
use lazy_static::lazy_static;

lazy_static! {
    /// Squares.
    static ref SQUARES: Vec<u64> = (1..=5).map(|x| x * x).collect();
    pub static ref LOOKUP: HashMap<&'static str, i32> = {
        let mut m = HashMap::new();
        m.insert("a", 1);
        m
    };
}

pub async fn main(ns: NS) {
    let mut m: HashMap<String, usize> = HashMap::new();
    *m.entry("x".to_string()).or_insert(0) += 2;
    let s: HashSet<i32> = [1, 2, 2, 3].into_iter().collect();
    let h = hashbrown::HashMap::<u8, u8>::new();
    ns.tprint(&format!("{} {} {} {}", SQUARES.iter().sum::<u64>(), LOOKUP["a"], m["x"], s.len()));
    ns.tprint(&format!("{} {:?}", h.is_empty(), SQUARES.first()));
}
