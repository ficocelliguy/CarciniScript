// Std surface from the Bitburner corpus (GAPS.md section D): `split_first` / `split_last`,
// `BinaryHeap::into_iter`, `VecDeque` from a `Vec`, the `hash_map::Entry` enum, `Rc::default`,
// `checked_add_signed`, `is_multiple_of`, `iter::repeat_n`, std methods as values, and
// `std::error::Error::source(&e)`.
use std::collections::hash_map::Entry;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::error::Error;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug)]
struct E(String);
impl std::fmt::Display for E {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for E {}

fn main() {
    let v = vec![1, 2, 3];
    if let Some((last, rest)) = v.split_last() {
        println!("{} {:?}", last, rest);
    }
    if let Some((first, rest)) = v.split_first() {
        println!("{} {:?}", first, rest);
    }
    let mut h = BinaryHeap::new();
    h.push(3);
    h.push(1);
    h.push(2);
    let mut all: Vec<i32> = h.into_iter().collect();
    all.sort();
    println!("{:?}", all);
    let d: VecDeque<i32> = VecDeque::from(vec![1, 2]);
    let d2: VecDeque<i32> = vec![3, 4].into();
    println!("{:?} {:?}", d, d2);
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("a", 1);
    for k in ["a", "b"] {
        match m.entry(k) {
            Entry::Occupied(mut e) => {
                *e.get_mut() += 10;
            }
            Entry::Vacant(e) => {
                e.insert(5);
            }
        }
    }
    if let Entry::Vacant(e) = m.entry("c") {
        e.insert(7);
    }
    let mut ks: Vec<_> = m.iter().collect();
    ks.sort();
    println!("{:?}", ks);
    let r: Rc<RefCell<Vec<i32>>> = Rc::default();
    r.borrow_mut().push(1);
    println!("{:?}", r.borrow());
    let x: usize = 5;
    println!("{:?} {:?}", x.checked_add_signed(-2), x.checked_add_signed(-9));
    println!("{} {}", 12u64.is_multiple_of(4), 13u32.is_multiple_of(4));
    let s: Vec<i32> = std::iter::repeat_n(7, 3).collect();
    println!("{:?}", s);
    let lens: Vec<usize> = vec![vec![1], vec![1, 2]].iter().map(Vec::len).collect();
    println!("{:?}", lens);
    let cs: String = vec![104u8, 105].into_iter().map(char::from).collect();
    println!("{}", cs);
    let fl: Vec<f64> = vec![1i32, 2].into_iter().map(Into::into).collect();
    println!("{:?}", fl);
    let e: Box<dyn std::error::Error> = Box::new(E("bad".into()));
    println!("{:?}", std::error::Error::source(e.as_ref()).is_none());
    let own = E("own".into());
    println!("{:?} {:?}", std::error::Error::source(&own).is_none(), own.source().is_none());
}
