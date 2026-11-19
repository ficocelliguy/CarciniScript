// `&mut` to a number, string, bool or `Option` inside a struct, tuple or Vec, kept as a value:
// in a local (re-pointed later, walking a list), returned from a function (`-> &mut i32`,
// `-> Option<&mut i32>`), passed as an `Option<&mut i32>`, held in a struct field, picked by an
// `if`, destructured from a tuple, mapped with `as_mut().map(..)` / `.filter(..)`, and a
// `&mut i32` function given to `for_each`. Every write must reach the place.
use std::collections::HashMap;
use std::ops::AddAssign;

struct Node {
    v: i32,
    next: Option<Box<Node>>,
}

struct List {
    head: Option<Box<Node>>,
}

impl List {
    fn push_back(&mut self, v: i32) {
        let mut cur = &mut self.head;
        while let Some(node) = cur {
            cur = &mut node.next;
        }
        *cur = Some(Box::new(Node { v, next: None }));
    }

    fn items(&self) -> Vec<i32> {
        let mut c = &self.head;
        let mut out = vec![];
        while let Some(n) = c {
            out.push(n.v);
            c = &n.next;
        }
        out
    }
}

struct S {
    count: i32,
    other: i32,
    name: String,
    timer: Option<i32>,
    label: Option<String>,
    v: Vec<i32>,
    m: HashMap<String, f64>,
}

impl S {
    fn slot(&mut self) -> &mut i32 {
        &mut self.count
    }
    fn pick(&mut self, c: bool) -> &mut i32 {
        if c { &mut self.count } else { &mut self.other }
    }
    fn at(&mut self, i: usize) -> &mut i32 {
        &mut self.v[i]
    }
    fn find(&mut self, k: &str) -> Option<&mut f64> {
        self.m.get_mut(k)
    }
    fn first(&mut self) -> Option<&mut i32> {
        self.v.first_mut()
    }
    fn big(&mut self) -> Option<&mut i32> {
        self.v.iter_mut().find(|x| **x > 5)
    }
    fn timer_mut(&mut self) -> Option<&mut i32> {
        self.timer.as_mut()
    }
    fn maybe(&mut self, c: bool) -> Option<&mut i32> {
        if c { Some(&mut self.count) } else { None }
    }
}

fn pick2<'a>(a: &'a mut i32, b: &'a mut i32, c: bool) -> &'a mut i32 {
    if c { a } else { b }
}

fn bump_opt(o: Option<&mut i32>) {
    if let Some(n) = o {
        *n += 1;
    }
}

fn bump_opt_match(o: Option<&mut i32>) -> bool {
    match o {
        Some(n) => {
            *n *= 2;
            true
        }
        None => false,
    }
}

fn bump(n: &mut i32) {
    *n += 10;
}

fn shout(s: &mut String) {
    s.push('!');
}

#[derive(Debug)]
struct W<'a> {
    n: &'a mut i32,
    tag: u8,
}

fn bump_w(w: &mut W) {
    *w.n += 1;
}

trait BumpBy {
    fn bump_by(&mut self, by: Self);
}

impl<T: AddAssign + Copy> BumpBy for T {
    fn bump_by(&mut self, by: T) {
        *self += by;
    }
}

fn bump_generic<T: BumpBy + Copy>(mut t: T, by: T) -> T {
    t.bump_by(by);
    t
}

