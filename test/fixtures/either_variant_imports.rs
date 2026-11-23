// A bundled crate's enum variants imported by `use` (`use either::Either::{self, Left, Right};`),
// written bare in expressions and patterns.
use either::Either::{self, Left, Right};

fn parse(s: &str) -> Either<i32, String> {
    match s.parse() {
        Ok(n) => Left(n),
        Err(_) => Right(s.to_uppercase()),
    }
}

fn main() {
    for s in ["12", "ab"] {
        match parse(s) {
            Left(n) => println!("number {}", n + 1),
            Right(t) => println!("text {}", t),
        }
    }
    let e: Either<i32, String> = Right("x".into());
    println!("{:?} {}", e, e.is_right());
}
