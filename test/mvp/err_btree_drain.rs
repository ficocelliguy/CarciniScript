// Reach for `drain()` on an ordered map to empty it in order (only `HashMap` has one).
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let mut by_level: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    by_level.entry(1).or_default().push("n00dles".to_string());
    by_level.entry(5).or_default().push("sigma-cosmetics".to_string());
    by_level.entry(1).or_default().push("foodnstuff".to_string());

    for (level, hosts) in by_level.drain() {
        println!("{level}: {}", hosts.join(", "));
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    seen.insert("home".to_string());
    let all: Vec<String> = seen.drain().collect();
    println!("{:?}", all);
}
