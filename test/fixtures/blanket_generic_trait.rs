// A blanket impl of a generic trait (`impl<T, U> DoubleDrop<T> for U`): the impl's other type
// parameters are inferred at each call.
use std::fmt::Debug;

#[derive(Debug, Clone)]
struct Empty;
#[derive(Debug)]
struct Null;

trait DoubleDrop<T> {
    fn double_drop(self, _: T);
}

impl<T, U> DoubleDrop<T> for U {
    fn double_drop(self, _: T) {}
}

trait PairWith<T> {
    fn pair_with(self, other: T) -> (Self, T)
    where
        Self: Sized;
}

impl<T: Debug, U: Debug> PairWith<T> for U {
    fn pair_with(self, other: T) -> (Self, T) {
        println!("pairing {:?} with {:?}", self, other);
        (self, other)
    }
}

fn main() {
    let empty = Empty;
    let null = Null;
    empty.double_drop(null);
    5.double_drop("x");

    let (a, b) = Empty.pair_with(3);
    println!("{:?} {}", a, b + 1);
    let (c, d) = Empty.pair_with("two");
    println!("{:?} {}", c, d.len());
    let (n, s) = 7u8.pair_with(String::from("seven"));
    println!("{} {}", n * 2, s.to_uppercase());
    println!("done");
}
