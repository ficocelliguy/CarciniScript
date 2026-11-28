// `c[i].push(..)` and other `String` methods through a `C: IndexMut<Ix, Output = String>` bound:
// the string is changed in place, in your type or in a `Vec<String>`.
use std::ops::{Index, IndexMut};

struct Names(Vec<String>);

impl Index<usize> for Names {
    type Output = String;
    fn index(&self, i: usize) -> &String {
        &self.0[i]
    }
}

impl IndexMut<usize> for Names {
    fn index_mut(&mut self, i: usize) -> &mut String {
        &mut self.0[i]
    }
}

struct Lists(Vec<Vec<u32>>);

impl Index<usize> for Lists {
    type Output = Vec<u32>;
    fn index(&self, i: usize) -> &Vec<u32> {
        &self.0[i]
    }
}

impl IndexMut<usize> for Lists {
    fn index_mut(&mut self, i: usize) -> &mut Vec<u32> {
        &mut self.0[i]
    }
}

fn shout<C: IndexMut<usize, Output = String>>(c: &mut C, i: usize) {
    c[i].push('!');
    c[i].push_str("?");
    c[i].insert(0, '<');
    let n = c[i].len();
    c[i].truncate(n - 1);
}

fn reset<C>(c: &mut C, i: usize, j: usize)
where
    C: IndexMut<usize, Output = String>,
{
    let from = c[j].clone();
    c[i].clear();
    c[i].push_str(&from);
    c[i] += "+";
    println!("{} {}", c[i].len(), c[i].to_uppercase());
}

fn width<C: Index<usize, Output = String>>(c: &C, i: usize) -> usize {
    c[i].len()
}

fn grow<C: IndexMut<usize, Output = Vec<u32>>>(c: &mut C, i: usize) {
    c[i].push(9);
    c[i].sort();
}

fn main() {
    let mut n = Names(vec!["a".into(), "b".into()]);
    shout(&mut n, 1);
    println!("{} {}", n[0], n[1]);
    reset(&mut n, 0, 1);
    println!("{} {} {}", n[0], n[1], width(&n, 0));

    let mut v = vec![String::from("x"), String::from("yz")];
    shout(&mut v, 0);
    reset(&mut v, 1, 0);
    println!("{:?} {}", v, width(&v, 1));

    let mut l = Lists(vec![vec![3, 1], vec![]]);
    grow(&mut l, 0);
    grow(&mut l, 1);
    println!("{:?} {:?}", l[0], l[1]);
}
