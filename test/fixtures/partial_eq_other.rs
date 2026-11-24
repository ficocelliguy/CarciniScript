// `impl PartialEq<Other>` / `PartialOrd<Other>` next to a type's own comparison, and
// `T: PartialEq<U>` bounds on type parameters and projections.
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct StatusCode(pub u16);

impl PartialEq<u16> for StatusCode {
    fn eq(&self, other: &u16) -> bool {
        self.0 == *other
    }
}

impl PartialEq<StatusCode> for u16 {
    fn eq(&self, other: &StatusCode) -> bool {
        self == &other.0
    }
}

// (`self.0.partial_cmp(other)` on the `u16` is std's, not the `PartialOrd<StatusCode>` below.)
impl PartialOrd<u16> for StatusCode {
    fn partial_cmp(&self, other: &u16) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialOrd<StatusCode> for u16 {
    fn partial_cmp(&self, other: &StatusCode) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }
}

// A derived `PartialEq` / `PartialOrd` and one with `str`: `self.eq(&Potential(..))` is the
// derived one.
#[derive(PartialEq, PartialOrd, Debug)]
pub struct Potential(Vec<u8>);

impl PartialEq<str> for Potential {
    fn eq(&self, other: &str) -> bool {
        self.eq(&Potential(other.as_bytes().to_vec()))
    }
}

impl PartialOrd<str> for Potential {
    fn partial_cmp(&self, other: &str) -> Option<Ordering> {
        self.partial_cmp(&Potential(other.as_bytes().to_vec()))
    }
}

pub fn generic_hamming<Iter1, Iter2, Elem1, Elem2>(a: Iter1, b: Iter2) -> usize
where
    Iter1: IntoIterator<Item = Elem1>,
    Iter2: IntoIterator<Item = Elem2>,
    Elem1: PartialEq<Elem2>,
{
    let (mut ita, mut itb) = (a.into_iter(), b.into_iter());
    let mut count = 0;
    loop {
        match (ita.next(), itb.next()) {
            (Some(x), Some(y)) => {
                if x != y {
                    count += 1;
                }
            }
            _ => return count,
        }
    }
}

pub fn equal<I, J>(a: I, b: J) -> bool
where
    I: IntoIterator,
    J: IntoIterator,
    I::Item: PartialEq<J::Item>,
{
    a.into_iter().eq(b)
}

pub struct Sp<T> {
    val: T,
}

impl<T: PartialEq<U>, U> PartialEq<U> for Sp<T> {
    fn eq(&self, other: &U) -> bool {
        self.val == *other
    }
}

fn below<T: PartialOrd<U>, U>(a: T, b: U) -> bool {
    a < b
}

// (At run time: `a`'s own `impl PartialEq<u16>` / `PartialOrd<u16>` when it's a `StatusCode`.)
fn is_ok<T: PartialEq<u16>>(a: T) -> bool {
    a == 200
}

fn under<T: PartialOrd<u16>>(a: T, limit: u16) -> bool {
    a < limit
}

pub trait Equivalent<K: ?Sized> {
    fn equivalent(&self, key: &K) -> bool;
}

impl<A, B, C, D> Equivalent<(C, D)> for (A, B)
where
    A: PartialEq<C>,
    B: PartialEq<D>,
{
    fn equivalent(&self, rhs: &(C, D)) -> bool {
        self.0 == rhs.0 && self.1 == rhs.1
    }
}

fn main() {
    let s = StatusCode(200);
    println!("{} {} {} {}", s == 200, 200u16 == s, s < 300, 404u16 > s);
    println!("{} {}", s != 404, 200u16 != s);
    println!("{:?} {:?}", 5u16.partial_cmp(&7u16), s.partial_cmp(&StatusCode(100)));
    println!("{} {}", s == StatusCode(200), s.eq(&StatusCode(201)));

    let p = Potential(vec![97, 98]);
    println!("{} {} {}", p == *"ab", p == *"abc", p.eq(&Potential(vec![97, 98])));
    println!("{:?} {}", p.partial_cmp(&Potential(vec![97, 99])), p < *"b");

    println!("{}", generic_hamming("karolin".chars(), "kathrin".chars()));
    println!("{} {}", equal(vec![1, 2, 3], 1..4), equal(vec![1, 2], vec![1, 5]));
    println!("{} {}", Sp { val: 3u8 } == 3u8, Sp { val: 3u8 } != 4u8);
    println!("{} {}", below(1.5, 2.0), below(String::from("b"), String::from("a")));
    println!(
        "{} {} {} {}",
        is_ok(StatusCode(200)),
        is_ok(404u16),
        under(StatusCode(200), 300),
        under(StatusCode(500), 300)
    );
    println!(
        "{} {}",
        (1u8, "a").equivalent(&(1u8, "a")),
        (1u8, "a").equivalent(&(2u8, "a"))
    );
}
