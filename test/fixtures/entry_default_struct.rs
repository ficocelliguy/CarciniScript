// `map.entry(k).or_default()` with a struct (or enum) value: derived or hand-written `Default`,
// per-key stats maps updated in place.
use std::collections::{BTreeMap, HashMap};

#[derive(Default, Debug)]
struct Stats {
    hits: u32,
    money: f64,
    hosts: Vec<String>,
}

#[derive(Debug, PartialEq)]
struct Limit(u32);

impl Default for Limit {
    fn default() -> Self {
        Limit(10)
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq)]
enum Phase {
    #[default]
    Idle,
    Busy,
}

fn main() {
    let mut by_target: HashMap<&str, Stats> = HashMap::new();
    for (t, m) in [("n00dles", 1.5), ("joes", 2.0), ("n00dles", 3.0)] {
        let s = by_target.entry(t).or_default();
        s.hits += 1;
        s.money += m;
        s.hosts.push(format!("{t}-{}", s.hits));
    }
    let mut keys: Vec<_> = by_target.keys().copied().collect();
    keys.sort();
    for k in keys {
        println!("{k}: {:?}", by_target[k]);
    }
    let mut limits: BTreeMap<u8, Limit> = BTreeMap::new();
    limits.entry(1).or_default().0 += 5;
    limits.entry(2).or_default();
    println!("{limits:?}");
    let mut phases: HashMap<i32, Phase> = HashMap::new();
    *phases.entry(3).or_default() = Phase::Busy;
    let three = *phases.entry(3).or_default();
    let four = *phases.entry(4).or_default();
    println!("{three:?} {four:?} {}", phases.len());
}
