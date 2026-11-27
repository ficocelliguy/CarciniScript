// Derives over std field types: a `slice::IterMut` (and `Enumerate` of one) is `Debug`; a field
// typed by a parameter bounded by an `Fn` trait leaves the derive bounded on that parameter (as
// rustc's are); a `TypeId` field is `Copy`, `Hash` and `Ord`; `dyn Any` is `Debug`.
use std::any::{Any, TypeId};
use std::collections::HashSet;

#[derive(Debug)]
struct Cursor<'a, T> {
    inner: std::slice::IterMut<'a, T>,
}

#[derive(Debug)]
struct Numbered<'a, T: 'a> {
    _iter: std::iter::Enumerate<std::slice::IterMut<'a, T>>,
}

#[derive(Debug)]
struct DrainFilter<'a, T: 'a, F>
where
    F: FnMut(&mut T) -> bool,
{
    vec: &'a mut Vec<T>,
    idx: usize,
    pred: F,
}

impl<'a, T, F: FnMut(&mut T) -> bool> DrainFilter<'a, T, F> {
    fn run(&mut self) -> usize {
        let mut n = 0;
        while self.idx < self.vec.len() {
            if (self.pred)(&mut self.vec[self.idx]) {
                n += 1;
            }
            self.idx += 1;
        }
        n
    }
}

#[derive(Clone, Copy)]
struct Memo<F: Fn(u32) -> u32> {
    f: F,
    seed: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
struct AnyValueId {
    type_id: TypeId,
    name: &'static str,
}

#[derive(Debug)]
struct Holder {
    tag: u8,
    extra: Option<Box<dyn Any>>,
}

fn double(x: u32) -> u32 {
    x * 2
}

fn main() {
    let mut v = vec![1, 2, 3];
    let mut c = Cursor { inner: v.iter_mut() };
    if let Some(x) = c.inner.next() {
        *x += 10;
    }
    println!("{:?}", c);
    let mut f = vec![1.5f64, 2.0];
    let mut fi = f.iter_mut();
    *fi.next().unwrap() += 1.0;
    println!("{:?}", fi);
    let mut ps = vec![(1, 'a'), (2, 'b')];
    let pi = ps.iter_mut();
    println!("{:?}", pi);
    let mut w = vec![4u8, 5];
    let _n = Numbered { _iter: w.iter_mut().enumerate() };

    let mut d = vec![1, 2, 3, 4, 5];
    let mut df = DrainFilter { vec: &mut d, idx: 0, pred: |x: &mut i32| { *x *= 10; *x > 20 } };
    println!("{}", df.run());
    println!("{:?}", d);

    let m = Memo { f: double, seed: 4 };
    let m2 = m;
    let m3 = m.clone();
    println!("{} {} {}", (m.f)(m.seed), (m2.f)(1), (m3.f)(m3.seed + 1));
    let k = 3;
    let mc = Memo { f: move |x| x + k, seed: 1 };
    let mc2 = mc;
    println!("{} {}", (mc.f)(mc.seed), (mc2.f)(2));

    let a = AnyValueId { type_id: TypeId::of::<u8>(), name: "u8" };
    let b = a;
    let c2 = AnyValueId { type_id: TypeId::of::<String>(), name: "String" };
    println!("{} {} {}", a == b, a == c2, a.name);
    let set: HashSet<AnyValueId> = [a, b, c2].into_iter().collect();
    println!("{}", set.len());

    let h = Holder { tag: 1, extra: None };
    println!("{:?}", h);
    let h2 = Holder { tag: 2, extra: Some(Box::new(5u8)) };
    println!("{:?}", h2);
}
