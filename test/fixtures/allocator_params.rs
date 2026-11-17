// `A: Allocator` parameters, `Vec<T, A>` / `Box<T, A>`, `Global` / `System` and the `_in`
// constructors: accepted and ignored (JS has one allocator). Natively a nightly API
// (`RUSTC_BOOTSTRAP` builds it on stable).
#![feature(allocator_api)]
use std::alloc::{Allocator, Global, System};

fn total<A: Allocator>(v: &Vec<i32, A>) -> i32 {
    v.iter().sum()
}

fn boxed<A>(b: Box<i32, A>) -> i32
where
    A: Allocator,
{
    *b + 1
}

fn grow<A: Allocator + Clone>(v: &mut Vec<u8, A>, n: u8) {
    for i in 0..n {
        v.push(i);
    }
}

struct Pool<T, A: Allocator = Global> {
    items: Vec<T, A>,
}

impl<T: std::fmt::Debug, A: Allocator> Pool<T, A> {
    fn show(&self) {
        println!("{} {:?}", self.items.len(), self.items);
    }
}

fn main() {
    let mut v: Vec<i32, Global> = Vec::new_in(Global);
    v.push(3);
    v.push(4);
    println!("{}", total(&v));
    let mut w = Vec::with_capacity_in(4, System);
    grow(&mut w, 3);
    println!("{:?} {}", w, w.capacity() >= 3);
    let b = Box::new_in(41, Global);
    println!("{}", boxed(b));
    let p = Pool { items: vec![1, 2] };
    p.show();
    let q: Pool<&str> = Pool { items: Vec::new() };
    q.show();
    println!("{:?}", Global);
}
