// `impl<T> S<Vec<T>>`: `T`'s class rides on the `Vec`'s, so integer `/`, `%`, shifts, `!`,
// wrapping and `parse::<T>()` follow `T`.
use std::fmt::Debug;
use std::ops::{Add, Div, Not, Rem, Shl};
use std::str::FromStr;

struct Bag<T> {
    items: T,
}

impl<T: Copy + Div<Output = T> + Rem<Output = T>> Bag<Vec<T>> {
    fn halves(&self, by: T) -> Vec<(T, T)> {
        self.items.iter().map(|&x| (x / by, x % by)).collect()
    }
}

impl<T: Copy + Shl<u32, Output = T> + Not<Output = T>> Bag<Vec<T>> {
    fn shifted(&self) -> Vec<T> {
        self.items.iter().map(|&x| x << 3).collect()
    }
    fn flipped(&self) -> Vec<T> {
        self.items.iter().map(|&x| !x).collect()
    }
}

impl<T: Copy + Add<Output = T>> Bag<Vec<T>> {
    fn doubled(&self) -> Vec<T> {
        self.items.iter().map(|&x| x + x).collect()
    }
}

impl<T: FromStr + Debug> Bag<Vec<T>> {
    fn push_parsed(&mut self, s: &str) -> bool {
        match s.parse::<T>() {
            Ok(v) => {
                self.items.push(v);
                true
            }
            Err(_) => false,
        }
    }
    fn show(&self) -> String {
        format!("{:?}", self.items)
    }
}

fn bag_of<T>(items: Vec<T>) -> Bag<Vec<T>> {
    Bag { items }
}

fn main() {
    println!("{:?}", bag_of(vec![9u16, 10]).halves(4));
    println!("{:?}", bag_of(vec![9.0f32]).halves(4.0));
    let ints = Bag { items: vec![7i32, -7, 9] };
    println!("{:?}", ints.halves(2));
    let floats = Bag { items: vec![7.0f64, 9.0] };
    println!("{:?}", floats.halves(2.0));
    let bytes = Bag { items: vec![200u8, 31] };
    println!("{:?} {:?} {:?}", bytes.shifted(), bytes.flipped(), bytes.doubled());
    let signed = Bag { items: vec![5i8, 0] };
    println!("{:?}", signed.flipped());

    let mut parsed: Bag<Vec<u8>> = Bag { items: Vec::new() };
    println!("{} {} {}", parsed.push_parsed("12"), parsed.push_parsed("300"), parsed.push_parsed("x"));
    let mut fl: Bag<Vec<f32>> = Bag { items: vec![] };
    fl.push_parsed("2.5");
    fl.push_parsed("3");
    println!("{} {}", parsed.show(), fl.show());
}
