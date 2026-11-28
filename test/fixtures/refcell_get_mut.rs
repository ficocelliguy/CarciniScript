// `RefCell::get_mut` / `Cell::get_mut`: the value in place, through `&mut self`.
use std::cell::{Cell, RefCell};

struct Counter {
    hits: RefCell<Vec<u32>>,
    total: Cell<u32>,
}

impl Counter {
    fn reset(&mut self) {
        self.hits.get_mut().clear();
        *self.total.get_mut() = 0;
    }
}

fn bump(c: &mut RefCell<i32>) {
    *c.get_mut() += 5;
}

fn main() {
    let mut c = RefCell::new(1);
    *c.get_mut() += 10;
    bump(&mut c);
    println!("{}", c.borrow());

    let mut v = RefCell::new(vec![1]);
    v.get_mut().push(2);
    let r = v.get_mut();
    r.push(3);
    println!("{:?}", v.borrow());

    let mut n = Cell::new(4u8);
    *n.get_mut() *= 3;
    println!("{}", n.get());

    let mut k = Counter { hits: RefCell::new(vec![7, 8]), total: Cell::new(15) };
    k.hits.borrow_mut().push(9);
    println!("{:?} {}", k.hits.borrow(), k.total.get());
    k.reset();
    println!("{:?} {}", k.hits.borrow(), k.total.get());
    // (A `&mut` from `get_mut` after a borrow has ended.)
    {
        let g = c.borrow();
        println!("{}", *g);
    }
    *c.get_mut() -= 1;
    println!("{}", c.into_inner());
}
