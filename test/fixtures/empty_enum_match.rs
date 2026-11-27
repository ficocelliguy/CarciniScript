// `match x {}` on a value no type has (`Infallible`, a local `enum Void {}`, through `*r`), and
// arrays matched inside an `Option`, a `Result` or a tuple field, whose length the declared type
// or `<[T; N]>::try_from` gives.
use std::convert::Infallible;

struct Error;

impl From<Infallible> for Error {
    fn from(e: Infallible) -> Error {
        match e {}
    }
}

fn unwrap_ok(r: Result<u8, Infallible>) -> u8 {
    match r {
        Ok(v) => v,
        Err(e) => match e {},
    }
}

enum Void {}

fn never(v: &Void) -> u8 {
    match *v {}
}

fn try_all<F: FnMut() -> Result<u32, Void>>(mut f: F) -> u32 {
    match f() {
        Ok(v) => v,
        Err(void) => match void {},
    }
}

struct Pair<T>(Option<[T; 2]>);

impl<T: std::fmt::Debug> Pair<T> {
    fn describe(&self) -> String {
        match &self.0 {
            None => "none".to_string(),
            Some([a, b]) => format!("{a:?} and {b:?}"),
        }
    }
}

struct Named {
    pt: (u8, [u8; 2]),
}

fn one(items: Vec<u32>) -> u32 {
    match <[_; 1]>::try_from(items) {
        Ok([item]) => item * 100,
        Err(items) => items.len() as u32,
    }
}

fn main() {
    let _ = never;
    let _ = Error;
    println!("{}", unwrap_ok(Ok(7)));
    println!("{}", try_all(|| Ok(5)));
    println!("{}", Pair(Some([1, 2])).describe());
    println!("{}", Pair::<u8>(None).describe());
    let n = Named { pt: (3, [4, 5]) };
    match n.pt {
        (x, [y, z]) => println!("{}", x + y + z),
    }
    println!("{} {}", one(vec![9]), one(vec![1, 2, 3]));
}
