// Raw pointers: `*const T` / `*mut T` to locals, structs and boxed nodes, `ptr::null`,
// `is_null`, `as_ref` / `as_mut`, `ptr::eq`, `Box::into_raw` / `from_raw`, and same-size
// `transmute`.
use std::ptr;

struct Node {
    val: i32,
    next: *mut Node,
}

/// A queue whose tail is a raw pointer into the chain of boxes.
struct Queue {
    head: *mut Node,
    tail: *mut Node,
    len: usize,
}

impl Queue {
    fn new() -> Self {
        Queue { head: ptr::null_mut(), tail: ptr::null_mut(), len: 0 }
    }

    fn push(&mut self, val: i32) {
        let node = Box::into_raw(Box::new(Node { val, next: ptr::null_mut() }));
        if self.tail.is_null() {
            self.head = node;
        } else {
            unsafe {
                (*self.tail).next = node;
            }
        }
        self.tail = node;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<i32> {
        if self.head.is_null() {
            return None;
        }
        let first = unsafe { Box::from_raw(self.head) };
        self.head = first.next;
        if self.head.is_null() {
            self.tail = ptr::null_mut();
        }
        self.len -= 1;
        Some(first.val)
    }

    fn sum(&self) -> i32 {
        let mut total = 0;
        let mut cur: *const Node = self.head;
        while let Some(n) = unsafe { cur.as_ref() } {
            total += n.val;
            cur = n.next;
        }
        total
    }
}

fn bump(p: *mut i32) {
    unsafe {
        *p += 1;
    }
}

fn read_or(p: *const i32, default: i32) -> i32 {
    if p.is_null() { default } else { unsafe { *p } }
}

fn main() {
    // Pointers to a local see every write.
    let mut x = 5;
    let p: *const i32 = &x;
    let q = &mut x as *mut i32;
    unsafe {
        *q = 10;
        println!("{}", *p);
    }
    bump(q);
    bump(&mut x);
    println!("{} {} {}", x, read_or(p, -1), read_or(ptr::null(), -1));
    let r = unsafe { &mut *q };
    *r += 5;
    println!("{x}");

    // Re-pointing.
    let mut y = 1;
    let mut z = 2;
    let mut pp: *mut i32 = &mut y;
    unsafe {
        *pp += 10;
    }
    pp = &mut z;
    unsafe {
        *pp += 10;
    }
    println!("{y} {z} {}", unsafe { pp.read() });

    // Null and identity.
    let n: *const i32 = ptr::null();
    println!("{} {}", n.is_null(), p.is_null());
    println!("{:?} {:?}", unsafe { n.as_ref() }, unsafe { p.as_ref() });
    let a = 1;
    let b = 1;
    println!("{} {}", ptr::eq(&a, &a), ptr::eq(&a, &b));

    // A chain of nodes on the stack.
    let mut second = Node { val: 2, next: ptr::null_mut() };
    let first = Node { val: 1, next: &mut second };
    let mut cur: *const Node = &first;
    while !cur.is_null() {
        unsafe {
            println!("node {}", (*cur).val);
            cur = (*cur).next;
        }
    }
    let np: *mut Node = &mut second;
    if let Some(n) = unsafe { np.as_mut() } {
        n.val += 40;
    }
    println!("{} {}", second.val, ptr::eq(first.next, np));

    // Boxes handed out as raw pointers and taken back.
    let raw = Box::into_raw(Box::new(7));
    unsafe {
        *raw += 1;
        let back = Box::from_raw(raw);
        println!("{back}");
    }
    let mut queue = Queue::new();
    for v in [3, 4, 5] {
        queue.push(v);
    }
    println!("len {} sum {}", queue.len, queue.sum());
    while let Some(v) = queue.pop() {
        print!("{v} ");
    }
    println!("| {} {}", queue.len, queue.head.is_null());

    // Reinterpreting bits.
    let bits: u32 = unsafe { std::mem::transmute(1.5f32) };
    let f: f64 = unsafe { std::mem::transmute(0x4000_0000_0000_0000u64) };
    let c: char = unsafe { std::mem::transmute(0x41u32) };
    let i: i32 = unsafe { std::mem::transmute(u32::MAX) };
    let back = unsafe { std::mem::transmute::<f32, u32>(-2.0) };
    let byte: i8 = unsafe { std::mem::transmute(200u8) };
    println!("{bits:#x} {f} {c} {i} {back:#x} {byte}");
}
