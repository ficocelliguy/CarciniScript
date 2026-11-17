// `&mut place` arguments (fields, elements, locals) to functions and methods that can't keep the
// borrow: the place is read into a temp and written back after the call. Nested calls, calls in
// expressions, early returns, `?`, recursion through the same place, Options, arrays, and callees
// that could keep the borrow (lifetimes, generic returns) which still get a live cell.
fn bump(x: &mut u32, by: u32) -> u32 {
    *x = x.wrapping_add(by);
    *x
}

fn bump_twice(x: &mut u32) {
    bump(x, 1);
    bump(&mut *x, 1);
}

fn early(x: &mut u32, stop: bool) -> u32 {
    if stop {
        return *x;
    }
    *x += 100;
    *x
}

fn parse_into(x: &mut u32, s: &str) -> Result<u32, std::num::ParseIntError> {
    *x += 1;
    let v: u32 = s.parse()?;
    *x += v;
    Ok(*x)
}

fn countdown(x: &mut u32, depth: u32) {
    if depth == 0 {
        return;
    }
    *x -= 1;
    countdown(x, depth - 1);
}

fn fill(slot: &mut Option<u32>, v: u32) -> bool {
    if slot.is_none() {
        *slot = Some(v);
        true
    } else {
        false
    }
}

fn swap_pair(a: &mut i32, b: &mut i32) {
    let t = *a;
    *a = *b;
    *b = t;
}

fn longest<'a>(x: &'a mut u32, _y: &'a mut u32) -> &'a mut u32 {
    *x += 1;
    x
}

fn keep<T>(x: &mut u32, v: T) -> T {
    *x += 1;
    v
}

struct Inner {
    n: u32,
}

struct Counter {
    n: u32,
    hits: Vec<u32>,
    inner: Inner,
    arr: [u32; 3],
    tag: Option<u32>,
}

impl Counter {
    fn run(&mut self, rounds: u32) {
        for i in 0..rounds {
            bump(&mut self.n, i);
            let k = (i % 3) as usize;
            bump(&mut self.hits[k], 1);
            bump(&mut self.inner.n, 2);
            bump(&mut self.arr[k], 3);
        }
    }

    fn add_to(&self, x: &mut u32) {
        *x += self.n;
    }

    fn take_from(&mut self, x: &mut u32) -> u32 {
        self.n += *x;
        *x = 0;
        self.n
    }
}

struct Holder<'a> {
    r: &'a mut u32,
}

impl<'a> Holder<'a> {
    fn set(&mut self, r: &'a mut u32) {
        self.r = r;
    }
    fn get(&self) -> u32 {
        *self.r
    }
}

fn main() {
    let mut n = 5;
    bump(&mut n, 2);
    bump_twice(&mut n);
    println!("{}", n);
    println!("{} {}", early(&mut n, true), early(&mut n, false));
    println!("{:?} {:?} {}", parse_into(&mut n, "7"), parse_into(&mut n, "x"), n);
    let mut d = 10;
    countdown(&mut d, 4);
    println!("{}", d);
    let total = bump(&mut n, 1) + bump(&mut n, 1) * 2;
    println!("{} {}", total, n);
    let mut c = Counter { n: 0, hits: vec![0; 3], inner: Inner { n: 0 }, arr: [0; 3], tag: None };
    c.run(10);
    println!("{} {:?} {} {:?}", c.n, c.hits, c.inner.n, c.arr);
    let mut x = 1;
    c.add_to(&mut x);
    let taken = c.take_from(&mut x);
    println!("{} {}", x, taken);
    println!("{} {}", x, c.n);
    let filled = (fill(&mut c.tag, 4), fill(&mut c.tag, 5));
    println!("{:?} {:?}", filled, c.tag);
    let mut v = vec![1, 2, 3];
    let mut w = vec![7, 8];
    let (mut p, mut q) = (1, 2);
    swap_pair(&mut v[0], &mut w[1]);
    swap_pair(&mut p, &mut q);
    println!("{:?} {:?} {} {}", v, w, p, q);
    // The callee hands the borrow back: a live cell, so the write-through shows.
    let (mut a, mut b) = (1, 1);
    let seen_a = *longest(&mut a, &mut b) + 10;
    println!("{} {} {}", seen_a, a, b);
    println!("{} {}", keep(&mut a, "k"), a);
    // A `&mut` stored in a struct outlives the call.
    let mut target = 7;
    let mut other = 0;
    let mut h = Holder { r: &mut other };
    h.set(&mut target);
    *h.r += 1;
    println!("{}", h.get());
    // Nested: an argument's call writes another place while the outer call's temp is live.
    let mut m = 1;
    let mut k = 10;
    let r = bump(&mut m, bump(&mut k, 5));
    println!("{} {} {}", r, m, k);
    // The same place in a closure called by the callee isn't possible under the borrow rules;
    // a closure that captures a copy is.
    let seen = m;
    let f = move |by: u32| seen + by;
    println!("{} {}", bump(&mut m, f(1)), m);
}