fn main() {
    // A list built by walking a kept `&mut Option<Box<Node>>`.
    let mut l = List { head: None };
    l.push_back(1);
    l.push_back(2);
    l.push_back(3);
    println!("{:?}", l.items());

    let mut s = S {
        count: 1,
        other: 100,
        name: "a".into(),
        timer: Some(1),
        label: Some("x".into()),
        v: vec![1, 2, 3, 9],
        m: HashMap::new(),
    };
    s.m.insert("a".into(), 1.0);

    // Kept in a local, re-pointed.
    let mut r = &mut s.count;
    *r += 1;
    r = &mut s.other;
    *r += 1;
    let mut e = &mut s.v[0];
    *e += 5;
    e = &mut s.v[1];
    *e += 5;
    let t = &mut s.name;
    t.push('b');
    println!("{} {} {:?} {}", s.count, s.other, s.v, s.name);

    // Returned from methods and functions.
    *s.slot() += 1;
    let q = s.slot();
    *q *= 10;
    *s.pick(false) += 1;
    *s.at(2) += 1;
    let a2 = s.at(3);
    *a2 -= 1;
    bump(s.at(0));
    let mut x = 1;
    let mut y = 2;
    *pick2(&mut x, &mut y, false) = 7;
    *pick2(&mut x, &mut y, true) += 1;
    println!("{} {} {:?} {} {}", s.count, s.other, s.v, x, y);
    if let Some(f) = s.find("a") {
        *f += 1.5;
    }
    *s.find("a").unwrap() *= 2.0;
    if let Some(f) = s.first() {
        *f = 9;
    }
    match s.big() {
        Some(b) => *b = 0,
        None => {}
    }
    if let Some(t) = s.timer_mut() {
        *t += 41;
    }
    let got = s.maybe(true);
    bump_opt(got);
    bump_opt(s.maybe(false));
    let Some(c) = s.maybe(true) else { return };
    *c += 1;
    println!("{:?} {:?} {:?} {}", s.m.get("a"), s.v, s.timer, s.count);

    // `Option<&mut i32>` arguments.
    bump_opt(s.timer.as_mut());
    bump(s.timer.as_mut().unwrap());
    let mut t = Some(5);
    bump_opt(t.as_mut());
    println!("{} {}", bump_opt_match(t.as_mut()), bump_opt_match(None));
    bump_opt(Some(&mut x));
    bump_opt(s.v.get_mut(1));
    bump_opt(s.v.iter_mut().find(|x| **x == 3));
    println!("{:?} {:?} {} {:?}", s.timer, t, x, s.v);

    // Mapped and filtered.
    s.timer.as_mut().map(|n| *n += 1);
    if let Some(n) = s.timer.as_mut().filter(|n| **n > 0) {
        *n += 5;
    }
    s.label.as_mut().map(|x| x.push('!'));
    let big = s.timer.as_mut().is_some_and(|n| {
        *n -= 1;
        *n > 10
    });
    let o = s.timer.as_mut();
    if let Some(n) = o {
        *n += 100;
    }
    let o2 = Some(&mut s.other);
    bump_opt(o2);
    println!("{:?} {:?} {} {}", s.timer, s.label, big, s.other);

    // Picked by an `if`, destructured from a tuple.
    let mut pa = 1;
    let mut pb = 10;
    let c = false;
    let z: &mut i32 = if c { &mut pa } else { &mut pb };
    *z += 1;
    let mut pair = (1, 2);
    let (p, q2) = (&mut pair.0, &mut pair.1);
    *p += 1;
    *q2 += 1;
    println!("{} {} {:?}", pa, pb, pair);

    // A struct holding a `&mut i32`.
    {
        let w = W { n: &mut s.count, tag: 0 };
        *w.n += 1;
        let _ = w.tag;
    }
    {
        let mut w = W { n: &mut x, tag: 1 };
        bump_w(&mut w);
        *w.n *= 3;
        let seen = *w.n;
        println!("{} {:?}", seen, w);
    }
    println!("{} {}", s.count, x);

    // A blanket impl's `*self += by` on a number local, a field, an element, a `&mut i32`, and
    // through a type parameter.
    let mut bx = 1;
    bx.bump_by(2);
    s.count.bump_by(5);
    let mut fv = vec![1.5];
    fv[0].bump_by(1.0);
    let br = &mut bx;
    br.bump_by(10);
    println!("{} {} {:?} {}", bx, s.count, fv, bump_generic(10u8, 5));

    // A `&mut i32` function given to `for_each`.
    let mut v = vec![1, 2];
    v.iter_mut().for_each(bump);
    let mut m: HashMap<String, i32> = HashMap::new();
    m.insert("a".into(), 1);
    m.values_mut().for_each(bump);
    let mut names = vec!["p".to_string()];
    names.iter_mut().for_each(shout);
    println!("{:?} {:?} {:?}", v, m, names);
}
