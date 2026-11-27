// `IntoIterator` bounds on type parameters that stay parameters: on an impl's (bounded in a
// method's `where`, with `L::IntoIter` in the return type: either), `where &'a I: IntoIterator`
// (strsim), `H::Item: IntoIterator` and `<I::Item as IntoIterator>::Item: Into<usize>`
// (itertools); with `T::clone(b)` of a `b: &Bx<T>` through `Deref`, `*this = ..` of a `this: &mut
// Self` whose type derefs, and `Default::default()` / `unwrap_or_default()` of a `T: Default`.
use std::collections::HashSet;
use std::ops::Deref;

#[derive(Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Either<L, R> {
    fn into_iter(self) -> Either<L::IntoIter, R::IntoIter>
    where
        L: IntoIterator,
        R: IntoIterator<Item = L::Item>,
    {
        match self {
            Either::Left(inner) => Either::Left(inner.into_iter()),
            Either::Right(inner) => Either::Right(inner.into_iter()),
        }
    }

    fn iter(&self) -> Either<<&L as IntoIterator>::IntoIter, <&R as IntoIterator>::IntoIter>
    where
        for<'a> &'a L: IntoIterator,
        for<'a> &'a R: IntoIterator<Item = <&'a L as IntoIterator>::Item>,
    {
        match self {
            Either::Left(inner) => Either::Left(inner.into_iter()),
            Either::Right(inner) => Either::Right(inner.into_iter()),
        }
    }

    fn count_all(self) -> usize
    where
        L: IntoIterator,
        R: IntoIterator,
    {
        match self {
            Either::Left(inner) => inner.into_iter().count(),
            Either::Right(inner) => inner.into_iter().count(),
        }
    }
}

impl<L: Iterator, R: Iterator<Item = L::Item>> Either<L, R> {
    fn next_item(&mut self) -> Option<L::Item> {
        match self {
            Either::Left(l) => l.next(),
            Either::Right(r) => r.next(),
        }
    }
}

struct Wrap<C>(C);

impl<C: IntoIterator> Wrap<C> {
    fn items(self) -> C::IntoIter {
        self.0.into_iter()
    }
}

fn spread<I: IntoIterator>(x: I) -> I::IntoIter {
    x.into_iter()
}

fn jaro<'a, 'b, I1, I2, E1, E2>(a: &'a I1, b: &'b I2) -> usize
where
    &'a I1: IntoIterator<Item = E1>,
    &'b I2: IntoIterator<Item = E2>,
    E1: PartialEq<E2>,
{
    let mut n = a.into_iter().count();
    for (i, x) in a.into_iter().enumerate() {
        for (j, y) in b.into_iter().enumerate().take(3) {
            if x == y {
                n += i + j;
            }
        }
    }
    let mut same = 0;
    for (x, y) in a.into_iter().zip(b) {
        if x == y {
            same += 1;
        }
    }
    let mut all = 0;
    for _ in a {
        all += 1;
    }
    n * 100 + same * 10 + all
}

fn flatten_count<H>(iters: H) -> usize
where
    H: Iterator,
    H::Item: IntoIterator,
{
    let mut n = 0;
    for it in iters {
        n += it.into_iter().count();
    }
    n
}

fn firsts<I>(iterable: I) -> Vec<usize>
where
    I: IntoIterator,
    I::Item: IntoIterator,
    <I::Item as IntoIterator>::Item: Clone + Into<usize>,
{
    let mut out = Vec::new();
    for it in iterable {
        if let Some(x) = it.into_iter().next() {
            out.push(x.clone().into());
        }
    }
    out
}

struct Bx<T> {
    v: Box<T>,
}

impl<T> Deref for Bx<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.v
    }
}

impl<T: Clone> Bx<T> {
    fn make_mut(this: &mut Self) -> &mut T {
        *this = Bx { v: Box::new(T::clone(this)) };
        &mut this.v
    }

    fn reset(this: &mut Self, x: T) {
        *this = Bx { v: Box::new(x) };
    }
}

fn dup<T: Clone>(b: &Bx<T>) -> T {
    T::clone(b)
}

struct Table<V> {
    map: Option<Vec<V>>,
}

impl<V> Table<V>
where
    V: Default + Clone + Eq + Copy,
{
    fn get(&self, key: usize) -> V {
        self.map.as_ref().map_or_else(|| Default::default(), |map| map[key])
    }
    fn first(&self) -> V {
        self.map.as_ref().and_then(|m| m.first().copied()).unwrap_or_default()
    }
}

fn main() {
    let l: Either<Vec<u32>, Vec<u32>> = Either::Left(vec![1, 2, 3]);
    let mut it = l.into_iter();
    println!("{:?} {:?}", it.next_item(), it.next_item());
    let r: Either<Vec<u32>, HashSet<u32>> = Either::Right([7].into_iter().collect());
    let v: Vec<u32> = match r.into_iter() {
        Either::Left(i) => i.collect(),
        Either::Right(i) => i.collect(),
    };
    println!("{:?}", v);
    let e: Either<Vec<u8>, Vec<u8>> = Either::Right(vec![4, 5]);
    let s: u32 = match e.iter() {
        Either::Left(i) => i.map(|x| *x as u32).sum(),
        Either::Right(i) => i.map(|x| *x as u32).sum(),
    };
    println!("{} {:?}", s, e);
    println!("{}", Either::<Vec<char>, Vec<u8>>::Left(vec!['a', 'b']).count_all());
    let w: Vec<u16> = Wrap(vec![9u16, 8]).items().collect();
    println!("{:?}", w);
    let sp: Vec<i64> = spread(vec![3i64, 4]).rev().collect();
    println!("{:?}", sp);
    println!("{}", jaro(&vec![1, 2, 3], &vec![3, 2, 1]));
    println!("{}", jaro(&"abc".chars().collect::<Vec<_>>(), &vec!['c', 'a']));
    println!("{}", flatten_count(vec![vec![1u8], vec![2, 3]].into_iter()));
    println!("{:?}", firsts(vec![vec![4u8, 5], vec![], vec![6]]));
    let mut b = Bx { v: Box::new(vec![1u8, 2]) };
    Bx::make_mut(&mut b);
    b.v.push(3);
    println!("{:?} {:?}", *b, dup(&b));
    Bx::reset(&mut b, vec![9]);
    println!("{:?}", b.v);
    let t: Table<u8> = Table { map: None };
    let u: Table<u8> = Table { map: Some(vec![5, 6]) };
    println!("{} {} {} {}", t.get(0), u.get(1), t.first(), u.first());
}
