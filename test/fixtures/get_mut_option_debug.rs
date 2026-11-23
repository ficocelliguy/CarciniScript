// `{:?}` of a kept `Option` from `get_mut` / `first_mut` / `peek_mut` over numbers and
// strings shows the value, and a `get_mut` printed directly is a copy.
use std::collections::HashMap;

fn part1() {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("a", 5);
    let o = m.get_mut("a");
    println!("{:?}", o);
    let n = m.get_mut("zz");
    println!("{:?}", n);
    let mut v = vec![1.5, 2.5];
    let o2 = v.get_mut(1);
    println!("{:?}", o2);
    let mut ms: HashMap<i32, String> = HashMap::new();
    ms.insert(1, "s".into());
    let o3 = ms.get_mut(&1);
    println!("{:?} {}", o3, format!("{:?}", 1));
    let mut it = vec![7, 8].into_iter().peekable();
    let p = it.peek_mut();
    println!("{:?}", p);
}

fn part2() {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("a", 5);
    let o = m.get_mut("a");
    println!("{o:?}");
    let s = format!("{:?}!", o);
    println!("{}", s);
    if let Some(v) = o { *v += 1; }
    println!("{:?}", m.get_mut("a"));
    let mut v = vec![true];
    let f = v.first_mut();
    eprintln!("{:?}", f);
    println!("{:?}", f);
}

fn main() {
    part1();
    part2();
}
