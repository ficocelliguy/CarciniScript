// Raw pointer arithmetic inside one buffer: `as_ptr()` / `as_mut_ptr()` of a Vec, array or slice of
// numbers, strings or bools is a pointer at element 0; `add` / `sub` / `offset` move it, `offset_from`
// measures, `==` / `<` compare positions, and `*p` / `read` / `write` / `as_mut` reach the element.
use std::ptr;
struct Cursor {
    at: *mut u8,
    end: *mut u8,
}
impl Cursor {
    fn next(&mut self) -> Option<u8> {
        if self.at == self.end {
            return None;
        }
        unsafe {
            let b = *self.at;
            self.at = self.at.add(1);
            Some(b)
        }
    }
}
fn fill(mut p: *mut u32, n: usize, x: u32) {
    for _ in 0..n {
        unsafe {
            *p = x;
            p = p.add(1);
        }
    }
}
fn sum(p: *const i32, n: usize) -> i32 {
    let mut s = 0;
    for i in 0..n {
        unsafe { s += *p.add(i); }
    }
    s
}
fn main() {
    let mut bytes = vec![10u8, 20, 30];
    let start = bytes.as_mut_ptr();
    let mut c = Cursor { at: start, end: unsafe { start.add(3) } };
    while let Some(b) = c.next() {
        print!("{b} ");
    }
    println!();
    let mut words = [0u32; 5];
    fill(unsafe { words.as_mut_ptr().add(1) }, 3, 7);
    println!("{:?}", words);
    let names = vec![String::from("a"), String::from("b")];
    let np = names.as_ptr();
    unsafe {
        println!("{} {}", *np, *np.add(1));
    }
    let mut flags = [false; 3];
    let fp = flags.as_mut_ptr();
    unsafe {
        *fp.offset(2) = true;
        if let Some(f) = fp.add(1).as_mut() {
            *f = true;
        }
    }
    println!("{:?}", flags);
    let a = bytes.as_ptr();
    let b = unsafe { a.add(1).sub(1) };
    println!("{} {}", ptr::eq(a, b), a == b);
    let s = &mut bytes[1..];
    let sp = s.as_mut_ptr();
    unsafe { *sp.add(1) = 99; }
    println!("{:?}", bytes);
    let mut p = words.as_ptr();
    let end = unsafe { p.add(words.len()) };
    let mut total = 0;
    while p < end {
        unsafe {
            total += *p;
            p = p.add(1);
        }
    }
    println!("{total}");
    let mut v = vec![1, 2, 3, 4];
    let p = v.as_mut_ptr();
    unsafe {
        *p.add(2) = 30;
        let q = p.add(3);
        *q += 1;
        println!("{}", q.offset_from(p));
        let r = q.sub(1);
        println!("{} {}", *r, r.read());
        p.write(10);
        println!("{}", *p.offset(1));
    }
    println!("{:?} {}", v, sum(v.as_ptr(), v.len()));
    let a = [5u8, 6, 7];
    let ap = a.as_ptr();
    println!("{}", unsafe { *ap.add(1) });
    // A pointer made from an element moves too.
    let mut w = vec![1u16, 2, 3, 4];
    let e = &mut w[1] as *mut u16;
    unsafe {
        *e.add(2) += 40;
        println!("{} {}", *e.sub(1), e.add(2).offset_from(e));
    }
    println!("{:?}", w);
}
