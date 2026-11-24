// Types of the program's own files named like std's (or a bundled crate's) types: `IoRead`,
// `Instant`, `Duration`, `Value` are the files' types, not a prelude's.
mod read;
mod inst;
pub use crate::inst::{Duration, Instant};
pub use read::IoRead;
use read::Value;
fn make(n: i32) -> IoRead { IoRead::new(n) }
fn main() {
    println!("{}", make(3).n);
    println!("{}", read::IoRead::new(4).n);
    println!("{}", Instant(5).0 + Duration::from_secs(6).secs);
    println!("{:?}", Value::Num(7));
}
