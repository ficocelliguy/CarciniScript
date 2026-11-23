// A `Mutex` / `RwLock` guard bound by a pattern (`if let Ok(mut g) = m.lock()`, `match`,
// `while let`) or taken with `unwrap_or_else(|e| e.into_inner())`: the lock's value in place.
use std::sync::{Mutex, RwLock, Arc};

fn part1() {
    let m = Mutex::new(5);
    if let Ok(mut g) = m.lock() { *g += 1; }
    match m.lock() { Ok(mut g) => *g *= 10, Err(_) => {} }
    println!("{}", *m.lock().unwrap());
    let s = Mutex::new(String::from("a"));
    if let Ok(mut g) = s.lock() { g.push('b'); }
    match s.lock() { Ok(mut g) => { g.push_str("cd"); } Err(_) => {} }
    println!("{}", s.lock().unwrap());
    let v = Mutex::new(vec![1]);
    if let Ok(mut g) = v.lock() { g.push(2); }
    println!("{:?}", v.lock().unwrap());
}

struct S { m: Mutex<i32>, name: RwLock<String> }
impl S {
    fn bump(&self) { if let Ok(mut g) = self.m.lock() { *g += 1; } }
}
fn part2() {
    let s = S { m: Mutex::new(1), name: RwLock::new("n".into()) };
    s.bump(); s.bump();
    if let Ok(mut w) = s.name.write() { w.push_str("ame"); }
    if let Ok(r) = s.name.read() { println!("{}", *r); }
    let a = Arc::new(Mutex::new(0u64));
    for _ in 0..3 {
        let a2 = Arc::clone(&a);
        match a2.lock() { Ok(mut g) => { *g += 2; } Err(p) => { *p.into_inner() += 1; } }
    }
    let mut n = 0;
    while let Ok(mut g) = s.m.try_lock() { *g *= 2; n += 1; if n == 3 { break; } }
    { let mut g = s.m.lock().unwrap_or_else(|e| e.into_inner()); *g += 1; }
    println!("{} {}", s.m.lock().unwrap(), *a.lock().unwrap());
    println!("{:?}", s.m.lock().is_ok());
}

// A result kept in a local, matched later.
fn part3() {
    let m = Mutex::new(1);
    let r = m.lock();
    if let Ok(mut g) = r {
        *g += 5;
    }
    println!("{}", m.lock().unwrap());
}

fn main() {
    part1();
    part2();
    part3();
}
