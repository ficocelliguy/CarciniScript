// Tuples holding a `&mut` to a number, string, bool or `Option` (`let t = (1, &mut x);`, a
// `t: &mut (usize, &mut i32)` parameter, nested tuples, destructuring): the tuple holds a slot,
// so writes reach `x`; `&t` given to a `&(usize, &mut i32)` is a copy with the slots read.

fn bump(t: &mut (usize, &mut i32)) { *t.1 += 100; t.0 += 1; }
fn part1() {
    let mut x = 1;
    let mut t = (1usize, &mut x);
    *t.1 += 5;
    bump(&mut t);
    println!("{}", t.0);
    println!("{}", x);
    let mut s = String::from("a");
    let t2 = (0, &mut s);
    t2.1.push('b');
    println!("{}", s);
    let mut b = false;
    { let t3 = (&mut b, 3); *t3.0 = true; }
    println!("{}", b);
}

fn inc(n: &mut i32) { *n += 1; }
fn show(t: (usize, &mut i32)) -> i32 { *t.1 * 2 }
fn part2() {
    let mut x = 1;
    let mut y = 10;
    {
        let (i, r) = (0, &mut x);
        *r += 1 + i;
    }
    let t: (i32, &mut i32) = (5, &mut y);
    inc(t.1);
    *t.1 += t.0;
    println!("{:?}", t);
    let n = show((3, &mut x));
    println!("{} {} {}", x, y, n);
    let mut o: Option<i32> = None;
    let mut z = 0.5f64;
    let w = (&mut o, (1, &mut z));
    *w.0 = Some(4);
    *(w.1).1 *= 4.0;
    println!("{:?} {}", o, z);
    let mut s = String::new();
    let p = (&mut s, 1);
    p.0.push_str("hi");
    let q = p.0.len();
    println!("{} {}", s, q);
}

fn show_ref(t: &(usize, &mut i32)) -> String { format!("#{} = {}", t.0, *t.1) }
fn gener<T: std::fmt::Debug>(t: T) -> String { format!("{:?}", t) }
fn part3() {
    let mut x = 4;
    let t = (1usize, &mut x);
    println!("{}", show_ref(&t));
    let o = Some(&t).map(|p| *p.1 + 1);
    println!("{:?}", o);
    println!("{}", gener(&t));
    let u = &t;
    println!("{}", u.1);
    let (a, b) = t;
    *b += 1;
    println!("{} {}", a, x);
}

fn main() {
    part1();
    part2();
    part3();
}
