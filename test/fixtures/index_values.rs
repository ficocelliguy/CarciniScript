// Indexing: ranges as values that reach `[..]` through a tuple pattern, an `if` of tuples, an
// array or `vec!` of ranges (and still iterate), and two `Index` impls on one type.
fn word(s: &str, k: bool) -> &str {
    let (r, _tag) = if k { (0..5, 0u8) } else { (6..11, 1u8) };
    &s[r]
}

fn main() {
    let s = "hello world";
    let (b, n) = (1..3, 7u8);
    println!("{} {}", &s[b.clone()], n);
    println!("{} {}", word(s, true), word(s, false));
    let bytes = s.as_bytes();
    let (r, _) = (2..4, ());
    println!("{:?}", &bytes[r]);
    let parts = [0..2, 4..6];
    for p in parts.iter() {
        print!("{} ", &s[p.clone()]);
    }
    println!();
    let spans = vec![0..1, 3..5, 8..11];
    let got: Vec<&str> = spans.iter().map(|r| &s[r.clone()]).collect();
    println!("{:?}", got);
    let (a, z) = (0..3, 10..12);
    let total: u32 = a.chain(z).sum();
    println!("{}", total);
    for (i, r) in [1..2, 2..4].into_iter().enumerate() {
        println!("{} {:?} {}", i, r, r.len());
    }
    snaps();
}

// `impl Index<usize>` beside a generic `impl<U: Id> Index<U>`: the concrete one serves `usize`.
pub trait Id: Copy {
    fn index(&self) -> usize;
}
#[derive(Clone, Copy)]
pub struct CoreId(u32);
impl Id for CoreId {
    fn index(&self) -> usize {
        self.0 as usize
    }
}
pub struct Snap<T> {
    v: Vec<T>,
}
impl<T> std::ops::Index<usize> for Snap<T> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        &self.v[i]
    }
}
impl<T, U: Id> std::ops::Index<U> for Snap<T> {
    type Output = T;
    fn index(&self, id: U) -> &T {
        &self.v[id.index() + 1]
    }
}

pub fn snaps() {
    let s = Snap { v: vec!["a".to_string(), "b".to_string(), "c".to_string()] };
    let id = CoreId(1);
    println!("{} {} {}", s[0], s[id.index()], s[id]);
}
