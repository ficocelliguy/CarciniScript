// Raw pointers to numbers, strings and bools held in struct fields and returned from functions:
// `*s.p` reads and writes the place, `&mut s.field as *mut _` / `&mut v[i] as *mut _` /
// `Box::into_raw(Box::new(5))` / a `-> *mut i32` call are sources, a copy of the struct shares
// the place, and `p.as_mut()` gives a `&mut` that writes through (`None` for null).
use std::ptr;

struct Counter {
    p: *mut i32,
    name: *const String,
}

#[derive(Clone, Copy)]
struct Flag {
    on: *mut bool,
}

struct Holder {
    count: i32,
    v: Vec<i32>,
    label: String,
}

impl Holder {
    fn count_ptr(&mut self) -> *mut i32 {
        &mut self.count
    }
    fn label_ptr(&mut self) -> *mut String {
        &mut self.label
    }
}

fn pick(a: &mut i32, b: &mut i32, first: bool) -> *mut i32 {
    if first { a } else { b }
}

fn bump(c: &Counter) {
    unsafe {
        *c.p += 1;
    }
}

fn main() {
    // Fields pointing at locals, written through an immutable struct.
    let mut x = 5;
    let s = String::from("hi");
    let mut c = Counter { p: &mut x, name: &s };
    unsafe {
        *c.p += 1;
        println!("{} {}", *c.p, *c.name);
    }
    bump(&c);
    println!("{x}");
    let c2 = Counter { p: c.p, name: ptr::null() };
    unsafe {
        *c2.p *= 10;
    }
    println!("{x} {} {}", c2.name.is_null(), c.name.is_null());
    let mut other = 1;
    c.p = &mut other;
    unsafe {
        *c.p += 1;
    }
    println!("{x} {other} {}", ptr::eq(c.p, c2.p));

    // A copy of the struct shares the place.
    let mut on = false;
    let f1 = Flag { on: &mut on };
    let f2 = f1;
    unsafe {
        *f2.on = true;
        println!("{} {}", *f1.on, on);
    }

    // Sources: a field, an element, a method returning a pointer, a box.
    let mut h = Holder { count: 1, v: vec![1, 2, 3], label: String::from("a") };
    let q = h.count_ptr();
    unsafe {
        *q += 41;
    }
    println!("{}", h.count);
    let e = &mut h.v[1] as *mut i32;
    unsafe {
        *e = 20;
    }
    println!("{:?}", h.v);
    let f = &mut h.count as *mut _;
    unsafe {
        *f -= 2;
    }
    println!("{}", h.count);
    let lp = h.label_ptr();
    unsafe {
        (*lp).push('b');
        *lp += "c";
    }
    println!("{}", h.label);
    let held = Counter { p: h.count_ptr(), name: &h.label };
    unsafe {
        *held.p = 0;
    }
    println!("{}", h.count);

    // Returned from a function, then `as_mut()`.
    let mut y = 3;
    let mut z = 4;
    let r = pick(&mut y, &mut z, false);
    unsafe {
        *r += 1;
    }
    println!("{y} {z}");
    if let Some(n) = unsafe { r.as_mut() } {
        *n += 100;
    }
    println!("{z}");
    let nul: *mut i32 = ptr::null_mut();
    println!("{:?} {}", unsafe { nul.as_mut() }, unsafe { nul.as_mut() }.is_none());
    if let Some(n) = unsafe { c2.p.as_mut() } {
        *n = 7;
    }
    println!("{x}");

    // A boxed number behind a field.
    let b = Box::into_raw(Box::new(5));
    let c3 = Counter { p: b, name: &s };
    unsafe {
        *c3.p += 1;
        println!("{}", *b);
        drop(Box::from_raw(b));
    }
}
