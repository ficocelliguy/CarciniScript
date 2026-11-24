// A struct's field typed through its parameter's associated type (smallvec's `Array` trait), and
// `Self::Error::custom(..)` in a trait's default methods (serde's `de::Error`).
use std::fmt::{self, Display};

pub unsafe trait Array {
    type Item;
    fn size() -> usize;
}

unsafe impl<T> Array for [T; 4] {
    type Item = T;
    fn size() -> usize {
        4
    }
}

pub struct SmallVec<A: Array> {
    v: Vec<A::Item>,
}

pub struct IntoIter<A: Array> {
    v: Vec<A::Item>,
}

impl<A: Array> IntoIter<A> {
    pub fn as_slice(&self) -> &[A::Item] {
        &self.v
    }
}

impl<A: Array> Iterator for IntoIter<A> {
    type Item = A::Item;
    fn next(&mut self) -> Option<A::Item> {
        if self.v.is_empty() {
            None
        } else {
            Some(self.v.remove(0))
        }
    }
}

impl<A: Array> IntoIterator for SmallVec<A> {
    type Item = A::Item;
    type IntoIter = IntoIter<A>;
    fn into_iter(self) -> IntoIter<A> {
        IntoIter { v: self.v }
    }
}

impl<A: Array> std::ops::Deref for SmallVec<A> {
    type Target = [A::Item];
    fn deref(&self) -> &[A::Item] {
        &self.v
    }
}

impl<'a, A: Array> From<&'a [A::Item]> for SmallVec<A>
where
    A::Item: Clone,
{
    fn from(s: &'a [A::Item]) -> SmallVec<A> {
        SmallVec { v: s.to_vec() }
    }
}

impl<A: Array + Clone> Clone for IntoIter<A>
where
    A::Item: Clone,
{
    fn clone(&self) -> IntoIter<A> {
        SmallVec::from(self.as_slice()).into_iter()
    }
}

impl<A: Array> SmallVec<A> {
    pub fn new() -> SmallVec<A> {
        SmallVec { v: Vec::new() }
    }
    pub fn push(&mut self, x: A::Item) {
        self.v.push(x);
    }
    pub fn inline_size(&self) -> usize {
        A::size()
    }
}

pub enum Slot<A: Array> {
    Empty,
    Full(A::Item),
}

pub trait Error: Sized {
    fn custom<T: Display>(msg: T) -> Self;
    fn missing_field(field: &'static str) -> Self {
        Error::custom(format_args!("missing field `{}`", field))
    }
}

pub trait Visitor: Sized {
    type Value;
    type Error: Error;
    fn visit_u64(self, v: u64) -> Result<Self::Value, Self::Error> {
        Err(Self::Error::custom(format!("unexpected u64 {}", v)))
    }
    fn visit_str(self, v: &str) -> Result<Self::Value, Self::Error> {
        let _ = v;
        Err(Error::custom("unexpected str"))
    }
    fn visit_bool(self, v: bool) -> Result<Self::Value, Self::Error> {
        if v {
            Err(Self::Error::missing_field("flag"))
        } else {
            Err(<Self::Error as Error>::custom(v))
        }
    }
}

#[derive(Debug)]
pub struct MyError(String);

impl Error for MyError {
    fn custom<T: Display>(msg: T) -> Self {
        MyError(msg.to_string())
    }
}

impl Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}", self.0)
    }
}

#[derive(Debug)]
pub struct Loud(String);

impl Error for Loud {
    fn custom<T: Display>(msg: T) -> Self {
        Loud(msg.to_string().to_uppercase())
    }
}

pub struct Count;

impl Visitor for Count {
    type Value = u64;
    type Error = MyError;
    fn visit_u64(self, v: u64) -> Result<u64, MyError> {
        if v > 10 {
            Err(MyError::custom("too big"))
        } else {
            Ok(v)
        }
    }
}

pub struct Names;

impl Visitor for Names {
    type Value = String;
    type Error = Loud;
}

fn main() {
    let mut s: SmallVec<[i32; 4]> = SmallVec::new();
    s.push(3);
    s.push(5);
    println!("{} {} {:?}", s.len(), s.inline_size(), &s[..]);
    let total: i32 = s.iter().sum();
    let t = SmallVec::<[String; 4]>::from(&["a".to_string(), "b".to_string()][..]);
    let it = t.into_iter();
    let it2 = it.clone();
    let words: Vec<String> = it.collect();
    println!("{} {:?} {}", total, words, it2.count());
    let slot: Slot<[u8; 4]> = Slot::Full(7);
    if let Slot::Full(x) = slot {
        println!("{}", x + 1);
    }
    let e: Slot<[u8; 4]> = Slot::Empty;
    println!("{}", matches!(e, Slot::Empty));

    println!("{:?}", Count.visit_u64(3));
    println!("{:?}", Count.visit_u64(30));
    println!("{:?}", Count.visit_str("x"));
    println!("{:?}", Count.visit_bool(true));
    println!("{:?}", Names.visit_u64(7));
    println!("{:?}", Names.visit_bool(true));
    println!("{:?}", Names.visit_bool(false));
    match Count.visit_bool(false) {
        Err(e) => println!("{}", e),
        Ok(v) => println!("ok {}", v),
    }
}
