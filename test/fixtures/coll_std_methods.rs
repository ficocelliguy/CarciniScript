// `VecDeque::split_off` / `as_slices` / `reserve_exact` / `resize`, `get_key_value` and
// `remove_entry` on `HashMap` / `BTreeMap`, `Option::as_deref_mut`, `Result::or` / `and`.
use std::collections::{BTreeMap, HashMap, VecDeque};

fn shout(o: &mut Option<String>) {
    if let Some(s) = o.as_deref_mut() {
        s.make_ascii_uppercase();
    }
}

fn main() {
    let mut d: VecDeque<u8> = (1..=6).collect();
    d.push_front(0);
    let tail = d.split_off(4);
    println!("{:?} {:?}", d, tail);
    d.reserve_exact(3);
    d.resize(6, 9);
    println!("{:?} {}", d, d.len());
    d.resize(2, 0);
    let (a, b) = d.as_slices();
    println!("{} {:?}", a.len() + b.len(), d);

    let mut h: HashMap<String, u32> = HashMap::new();
    h.insert("a".to_string(), 1);
    println!("{:?} {:?}", h.get_key_value("a"), h.get_key_value("z"));
    println!("{:?} {}", h.remove_entry("a"), h.len());

    let mut m: BTreeMap<u8, &str> = BTreeMap::new();
    m.insert(2, "two");
    m.insert(1, "one");
    println!("{:?}", m.get_key_value(&2));
    println!("{:?}", m.remove_entry(&1));
    let gone = m.remove_entry(&7);
    println!("{:?} {:?}", m, gone);

    let mut o = Some("hi".to_string());
    shout(&mut o);
    println!("{:?}", o);

    let bad: Result<u32, String> = Err("no".into());
    let good: Result<u32, String> = Ok(3);
    let r1: Result<u32, u8> = bad.clone().or(Ok(1));
    let r2: Result<u32, u8> = good.clone().or(Err(7));
    println!("{:?} {:?}", r1, r2);
    println!("{:?} {:?}", good.and(Ok::<&str, String>("x")), bad.and(Ok::<&str, String>("y")));
}
