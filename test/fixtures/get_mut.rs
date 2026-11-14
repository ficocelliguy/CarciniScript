// `get_mut` / `first_mut` / `last_mut` on maps and Vecs of numbers, strings and bools: in
// `if let`, `while let` and `match`, as `*m.get_mut(k).unwrap()`, through `let r = ..expect(..)`,
// and only tested with `is_none()`; writes go back into the collection.
use std::collections::HashMap;

fn bump(scores: &mut HashMap<String, i32>, who: &str) {
    if let Some(s) = scores.get_mut(who) {
        *s += 10;
    } else {
        scores.insert(who.to_string(), 1);
    }
}

fn main() {
    let mut scores: HashMap<String, i32> = HashMap::new();
    bump(&mut scores, "a");
    bump(&mut scores, "a");
    bump(&mut scores, "b");
    *scores.get_mut("b").unwrap() *= 7;
    match scores.get_mut("c") {
        Some(v) => *v = 0,
        None => println!("no c"),
    }
    let r = scores.get_mut("a").expect("a exists");
    *r -= 1;
    let mut keys: Vec<_> = scores.iter().collect();
    keys.sort();
    println!("{:?}", keys);

    let mut v = vec![1, 2, 3];
    if let Some(x) = v.get_mut(1) {
        *x *= 10;
    }
    if let Some(f) = v.first_mut() {
        *f = -1;
    }
    *v.last_mut().unwrap() += 100;
    let missing = v.get_mut(9).is_none();
    println!("{:?} {}", v, missing);
    let mut names = vec![String::from("x")];
    if let Some(n) = names.get_mut(0) {
        n.push('!');
        *n = n.to_uppercase();
    }
    let mut flags = HashMap::new();
    flags.insert(1, false);
    while let Some(f) = flags.get_mut(&1) {
        if *f {
            break;
        }
        *f = true;
    }
    println!("{:?} {:?}", names, flags);
}
