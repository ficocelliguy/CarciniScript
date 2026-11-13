// `&mut` to numbers, strings, bools and Options: passed to functions and methods, handed on, and
// reborrowed; closures sharing the same variable; std::mem::swap on locals.
fn bump(x: &mut i32) {
    *x += 1;
}

fn bump_twice(x: &mut i32) {
    bump(x);
    bump(&mut *x);
}

fn shout(s: &mut String) {
    s.push_str("!");
    *s = s.to_uppercase();
}

fn toggle(flag: &mut bool) -> bool {
    *flag = !*flag;
    *flag
}

fn take_first(slot: &mut Option<u32>) -> Option<u32> {
    slot.take()
}

fn split(total: u32, a: &mut u32, b: &mut u32) {
    *a = total / 2;
    *b = total - *a;
}

struct Counter {
    hits: u32,
}

impl Counter {
    fn record(&mut self, by: &mut u32) {
        self.hits += *by;
        *by = 0;
    }
}

fn main() {
    let mut n = 5;
    bump(&mut n);
    bump_twice(&mut n);
    println!("{}", n);
    let mut s = String::from("hi");
    shout(&mut s);
    println!("{}", s);
    let mut on = false;
    println!("{} {} {}", toggle(&mut on), toggle(&mut on), on);
    let mut slot = Some(9);
    println!("{:?} {:?} {:?}", take_first(&mut slot), take_first(&mut slot), slot);
    let (mut a, mut b) = (0, 0);
    split(11, &mut a, &mut b);
    println!("{} {}", a, b);
    let mut pending = 4;
    let mut c = Counter { hits: 10 };
    c.record(&mut pending);
    println!("{} {}", c.hits, pending);
    let mut total = 0;
    let mut add = |k: i32| total += k;
    add(3);
    add(4);
    let r = &mut n;
    *r *= 10;
    bump(r);
    println!("{} {}", total, n);
    let mut x = 1;
    let mut y = 2;
    std::mem::swap(&mut x, &mut y);
    println!("{} {}", x, y);
}
