// `X::from_iter` as a function value: a closure collecting into `X`.
use std::collections::{BTreeMap, HashSet};
use std::iter::FromIterator;
fn main() {
    let v: Vec<Vec<u8>> = vec![vec![1, 2], vec![3]].into_iter().map(|x| x.into_iter()).map(Vec::from_iter).collect();
    println!("{:?}", v);
    let s: Vec<HashSet<u8>> = vec![vec![1u8, 1]].into_iter().map(HashSet::from_iter).collect();
    println!("{:?}", s);
    let m: Vec<BTreeMap<u8, char>> = vec![vec![(2, 'b'), (1, 'a')]].into_iter().map(std::collections::BTreeMap::from_iter).collect();
    println!("{:?}", m);
    let t: Vec<String> = vec![vec!['h', 'i']].into_iter().map(String::from_iter).collect();
    println!("{:?}", t);
    let w: Option<Vec<u8>> = Some([4u8, 5]).map(Vec::<u8>::from_iter);
    println!("{:?}", w);
}
