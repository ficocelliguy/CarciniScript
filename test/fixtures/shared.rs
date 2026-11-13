// Shared ownership and interior mutability: Rc (cloned handles share), RefCell (borrow,
// borrow_mut through `*`, methods and bindings, replace, take), Cell (get/set), Rc<RefCell<T>>
// captured by closures and stored in structs.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug)]
struct Account {
    balance: RefCell<i64>,
    log: RefCell<Vec<String>>,
    visits: Cell<u32>,
}

impl Account {
    fn deposit(&self, amount: i64) {
        *self.balance.borrow_mut() += amount;
        self.log.borrow_mut().push(format!("+{}", amount));
        self.visits.set(self.visits.get() + 1);
    }
}

struct Node {
    name: String,
    children: RefCell<Vec<Rc<Node>>>,
}

fn main() {
    let shared = Rc::new(RefCell::new(vec![1, 2]));
    let other = Rc::clone(&shared);
    let third = shared.clone();
    other.borrow_mut().push(3);
    third.borrow_mut().push(4);
    println!("{:?} {}", shared.borrow(), shared.borrow().len());

    let counter = Rc::new(Cell::new(0));
    let c2 = Rc::clone(&counter);
    let bump = move || c2.set(c2.get() + 10);
    bump();
    bump();
    println!("{}", counter.get());

    let acct = Account { balance: RefCell::new(100), log: RefCell::new(Vec::new()), visits: Cell::new(0) };
    acct.deposit(25);
    acct.deposit(-5);
    println!("{} {:?} {}", acct.balance.borrow(), acct.log.borrow(), acct.visits.get());
    let old = acct.balance.replace(0);
    let taken = acct.log.take();
    println!("{} {:?} {:?}", old, taken, acct);

    let total = RefCell::new(0);
    {
        let mut t = total.borrow_mut();
        *t += 7;
        *t *= 2;
    }
    println!("{}", total.borrow());

    let root = Rc::new(Node { name: "root".to_string(), children: RefCell::new(Vec::new()) });
    let leaf = Rc::new(Node { name: "leaf".to_string(), children: RefCell::new(Vec::new()) });
    root.children.borrow_mut().push(Rc::clone(&leaf));
    root.children.borrow_mut().push(leaf.clone());
    let names: Vec<String> = root.children.borrow().iter().map(|n| n.name.clone()).collect();
    println!("{} {:?}", root.name, names);
}
