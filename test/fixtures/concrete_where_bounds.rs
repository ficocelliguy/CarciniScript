// Calls through bounds: `T::clone(x)` by path, `where Self: PartialEq` on a trait's default
// method, and `where` clauses whose subject is a concrete type (`Ident: PartialEq<I>`,
// `TextSize: Add<A, Output = TextSize>`), with the operand's impl picked at run time.
use std::ops::{Add, AddAssign, Mul};

fn dup<T: Clone>(x: &T) -> (T, T) {
    (T::clone(x), T::clone(&x))
}

pub trait One: Sized + Mul<Self, Output = Self> {
    fn one() -> Self;
    fn is_one(&self) -> bool
    where
        Self: PartialEq,
    {
        *self == Self::one()
    }
}
impl One for u32 {
    fn one() -> u32 {
        1
    }
}

pub struct Ident(String);
impl PartialEq<str> for Ident {
    fn eq(&self, o: &str) -> bool {
        self.0 == o
    }
}
impl PartialEq for Ident {
    fn eq(&self, o: &Ident) -> bool {
        self.0 == o.0
    }
}
pub struct Path {
    id: Option<Ident>,
}
impl Path {
    fn is_ident<I>(&self, ident: &I) -> bool
    where
        I: ?Sized,
        Ident: PartialEq<I>,
    {
        match &self.id {
            Some(id) => id == ident,
            None => false,
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct TextSize(u32);
impl Add<TextSize> for TextSize {
    type Output = TextSize;
    fn add(self, o: TextSize) -> TextSize {
        TextSize(self.0 + o.0)
    }
}
impl Add<u32> for TextSize {
    type Output = TextSize;
    fn add(self, o: u32) -> TextSize {
        TextSize(self.0 + o * 10)
    }
}
impl<A> AddAssign<A> for TextSize
where
    TextSize: Add<A, Output = TextSize>,
{
    fn add_assign(&mut self, rhs: A) {
        *self = *self + rhs
    }
}
impl<A> std::iter::Sum<A> for TextSize
where
    TextSize: Add<A, Output = TextSize>,
{
    fn sum<I: Iterator<Item = A>>(iter: I) -> TextSize {
        iter.fold(TextSize(0), Add::add)
    }
}

fn main() {
    println!("{:?}", dup(&vec![1, 2]));
    println!("{} {}", 1u32.is_one(), 7u32.is_one());
    let p = Path { id: Some(Ident("x".to_string())) };
    let q = Path { id: None };
    println!(
        "{} {} {} {}",
        p.is_ident("x"),
        p.is_ident("y"),
        p.is_ident(&Ident("x".to_string())),
        q.is_ident("x")
    );
    let mut t = TextSize(1);
    t += 3u32;
    t += TextSize(2);
    println!("{:?}", t);
    let s: TextSize = vec![TextSize(1), TextSize(2)].into_iter().sum();
    println!("{:?}", s);
}
