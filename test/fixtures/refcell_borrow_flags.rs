use std::cell::RefCell;
use std::rc::Rc;

struct Bank {
    log: RefCell<Vec<String>>,
}

impl Bank {
    fn note(&self, s: &str) {
        self.log.borrow_mut().push(s.to_string());
    }
}

fn main() {
    let c = RefCell::new(vec![1, 2]);
    // Temporaries end with their statement.
    c.borrow_mut().push(3);
    let n = c.borrow().len();
    c.borrow_mut().push(n as i32);
    println!("{:?}", c.borrow());

    // A guard held by a `let` until its block ends.
    {
        let g = c.borrow();
        let h = c.borrow();
        println!("{} {}", g.len(), h[0]);
        println!("{}", c.try_borrow_mut().is_err());
        if let Err(e) = c.try_borrow_mut() {
            println!("{} | {:?}", e, e);
        }
    }
    println!("{}", c.try_borrow_mut().is_ok());

    // `drop` ends it early.
    let mut m = c.borrow_mut();
    m.push(9);
    match c.try_borrow() {
        Ok(v) => println!("ok {:?}", v),
        Err(e) => println!("{} | {:?}", e, e),
    }
    drop(m);
    println!("{:?}", c.try_borrow().map(|v| v.len()));

    // Through an `Rc`, and a method borrowing a field.
    let shared = Rc::new(RefCell::new(0));
    let other = Rc::clone(&shared);
    for i in 0..3 {
        let mut g = shared.borrow_mut();
        *g += i;
    }
    *other.borrow_mut() += 10;
    println!("{}", shared.borrow());
    let bank = Bank { log: RefCell::new(Vec::new()) };
    bank.note("a");
    bank.note("b");
    println!("{:?}", bank.log.borrow());

    // A second `borrow_mut` while one is held panics.
    let first = c.borrow_mut();
    let second = c.borrow_mut();
    println!("unreachable {} {}", first.len(), second.len());
}
