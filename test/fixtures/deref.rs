// `impl Deref` / `DerefMut`: methods the type doesn't have go to the target (`push`, `sort`, `len`,
// `iter`, `sqrt`), indexing, `*x`, and `&stack` where `&[i32]` is expected; own fields stay reachable.
use std::ops::{Deref, DerefMut};

struct Stack {
    items: Vec<i32>,
    name: String,
}

impl Deref for Stack {
    type Target = Vec<i32>;
    fn deref(&self) -> &Vec<i32> {
        &self.items
    }
}

impl DerefMut for Stack {
    fn deref_mut(&mut self) -> &mut Vec<i32> {
        &mut self.items
    }
}

struct Meters(f64);

impl Deref for Meters {
    type Target = f64;
    fn deref(&self) -> &f64 {
        &self.0
    }
}

fn total(v: &[i32]) -> i32 {
    v.iter().sum()
}

fn main() {
    let mut s = Stack { items: vec![3, 1, 2], name: "s".to_string() };
    s.push(5);
    s.sort();
    println!("{} {:?} {} {}", s.len(), s.first(), s[1], s.name);
    println!("{} {}", total(&s), s.iter().max().unwrap());
    let m = Meters(2.5);
    println!("{} {}", *m * 2.0, m.sqrt() > 1.0);
}
