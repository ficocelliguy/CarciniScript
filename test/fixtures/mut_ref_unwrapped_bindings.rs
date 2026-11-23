// `&mut` bindings from an unwrapped std call (`v[i].as_mut().unwrap()`, `m.get_mut(..).expect(..)`)
// and from a tuple of them (`if let (Some(dst), Some(src)) = (a.get_mut(..), w.get(..))`):
// written through without `mut`.
use std::collections::HashMap;
#[derive(Debug)]
struct L { next: u8 }
fn main() {
    let mut v = vec![Some(L { next: 0 }), None];
    let links = v[0].as_mut().unwrap();
    links.next = 1;
    let mut a = vec![0u8; 4];
    let w = vec![1u8, 2];
    if let (Some(dst), Some(src)) = (a.get_mut(0..2), w.get(0..2)) {
        dst.copy_from_slice(src)
    }
    let mut s = L { next: 3 };
    let (Some(p), b) = (Some(&mut s), 2) else { return };
    p.next += b;
    let mut m = HashMap::new();
    m.insert(1, L { next: 0 });
    let e = m.get_mut(&1).expect("k");
    e.next = 9;
    match (v.last_mut(), 1) {
        (Some(Some(l)), k) => l.next = k,
        _ => {}
    }
    let mut t = vec![L { next: 1 }];
    let f = t.first_mut().unwrap();
    f.next += 1;
    println!("{:?} {:?} {:?} {:?} {:?}", v, a, s, m[&1], t);
}
