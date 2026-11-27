// `impl<T, I: SliceIndex<[T]>> Index<I> for MyVec<T>` with `type Output = I::Output` (smallvec,
// thin-vec, bumpalo): an element for a `usize`, a slice for a range. Also the `IndexMut` beside it.
use std::ops::{self, Deref, DerefMut, Index, IndexMut};
use std::slice::SliceIndex;

// smallvec: `&(**self)[index]`.
pub struct SV<T> {
    v: Vec<T>,
}
impl<T> ops::Deref for SV<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.v
    }
}
impl<T> ops::DerefMut for SV<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.v
    }
}
impl<T, I: SliceIndex<[T]>> ops::Index<I> for SV<T> {
    type Output = I::Output;
    fn index(&self, index: I) -> &I::Output {
        &(**self)[index]
    }
}
impl<T, I: SliceIndex<[T]>> ops::IndexMut<I> for SV<T> {
    fn index_mut(&mut self, index: I) -> &mut I::Output {
        &mut (&mut **self)[index]
    }
}
impl<T> SV<T> {
    pub fn set(&mut self, i: usize, x: T) -> T {
        std::mem::replace(&mut self[i], x)
    }
    pub fn tail(&self) -> &[T] {
        &self[1..]
    }
}

// thin-vec: `&self.deref()[index]`, `Self::Output` spelled `<I as SliceIndex<[T]>>::Output`.
pub struct TV<T> {
    v: Vec<T>,
}
impl<T> Deref for TV<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.v.as_slice()
    }
}
impl<T> DerefMut for TV<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.v.as_mut_slice()
    }
}
impl<T, I: SliceIndex<[T]>> Index<I> for TV<T> {
    type Output = <I as SliceIndex<[T]>>::Output;
    fn index(&self, index: I) -> &Self::Output {
        &self.deref()[index]
    }
}
impl<T, I: SliceIndex<[T]>> IndexMut<I> for TV<T> {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.deref_mut()[index]
    }
}

// bumpalo: `Index::index(&**self, index)` under a `where` clause.
pub struct BV<T> {
    v: Vec<T>,
}
impl<T> Deref for BV<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.v[..]
    }
}
impl<T> DerefMut for BV<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.v[..]
    }
}
impl<T, I> Index<I> for BV<T>
where
    I: ::core::slice::SliceIndex<[T]>,
{
    type Output = I::Output;
    fn index(&self, index: I) -> &Self::Output {
        Index::index(&**self, index)
    }
}
impl<T, I> IndexMut<I> for BV<T>
where
    I: ::core::slice::SliceIndex<[T]>,
{
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        IndexMut::index_mut(&mut **self, index)
    }
}

// No `Deref`: the impl's own `index` / `index_mut`, called with a number or a range.
pub struct ND<T> {
    v: Vec<T>,
}
impl<T, I: SliceIndex<[T]>> Index<I> for ND<T> {
    type Output = I::Output;
    fn index(&self, index: I) -> &I::Output {
        &self.v[index]
    }
}
impl<T, I: SliceIndex<[T]>> IndexMut<I> for ND<T> {
    fn index_mut(&mut self, index: I) -> &mut I::Output {
        &mut self.v[index]
    }
}

// ascii's `R: SliceIndex<[u8], Output = [u8]>`: a part.
pub struct Bytes {
    v: Vec<u8>,
}
impl Bytes {
    pub fn part<R>(&self, r: R) -> &[u8]
    where
        R: SliceIndex<[u8], Output = [u8]>,
    {
        &self.v[r]
    }
}

// bumpalo's `String`: `Index::index(&**self, index)` over a `str` by a concrete range.
pub struct BS {
    s: String,
}
impl Deref for BS {
    type Target = str;
    fn deref(&self) -> &str {
        &self.s
    }
}
impl Index<ops::RangeInclusive<usize>> for BS {
    type Output = str;
    fn index(&self, index: ops::RangeInclusive<usize>) -> &str {
        Index::index(&**self, index)
    }
}
impl Index<ops::Range<usize>> for BS {
    type Output = str;
    fn index(&self, index: ops::Range<usize>) -> &str {
        &(**self)[index]
    }
}

fn bump<C: IndexMut<usize, Output = i32>>(c: &mut C, i: usize) {
    c[i] += 100;
}

fn head0<C: Index<usize, Output = i32>>(c: &C) -> i32 {
    c[0]
}

#[derive(Debug, Clone, PartialEq)]
struct P {
    x: i32,
}

fn main() {
    let mut s = SV { v: vec![1, 2, 3, 4] };
    println!("{} {:?} {:?} {:?}", s[0], &s[1..3], &s[..], s.tail());
    s[0] = 10;
    s[1] += 5;
    let old = s.set(2, 30);
    println!("{:?} {}", s.v, old);
    let r = 1..3;
    println!("{:?} {}", &s[r], s[1..].len());
    s[..2].reverse();
    println!("{:?}", s.v);

    let mut t = TV { v: vec![P { x: 1 }, P { x: 2 }, P { x: 3 }] };
    t[1].x = 20;
    t[2] = P { x: 30 };
    println!("{:?} {:?} {}", t[0], &t[1..], t[..2].len());
    let fp: &P = &t[0];
    println!("{:?}", fp);

    let mut b = BV { v: vec!["a".to_string(), "b".to_string()] };
    b[0].push('!');
    b[1] = "c".to_string();
    println!("{} {:?} {}", b[0], &b[..], b[1..].concat());
    let n: usize = b.iter().map(|x| x.len()).sum();
    println!("{}", n);

    let mut nd = ND { v: vec![1, 2, 3] };
    let x: i32 = nd[0] + 1;
    nd[1] = 5;
    nd[2] -= 1;
    println!("{} {:?} {} {:?}", x, &nd[1..], nd[2], nd[..=1].to_vec());
    let bs = Bytes { v: vec![1, 2, 3] };
    println!("{:?} {:?}", bs.part(1..), bs.part(..=1));

    let h = BS { s: "hello".to_string() };
    println!("{} {} {}", &h[1..=2], &h[0..4], h.len());
    let w = vec![4, 5, 6];
    let e: &i32 = Index::index(&w, 1);
    println!("{} {:?} {}", e, Index::index(&w[..], 0..2), Index::index("abc", 1..));

    bump(&mut s, 3);
    bump(&mut nd, 0);
    println!("{:?} {:?} {} {}", s.v, nd.v, head0(&s), head0(&nd));
}
