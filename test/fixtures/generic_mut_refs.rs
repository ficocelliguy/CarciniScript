// `&mut T` parameters with `T` a type parameter: numbers, strings, bools and Options (which JS
// can't point at) as well as structs and Vecs, from locals, fields, elements and other `&mut`
// parameters, through a chain of generic calls.
use std::ops::AddAssign;

fn bump<T: AddAssign + Copy>(x: &mut T, d: T) {
    *x += d;
}

fn set<T>(slot: &mut T, v: T) {
    *slot = v;
}

fn swap_vals<T: Clone>(a: &mut T, b: &mut T) {
    let t = a.clone();
    *a = b.clone();
    *b = t;
}

// Hands its `&mut` on, twice.
fn twice<T: AddAssign + Copy>(x: &mut T, d: T) {
    bump(x, d);
    bump(x, d);
}

fn take_or<T>(slot: &mut Option<T>, or: T) -> T {
    slot.take().unwrap_or(or)
}

fn fill<T: Clone>(v: &mut Vec<T>, x: T, n: usize) {
    for _ in 0..n {
        v.push(x.clone());
    }
}

fn update<T, F: FnOnce(&T) -> T>(x: &mut T, f: F) {
    *x = f(x);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct V(i32, i32);

impl AddAssign for V {
    fn add_assign(&mut self, o: V) {
        self.0 += o.0;
        self.1 += o.1;
    }
}

#[derive(Debug)]
struct Stats {
    hp: f64,
    name: String,
    tags: Vec<u8>,
}

fn main() {
    let mut n = 1;
    bump(&mut n, 2);
    twice(&mut n, 10);
    let mut f = 0.5;
    bump(&mut f, 0.25);
    let mut v = V(1, 1);
    twice(&mut v, V(2, 3));
    println!("{n} {f} {v:?}");

    let mut s = String::from("a");
    set(&mut s, String::from("b"));
    let mut flag = false;
    set(&mut flag, true);
    let mut o: Option<i32> = Some(4);
    set(&mut o, None);
    println!("{s} {flag} {o:?}");

    let (mut a, mut b) = (String::from("x"), String::from("y"));
    swap_vals(&mut a, &mut b);
    let (mut p, mut q) = (V(1, 2), V(3, 4));
    swap_vals(&mut p, &mut q);
    println!("{a}{b} {p:?} {q:?}");

    // Fields and elements.
    let mut st = Stats { hp: 1.0, name: "n".into(), tags: vec![] };
    bump(&mut st.hp, 2.5);
    set(&mut st.name, "bot".to_string());
    fill(&mut st.tags, 7, 2);
    let mut xs = vec![1, 2, 3];
    bump(&mut xs[1], 40);
    let mut names = vec![String::from("p")];
    set(&mut names[0], String::from("q"));
    println!("{st:?} {xs:?} {names:?}");

    let mut slot = Some(String::from("taken"));
    let got = take_or(&mut slot, String::new());
    let mut none: Option<u32> = None;
    println!("{got} {slot:?} {}", take_or(&mut none, 9));

    let mut k = 3;
    update(&mut k, |x| x * x);
    let mut w = String::from("ab");
    update(&mut w, |s| s.repeat(2));
    println!("{k} {w}");
}
