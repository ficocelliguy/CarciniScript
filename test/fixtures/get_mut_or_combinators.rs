// `get_mut` / `as_mut` / `peek_mut` of a number, string or bool given to `map_or` /
// `map_or_else` (the closure binds a slot), unwrapped with `unwrap_or(&mut default)` (a slot over
// the default local, or one holding a literal), or turned into a `Result` with `ok_or` /
// `ok_or_else` and unwrapped by `?` or `unwrap()`: writes reach the collection.
use std::collections::{BTreeMap, HashMap};

struct T {
    timer: Option<i32>,
}

fn bump(m: &mut HashMap<&str, i32>, k: &str) -> Result<i32, String> {
    *m.get_mut(k).ok_or(format!("no {k}"))? += 1;
    let r = m.get_mut(k).ok_or_else(|| "gone".to_string())?;
    *r *= 10;
    Ok(*r)
}

fn main() {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("a", 1);
    let n = m.get_mut("a").map_or(0, |v| {
        *v += 5;
        *v
    });
    println!("{} {:?}", n, m);
    let s = m.get_mut("zz").map_or_else(|| -1, |v| *v);
    let t = m.get_mut("a").map_or_else(|| -1, |v| {
        *v -= 1;
        *v
    });
    println!("{} {} {:?}", s, t, m);
    *m.get_mut("a").unwrap_or(&mut 0) += 100;
    *m.get_mut("b").unwrap_or(&mut 0) += 100;
    let mut spare = 7;
    *m.get_mut("c").unwrap_or(&mut spare) += 1;
    let x = *m.get_mut("a").unwrap_or(&mut 0);
    println!("{:?} {} {}", m, spare, x);
    println!("{:?}", bump(&mut m, "a"));
    println!("{:?}", bump(&mut m, "q"));
    println!("{:?}", m);
    let mut v = vec![1.5, 2.5];
    let w = v.get_mut(0).map_or(0.0, |r| {
        *r *= 2.0;
        *r
    });
    *v.get_mut(1).ok_or("x").unwrap() += 1.0;
    *v.get_mut(5).unwrap_or(&mut 0.0) += 1.0;
    println!("{} {:?}", w, v);
    let mut it = vec![1, 2].into_iter().peekable();
    let x = it.peek_mut().map_or(0, |p| {
        *p += 10;
        *p
    });
    println!("{} {:?}", x, it.collect::<Vec<_>>());
    let mut t = T { timer: Some(3) };
    *t.timer.as_mut().unwrap_or(&mut 0) += 1;
    let mut u = T { timer: None };
    *u.timer.as_mut().unwrap_or(&mut 0) += 1;
    println!("{:?} {:?}", t.timer, u.timer);
    let mut m: HashMap<&str, String> = HashMap::new();
    m.insert("a", "x".into());
    let mut d = String::new();
    let r = m.get_mut("b").unwrap_or(&mut d);
    r.push('!');
    let r2 = m.get_mut("a").unwrap_or(&mut d);
    r2.push('?');
    println!("{:?} {:?}", m, d);
    let mut b: BTreeMap<u8, bool> = BTreeMap::new();
    b.insert(1, false);
    b.get_mut(&1).map_or((), |v| *v = true);
    println!("{:?}", b);
}
