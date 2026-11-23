// A `where` clause on an inherent method bounding the impl's type parameter.
use std::fmt::{Debug, Display};

trait Speak {
    fn speak(&self) -> String;
}

impl Speak for u8 {
    fn speak(&self) -> String {
        format!("u8 {}", self)
    }
}

struct Value<T>(T);

impl<T> Value<T> {
    fn get(&self) -> &T {
        &self.0
    }

    fn log(&self)
    where
        T: Display,
    {
        println!("value {}", self.0)
    }

    fn speak(&self) -> String
    where
        T: Speak + Debug,
    {
        format!("{} ({:?})", self.0.speak(), self.0)
    }
}

fn main() {
    Value("hi").log();
    Value(2.5).log();
    println!("{}", Value(3u8).speak());
    println!("{}", Value(vec![1]).get().len());
}
