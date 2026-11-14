use mac::{FieldCount, reversed, tokens};
use std::collections::HashMap;

#[derive(FieldCount)]
struct Job {
    host: String,
    threads: u32,
    deps: HashMap<String, Vec<u8>>,
}

fn main() {
    println!("{}", tokens!(x::y<'a> + 1.5e3 => 'c' "s\"t" [0..10] { a -> b } b'x' r#"raw"# += -2));
    println!("{}", tokens!(fn f(&mut self) -> Option<i32> { self.0 }));
    let t = reversed!(1, "two", 3.0);
    println!("{:?}", t);
    println!("{}", Job::field_count());
    let j = Job { host: "a".into(), threads: 2, deps: HashMap::new() };
    println!("{} {} {}", j.host, j.threads, j.deps.len());
}
