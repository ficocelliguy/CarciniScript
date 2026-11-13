// Ordering, sorting with closures, Option closure methods, retain/dedup, set operations and the
// HashMap methods closures unblock.
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

fn classify(a: i32, b: i32) -> &'static str {
    match a.cmp(&b) {
        Ordering::Less => "less",
        Ordering::Equal => "equal",
        Ordering::Greater => "greater",
    }
}

fn sorted(it: Vec<&i32>) -> Vec<i32> {
    let mut v: Vec<i32> = it.into_iter().copied().collect();
    v.sort();
    v
}

fn main() {
    println!("{} {} {}", classify(1, 2), classify(2, 2), classify(3, 2));
    println!("{:?} {:?}", 1.cmp(&2).reverse(), "b".cmp("a"));
    println!("{:?}", 1.cmp(&1).then(2.cmp(&1)));
    println!("{:?} {:?}", 2.5f64.partial_cmp(&1.0), f64::NAN.partial_cmp(&1.0));
    println!("{}", 3.cmp(&4).is_lt());

    let mut people = vec![("bob", 30), ("ada", 36), ("cy", 30), ("dee", 25)];
    people.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    println!("{:?}", people);
    people.sort_by_key(|p| p.0.len());
    println!("{:?}", people);

    let mut fs = vec![2.5, -1.0, 9.75, 0.5];
    fs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("{:?}", fs);
    let biggest = fs.iter().max_by(|a, b| a.partial_cmp(b).unwrap());
    let top = fs.iter().fold(f64::MIN, |m, &x| m.max(x));
    println!("{} {:?}", top, biggest);

    let mut v = vec![1, 1, 2, 3, 3, 3, 4, 1, 1];
    v.dedup();
    println!("{:?}", v);
    v.retain(|&x| x != 3);
    println!("{:?}", v);
    let mut words = vec!["apple", "avocado", "banana", "blueberry", "cherry"];
    words.dedup_by_key(|w| w.len() > 6);
    println!("{:?}", words);
    let mut scores = vec![5, 3, 8];
    scores.iter_mut().for_each(|s| *s *= 10);
    scores.iter_mut().enumerate().for_each(|(i, s)| *s += i as i32);
    println!("{:?}", scores);

    let some: Option<i32> = Some(4);
    let none: Option<i32> = None;
    println!("{:?} {:?}", some.map(|x| x * 2), none.map(|x| x * 2));
    println!("{:?} {:?}", some.filter(|x| *x > 10), some.and_then(|x| if x > 1 { Some(x - 1) } else { None }));
    println!("{} {}", none.unwrap_or_else(|| 99), some.map_or(0, |x| x + 1));
    println!("{} {}", none.map_or_else(|| -1, |x| x), some.is_some_and(|x| x % 2 == 0));
    println!("{} {:?} {:?}", none.is_none_or(|x| x > 100), none.or(Some(7)), some.zip(Some('z')));

    let a: HashSet<i32> = (1..6).collect();
    let b: HashSet<i32> = (4..9).collect();
    println!("{:?}", sorted(a.union(&b).collect()));
    println!("{:?}", sorted(a.intersection(&b).collect()));
    println!("{:?}", sorted(a.difference(&b).collect()));
    println!("{:?}", sorted(a.symmetric_difference(&b).collect()));
    let mut c = a.clone();
    c.retain(|x| x % 2 == 1);
    c.extend(vec![10, 11]);
    let mut cv: Vec<_> = c.into_iter().collect();
    cv.sort_unstable();
    println!("{:?}", cv);

    let mut stock: HashMap<&str, i32> = HashMap::new();
    stock.extend(vec![("apple", 3), ("pear", 0), ("fig", 7)]);
    stock.entry("apple").and_modify(|n| *n += 1).or_insert(100);
    stock.entry("kiwi").and_modify(|n| *n += 1).or_insert(100);
    stock.entry("fig").and_modify(|n| *n *= 2);
    let made = stock.entry("plum").or_insert_with(|| 2 + 3);
    *made += 1;
    stock.entry("lime").or_insert_with_key(|k| k.len() as i32);
    stock.retain(|_, n| {
        *n -= 1;
        *n >= 0
    });
    let mut items: Vec<_> = stock.iter().map(|(k, v)| (*k, *v)).collect();
    items.sort_by(|a, b| a.0.cmp(b.0));
    println!("{:?}", items);
    let total: i32 = stock.values().sum();
    let mut keys: Vec<&&str> = stock.keys().collect();
    keys.sort();
    println!("{} {:?}", total, keys);
}
