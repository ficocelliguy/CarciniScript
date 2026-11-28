// Impls of one trait for std types that are one class in JS, with the same bodies: `Cell<T>` and
// `RefCell<T>` (both the runtime's cell), and std's iterators (`vec::IntoIter`,
// `array::IntoIter`, `vec::Drain`, `string::Drain`, `slice::Iter`: all the runtime's `Iter`, as
// in itertools' `PeekingNext`). One impl serves for all of them.
use std::cell::{Cell, RefCell};
use std::slice::Iter;

pub trait Schema {
    fn declaration() -> String;
}
impl Schema for u8 {
    fn declaration() -> String {
        "u8".to_string()
    }
}
impl Schema for bool {
    fn declaration() -> String {
        "bool".to_string()
    }
}
impl<T> Schema for Cell<T>
where
    T: Schema + Copy,
{
    fn declaration() -> String {
        format!("cell of {}", T::declaration())
    }
}
impl<T> Schema for RefCell<T>
where
    T: Schema + Sized,
{
    fn declaration() -> String {
        format!("cell of {}", T::declaration())
    }
}

fn schema_of<S: Schema>() -> String {
    S::declaration()
}

pub trait Describe {
    fn describe(&self) -> String;
}
impl Describe for Cell<u8> {
    fn describe(&self) -> String {
        format!("holds {:?}", self)
    }
}
impl Describe for RefCell<u8> {
    fn describe(&self) -> String {
        format!("holds {:?}", self)
    }
}

pub trait PeekingNext: Iterator {
    fn peeking_next<F>(&mut self, accept: F) -> Option<Self::Item>
    where
        F: FnOnce(&Self::Item) -> bool;
}
impl<T: Clone> PeekingNext for std::vec::IntoIter<T> {
    fn peeking_next<F>(&mut self, accept: F) -> Option<T>
    where
        F: FnOnce(&T) -> bool,
    {
        let saved = self.clone();
        match self.next() {
            Some(x) if accept(&x) => Some(x),
            _ => {
                *self = saved;
                None
            }
        }
    }
}
impl<T: Clone, const N: usize> PeekingNext for std::array::IntoIter<T, N> {
    fn peeking_next<F>(&mut self, accept: F) -> Option<T>
    where
        F: FnOnce(&T) -> bool,
    {
        let saved = self.clone();
        match self.next() {
            Some(x) if accept(&x) => Some(x),
            _ => {
                *self = saved;
                None
            }
        }
    }
}
impl<'a, T> PeekingNext for Iter<'a, T> {
    fn peeking_next<F>(&mut self, accept: F) -> Option<&'a T>
    where
        F: FnOnce(&&'a T) -> bool,
    {
        let saved = self.clone();
        match self.next() {
            Some(x) if accept(&x) => Some(x),
            _ => {
                *self = saved;
                None
            }
        }
    }
}

pub trait Counted {
    fn counted(self) -> usize;
}
impl<'a, T> Counted for std::vec::Drain<'a, T> {
    fn counted(self) -> usize {
        self.count()
    }
}
impl<'a> Counted for std::string::Drain<'a> {
    fn counted(self) -> usize {
        self.count()
    }
}

fn take_while_peeking<I: PeekingNext>(it: &mut I, f: impl Fn(&I::Item) -> bool) -> Vec<I::Item> {
    let mut out = Vec::new();
    while let Some(x) = it.peeking_next(|x| f(x)) {
        out.push(x);
    }
    out
}

fn main() {
    println!("{}", <Cell<u8>>::declaration());
    println!("{}", <RefCell<bool>>::declaration());
    println!("{}", schema_of::<Cell<bool>>());
    println!("{}", schema_of::<RefCell<u8>>());

    let c = Cell::new(3u8);
    let r = RefCell::new(4u8);
    println!("{} / {}", c.describe(), r.describe());

    let mut v = vec![1, 2, 3, 10, 4].into_iter();
    println!("{:?} then {:?}", take_while_peeking(&mut v, |x| *x < 5), v.next());
    let mut a = [5, 6, 0, 7].into_iter();
    println!("{:?} then {:?}", take_while_peeking(&mut a, |x| *x > 0), a.collect::<Vec<_>>());
    let xs = vec!["a", "bb", "ccc", "d"];
    let mut s = xs.iter();
    println!("{:?} then {:?}", take_while_peeking(&mut s, |x| x.len() < 3), s.next());

    let mut w = vec![1, 2, 3, 4];
    let mut t = String::from("hello");
    println!("{} {} {:?} {:?}", w.drain(1..3).counted(), t.drain(..2).counted(), w, t);
}
