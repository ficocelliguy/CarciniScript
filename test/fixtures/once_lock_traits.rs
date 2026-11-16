// `OnceLock` as a field of a struct deriving `Debug`, `Clone` and `Default`.
use std::sync::OnceLock;

#[derive(Debug, Clone, Default)]
struct Env {
    cached: OnceLock<String>,
}

fn main() {
    let e = Env::default();
    println!("{:?}", e);
    e.cached.set("x".into()).unwrap();
    let f = e.clone();
    println!("{:?} {:?}", f, OnceLock::<u8>::new());
}
