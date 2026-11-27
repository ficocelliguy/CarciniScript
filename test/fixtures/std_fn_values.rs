// Std constructors and methods named as function values: `.map(Box::new)`,
// `.unwrap_or_else(String::new)`, `Vec::new`, `BTreeMap::new`, `VecDeque::new`, `Rc::new`,
// `Rc::clone`, `Ordering::reverse`, `Box::as_ref`, `Vec::as_slice`, `Default::default`.
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::rc::Rc;

fn main() {
    let v: Vec<u32> = vec![3, 1, 2];
    let boxed: Vec<Box<u32>> = v.iter().copied().map(Box::new).collect();
    let refs: Vec<&u32> = boxed.iter().map(Box::as_ref).collect();
    println!("{boxed:?} {refs:?}");
    let s: String = None.unwrap_or_else(String::new);
    let w: Vec<u8> = None.unwrap_or_else(Vec::new);
    let m: BTreeMap<u8, u8> = None.unwrap_or_else(BTreeMap::new);
    let q: VecDeque<u8> = Some(VecDeque::from(vec![1u8])).unwrap_or_else(VecDeque::new);
    let h: HashSet<u8> = None.unwrap_or_else(HashSet::default);
    let n: u8 = None.unwrap_or_else(Default::default);
    println!("{s:?} {w:?} {m:?} {q:?} {} {n}", h.len());
    let rcs: Vec<Rc<u32>> = v.iter().copied().map(std::rc::Rc::new).collect();
    let more: Vec<Rc<u32>> = rcs.iter().map(Rc::clone).collect();
    println!("{:?} {}", more, rcs.len());
    let o: Option<Ordering> = Some(Ordering::Less).map(Ordering::reverse);
    let eq: Vec<bool> = [Ordering::Less, Ordering::Equal].into_iter().map(Ordering::is_eq).collect();
    println!("{o:?} {eq:?}");
    let vs: Vec<Vec<u8>> = vec![vec![1, 2], vec![3]];
    let sl: Vec<&[u8]> = vs.iter().map(Vec::as_slice).collect();
    println!("{sl:?}");
    let lens: Vec<usize> = ["ab", "c"].iter().map(|x| x.to_string()).map(|x| x.len()).collect();
    println!("{lens:?}");
}
