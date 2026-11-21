// A generic `-> &mut T` / `-> Option<&mut T>` returns a slot whatever `T` is: the caller keeps
// it for numbers, strings, bools and Options (and type parameters), and reads the struct or
// collection out of it otherwise. A return that has no place to point at is a slot holding
// the value.
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct P {
    hp: i32,
}

fn first_mut<T>(v: &mut [T]) -> Option<&mut T> {
    v.first_mut()
}

fn last_of<T>(v: &mut Vec<T>) -> Option<&mut T> {
    v.last_mut()
}

fn nth<T>(v: &mut Vec<T>, i: usize) -> Option<&mut T> {
    v.get_mut(i)
}

fn at<T>(v: &mut Vec<T>, i: usize) -> &mut T {
    &mut v[i]
}

fn pick<'a, T>(a: &'a mut T, b: &'a mut T, first: bool) -> &'a mut T {
    if first { a } else { b }
}

// A generic caller of a generic `-> &mut T`.
fn bump_first<T: std::ops::AddAssign + Copy>(v: &mut Vec<T>, d: T) {
    if let Some(x) = first_mut(v) {
        *x += d;
    }
    *at(v, 1) += d;
}

fn first_of_first<T>(v: &mut Vec<T>) -> &mut T {
    at(v, 0)
}

struct Stack<T> {
    items: Vec<T>,
    top: T,
}

impl<T> Stack<T> {
    fn peek_mut(&mut self) -> Option<&mut T> {
        self.items.last_mut()
    }
    fn top_mut(&mut self) -> &mut T {
        &mut self.top
    }
    fn get(&mut self, i: usize) -> &mut T {
        &mut self.items[i]
    }
}

fn entry<'a, V>(m: &'a mut HashMap<String, V>, k: &str) -> Option<&'a mut V> {
    m.get_mut(k)
}

fn either<'a, T>(v: &'a mut Vec<T>, w: &'a mut Vec<T>, i: usize) -> Option<&'a mut T> {
    if i < v.len() {
        v.get_mut(i)
    } else if i == 99 {
        None
    } else {
        w.first_mut()
    }
}

fn first_or_panic<T>(v: &mut Vec<T>) -> &mut T {
    match v.len() {
        0 => panic!("empty"),
        _ => &mut v[0],
    }
}

fn main() {
    let mut ps = vec![P { hp: 1 }, P { hp: 2 }];
    if let Some(p) = first_mut(&mut ps) {
        p.hp += 10;
    }
    at(&mut ps, 1).hp = 50;
    last_of(&mut ps).unwrap().hp += 1;
    match nth(&mut ps, 0) {
        Some(p) => p.hp *= 2,
        None => {}
    }
    println!("{ps:?} {:?}", nth(&mut ps, 5).is_none());

    let mut ns = vec![1.5, 2.5];
    if let Some(x) = last_of(&mut ns) {
        *x = 9.0;
    }
    if let Some(x) = nth(&mut ns, 0) {
        *x -= 1.0;
    }
    bump_first(&mut ns, 1.0);
    *first_of_first(&mut ns) *= 2.0;
    println!("{ns:?}");

    let mut os: Vec<Option<i32>> = vec![Some(1), None];
    if let Some(o) = first_mut(&mut os) {
        *o = None;
    }
    *at(&mut os, 1) = Some(7);
    println!("{os:?}");

    let (mut a, mut b) = (1, 2);
    *pick(&mut a, &mut b, false) += 40;
    let (mut c, mut d) = (P { hp: 0 }, P { hp: 0 });
    pick(&mut c, &mut d, true).hp = 3;
    println!("{a} {b} {c:?} {d:?}");

    let mut s = Stack { items: vec![1, 2], top: 0 };
    if let Some(t) = s.peek_mut() {
        *t += 5;
    }
    *s.top_mut() = 4;
    *s.get(0) += 100;
    let mut sp = Stack { items: vec![P { hp: 1 }], top: P { hp: 2 } };
    sp.peek_mut().unwrap().hp = 9;
    sp.top_mut().hp += 1;
    let g = sp.get(0);
    g.hp += 1;
    let r = s.top_mut();
    *r += 1;
    println!("{:?} {} {:?} {:?}", s.items, s.top, sp.items, sp.top);

    let mut m: HashMap<String, P> = HashMap::new();
    m.insert("a".into(), P { hp: 1 });
    if let Some(p) = entry(&mut m, "a") {
        p.hp += 5;
    }
    let none = entry(&mut m, "z").is_none();
    println!("{:?} {}", m["a"], none);

    let (mut a, mut b) = (vec![1, 2], vec![30]);
    if let Some(x) = either(&mut a, &mut b, 1) {
        *x += 100;
    }
    if let Some(x) = either(&mut a, &mut b, 5) {
        *x += 100;
    }
    println!("{a:?} {b:?} {}", either(&mut a, &mut b, 99).is_none());
    *first_or_panic(&mut a) = -1;
    let mut ps = vec![P { hp: 0 }];
    first_or_panic(&mut ps).hp = 8;
    println!("{a:?} {ps:?}");
}
