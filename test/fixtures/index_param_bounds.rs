// Indexing through a type parameter's bound (`C: IndexMut<Ix, Output = Entry>`, `C: Index<usize,
// Output = i32>`), given a Vec, a VecDeque or your own type with `impl Index` / `IndexMut`; and
// `Range::from(1..3)` / `RangeInclusive::from(..)` (std's `impl<T> From<T> for T`).
use std::collections::VecDeque;
use std::ops::{Index, IndexMut, Range, RangeInclusive};

#[derive(Debug, Clone)]
struct Entry {
    prev: Option<usize>,
}

struct Arena {
    items: Vec<Entry>,
}

impl Index<u32> for Arena {
    type Output = Entry;
    fn index(&self, i: u32) -> &Entry {
        &self.items[i as usize]
    }
}

impl IndexMut<u32> for Arena {
    fn index_mut(&mut self, i: u32) -> &mut Entry {
        &mut self.items[i as usize]
    }
}

fn link<C, Ix>(c: &mut C, start: Ix, push_ix: usize)
where
    C: IndexMut<Ix, Output = Entry>,
    Ix: Copy,
{
    let e = &mut c[start];
    e.prev = Some(push_ix);
    c[start].prev = Some(push_ix + 1);
}

fn total<C: Index<usize, Output = i32>>(c: &C, n: usize) -> i32 {
    let mut t = 0;
    for i in 0..n {
        t += c[i];
    }
    t
}

fn prev_of<C: Index<u32, Output = Entry>>(c: &C, i: u32) -> Option<usize> {
    c[i].prev
}

fn main() {
    let mut a = Arena { items: vec![Entry { prev: None }; 3] };
    link(&mut a, 1u32, 7);
    let mut v = vec![Entry { prev: None }; 2];
    link(&mut v, 0usize, 3);
    println!("{:?}", a.items);
    println!("{:?} {:?}", v, prev_of(&a, 1));
    let d: VecDeque<i32> = (1..=4).collect();
    println!("{} {}", total(&vec![4, 5, 6], 3), total(&d, 4));

    let r = Range::from(1..3);
    let s = "hello";
    println!("{:?} {}", r, &s[r.clone()]);
    let q: Range<usize> = From::from(2..4);
    let w = vec![1, 2, 3, 4, 5];
    println!("{:?}", &w[q]);
    let ri = RangeInclusive::from(1..=2);
    println!("{:?} {:?}", ri, &w[ri.clone()]);
}
