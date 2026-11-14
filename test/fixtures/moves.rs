// Move checking must accept valid code: rebinding consumed values, references and Copy types
// used twice, moves in exclusive branches, moves followed by reassignment (also in loops), loops
// and labeled blocks that exit after a move, iterator methods that only borrow, shadowing, and
// closures that only borrow.
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
struct Bag {
    items: Vec<String>,
}

impl Bag {
    fn new() -> Self {
        Bag { items: Vec::new() }
    }
    fn first(&self) -> Option<&String> {
        self.items.first()
    }
    fn add(mut self, s: String) -> Self {
        self.items.push(s);
        self
    }
    fn names(&self) -> &Vec<String> {
        &self.items
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pt {
    x: i32,
}

fn total(v: Vec<i32>) -> i32 {
    v.iter().sum()
}

fn pick(v: &[String]) -> Option<&String> {
    v.first()
}

fn main() {
    // Rebinding the result of a consuming method.
    let b = Bag::new();
    let b = b.add("a".to_string()).add(String::from("b"));
    println!("{:?}", b.first());
    // References from functions are Copy.
    let o = b.first();
    let o2 = o;
    println!("{:?} {:?}", o, o2);
    let names = b.names();
    let n2 = names;
    println!("{} {}", names.len(), n2.len());
    let p = pick(&b.items);
    println!("{:?} {:?}", p.unwrap(), p.unwrap());
    // Copy structs.
    let a = Pt { x: 1 };
    let c = a;
    println!("{:?} {:?}", a, c);
    // Moves in exclusive branches.
    let v = vec![1, 2, 3];
    let s = if v.len() > 2 { total(v) } else { total(v) };
    println!("{s}");
    // Move then reassign in a loop.
    let mut acc = Vec::new();
    for i in 0..3 {
        acc.push(i);
        if acc.len() == 2 {
            println!("{}", total(acc));
            acc = Vec::new();
        }
    }
    println!("{:?}", acc);
    // Move in a loop that always breaks.
    let w = vec![4];
    loop {
        if w.is_empty() {
            continue;
        }
        println!("{}", total(w));
        break;
    }
    // Iterators borrowed mutably.
    let mut it = vec![1, 2, 3, 4].into_iter();
    let found = it.any(|x| x == 2);
    let rest: Vec<i32> = it.collect();
    println!("{found} {:?}", rest);
    // Option of a Copy type.
    let m: Option<i32> = Some(3);
    println!("{} {}", m.unwrap(), m.unwrap_or(0));
    // HashMap insertion of clones.
    let key = String::from("k");
    let mut hm = HashMap::new();
    hm.insert(key.clone(), 1);
    hm.insert(key, 2);
    println!("{:?}", hm.len());
    // Shadowing after a move.
    let t = vec![1];
    let u = t;
    let t = u.clone();
    println!("{:?} {:?}", t, u);
    // Struct update via clone.
    let d = Bag::default();
    let e = Bag { items: d.items.clone() };
    println!("{:?} {:?}", d, e);
    // Closures that only borrow.
    let data = vec![1, 2];
    let sum = || data.iter().sum::<i32>();
    println!("{} {:?}", sum(), data);
    // A move in a labeled block that exits early.
    let q = vec![9];
    let r = 'blk: {
        if q.is_empty() {
            break 'blk 0;
        }
        total(q)
    };
    println!("{r}");
    // while with a reassigning body.
    let mut cur = String::from("x");
    while cur.len() < 3 {
        let next = cur + "x";
        cur = next;
    }
    println!("{cur}");
}
