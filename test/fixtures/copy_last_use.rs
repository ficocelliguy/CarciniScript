// A `Copy` value's last use moves it instead of copying (`codegen_lastuse.rs`); every shape here
// that could still see the value afterwards keeps its copy, so changing one side leaves the
// other alone, as in Rust.
#[derive(Debug, Clone, Copy, PartialEq)]
struct P {
    x: i32,
    y: i32,
}

impl P {
    fn sum(&self) -> i32 {
        self.x + self.y
    }
    fn me(&self) -> &P {
        self
    }
}

#[derive(Debug, Clone, Copy)]
struct Outer {
    inner: P,
    n: u8,
}

fn add(a: P, b: P) -> P {
    P { x: a.x + b.x, y: a.y + b.y }
}

fn bump(mut p: P) -> P {
    p.x += 100;
    p
}

fn main() {
    // Moved: a new value's only use, and after scalar reads and printing.
    let mut v: Vec<P> = Vec::new();
    let a = P { x: 1, y: 2 };
    let s = a.sum();
    println!("{:?} {}", a, a == P { x: 1, y: 2 });
    v.push(a);
    v[0].x = 50;
    println!("{:?} {}", v, s);

    // A copy of a place: `b` is its own, then moved.
    let b = v[0];
    v.push(b);
    v[1].y = 60;
    println!("{:?}", v);

    // `Option::copied` hands out the element itself in JS: not moved.
    let c = v.get(0).copied().unwrap();
    let mut w = vec![c];
    w[0].x = -1;
    println!("{:?} {:?}", v[0], w);

    // Borrowed: `r` still reads `d`.
    let d = P { x: 3, y: 4 };
    let r = &d;
    let mut u = vec![d];
    u[0].x = 30;
    println!("{:?} {:?}", r, u);
    let e = P { x: 5, y: 6 };
    let re = e.me();
    let mut u2 = vec![e];
    u2[0].y = 66;
    println!("{:?} {:?}", re, u2);

    // In a loop around its use: each iteration needs its own copy.
    let f = P { x: 7, y: 8 };
    let mut many = Vec::new();
    for i in 0..3 {
        many.push(f);
        many[i].x += i as i32;
    }
    println!("{:?}", many);

    // Declared in the loop: moved each time.
    let mut fresh = Vec::new();
    for i in 0..3 {
        let g = P { x: i, y: i * 2 };
        fresh.push(g);
    }
    fresh[0].y = 99;
    println!("{:?}", fresh);

    // A closure reads it later.
    let h = P { x: 9, y: 10 };
    let get = || h.x;
    let mut hv = vec![h];
    hv[0].x = 90;
    println!("{} {:?}", get(), hv);

    // Its field copied out first, then the whole moved.
    let o = Outer { inner: P { x: 11, y: 12 }, n: 1 };
    let mut inner = o.inner;
    let mut ov = vec![o];
    ov[0].inner.x = 110;
    inner.y = 120;
    println!("{:?} {:?} {}", ov, inner, o.n);

    // Twice in one call: the first copies, the last moves.
    let k = P { x: 1, y: 1 };
    let k2 = add(k, k);
    println!("{:?}", k2);

    // Into a function that changes its parameter.
    let m = P { x: 2, y: 3 };
    let m2 = bump(m);
    println!("{:?}", m2);

    // A tuple and an array, too.
    let t = (1, 2);
    let mut tv = vec![t];
    tv[0].0 = 10;
    let arr = [1u8, 2, 3];
    let mut av = vec![arr];
    av[0][0] = 100;
    println!("{:?} {:?}", tv, av);

    // Shadowed by a pattern binding of the same name: not moved.
    let q = P { x: 13, y: 14 };
    let mut qv = vec![q];
    let opt = Some(qv[0]);
    if let Some(q) = opt {
        qv.push(q);
    }
    qv[1].x = 130;
    println!("{:?} {:?}", qv, opt);

    // Returned.
    println!("{:?}", make());
}

fn make() -> P {
    let p = P { x: 15, y: 16 };
    let doubled = P { x: p.x * 2, y: p.y * 2 };
    doubled
}
