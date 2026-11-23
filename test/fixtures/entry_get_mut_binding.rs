// A kept `hash_map::Entry`'s `get_mut()` bound to a name: the map's slot for the key, so
// writes through the binding reach the map.
use std::collections::HashMap;
use std::collections::hash_map::Entry;

fn part1() {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("a", 1);
    match m.entry("a") {
        Entry::Occupied(mut e) => { let v = e.get_mut(); *v += 1; *v *= 10; }
        Entry::Vacant(e) => { e.insert(0); }
    }
    if let Entry::Occupied(mut e) = m.entry("a") {
        let v = e.get_mut();
        *v += 3;
        println!("{}", v);
    }
    let mut ms: HashMap<i32, String> = HashMap::new();
    ms.insert(1, "h".to_string());
    if let Entry::Occupied(mut e) = ms.entry(1) { let s = e.get_mut(); s.push('i'); }
    println!("{:?} {:?}", m, ms);
}

fn main() {
    part1();
}
