// `&a | &b`, `&a & &b`, `&a - &b` and `&a ^ &b` on `HashSet` and `BTreeSet`.
use std::collections::{BTreeSet, HashSet};

fn sorted(s: &HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = s.iter().cloned().collect();
    v.sort();
    v
}

fn main() {
    let a: HashSet<String> = ["n00dles", "foodnstuff", "joesguns"].iter().map(|s| s.to_string()).collect();
    let b: HashSet<String> = ["joesguns", "hong-fang-tea"].iter().map(|s| s.to_string()).collect();
    let u = &a | &b;
    println!("{:?} {}", sorted(&u), u.len());
    println!("{:?}", sorted(&(&a & &b)));
    println!("{:?}", sorted(&(&a - &b)));
    println!("{:?}", sorted(&(&a ^ &b)));
    // The new set is its own: changing it leaves the operands alone.
    let mut both = &a & &b;
    both.insert("sigma-cosmetics".to_string());
    println!("{} {} {}", both.len(), a.len(), b.len());

    let x: BTreeSet<i32> = (1..6).collect();
    let y: BTreeSet<i32> = (4..9).collect();
    println!("{:?}", &x | &y);
    println!("{:?}", &x & &y);
    println!("{:?}", &x - &y);
    println!("{:?}", &x ^ &y);
    let z = &(&x | &y) - &[2, 3].into_iter().collect::<BTreeSet<i32>>();
    println!("{:?} {:?}", z.first(), z.len());
}
