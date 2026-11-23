// `dbg!` prints `[file:line:col] expr = {:#?}` to stderr and hands its value back.
// test: stderr
use std::collections::BTreeMap;
use std::fmt::Debug;

#[derive(Debug, Clone, Copy)]
struct Pt(f64, f64);

#[derive(Debug)]
struct P {
    x: i32,
    name: String,
}

fn show<T: Debug>(x: T) -> T {
    dbg!(x)
}

fn fact(n: u32) -> u32 {
    if dbg!(n <= 1) {
        dbg!(1)
    } else {
        dbg!(n * fact(n - 1))
    }
}

fn main() {
    let a = 2;
    let b = dbg!(a * 2) + 1;
    dbg!(b);
    dbg!();
    let (x, y) = dbg!(a, b);
    println!("{} {}", x, y);

    let p = dbg!(P { x: 1, name: "n".into() });
    dbg!(&p);
    let s = String::from("hi");
    let s2 = dbg!(s);
    println!("{} {}", s2, fact(3));

    let f: f64 = dbg!(1.0);
    let o = dbg!(Some(3u8));
    let q = show(Pt(1.5, -2.0));
    let t = dbg!((1, "a"),);
    let n: u8 = dbg!(7);
    let w = show(vec![Some(1.0), None]);
    let m = BTreeMap::from([(1, "x")]);
    dbg!(&m, "str",);
    let mut c = 0;
    let unit = dbg!(c += 1);
    println!("{} {:?} {:?} {:?} {} {:?} {:?} {} {}", f, o, q, t, n, w, unit, c, p.x);
    if dbg!(f > 0.5) && dbg!(n == 7) {
        println!("both");
    }
}
