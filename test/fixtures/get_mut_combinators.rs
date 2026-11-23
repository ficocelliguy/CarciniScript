// `get_mut` of a number, string or bool used through `map` / `and_then` / `is_some_and` (the
// closure binds a slot over the entry or element, so writes stick), and read as a copy by
// `copied()` / `cloned()` / `== None`.
use std::collections::HashMap;
fn main() {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("a", 1);
    let x = m.get_mut("a").map(|v| *v);
    println!("{:?}", x);
    println!("{}", m.get_mut("b") == None);
    println!("{}", m.get_mut("a") != None);
    m.get_mut("a").map(|v| *v += 10);
    println!("{:?}", m.get_mut("a").copied());
    println!("{:?}", m.get_mut("a").cloned());
    println!("{}", m.get_mut("a").is_some_and(|v| { *v += 1; *v > 0 }));
    println!("{:?}", m.get_mut("z").map(|v| *v));
    println!("{:?}", m);
    let mut v = vec![1.0, 2.0];
    let y = v.get_mut(1).map(|r| *r * 2.0);
    println!("{:?}", y);
    v.last_mut().map(|r| *r -= 0.5);
    v.get_mut(7).map(|r| *r -= 0.5);
    let none = v.get_mut(9) == None;
    println!("{:?} {}", v, none);
    let mut s: HashMap<u8, String> = HashMap::new();
    s.insert(1, "x".into());
    s.get_mut(&1).map(|t| t.push('y'));
    let n = s.get_mut(&1).map(|t| t.len()).unwrap_or(0);
    println!("{:?} {}", s, n);
    let k = s.get_mut(&1).and_then(|t| t.pop());
    println!("{:?} {:?}", k, s);
}
