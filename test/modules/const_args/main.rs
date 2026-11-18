// Const arguments computed from another file's constants: `sizes::WORDS * 3`, an associated
// const (`Limits::ROWS`), a `u64` const parameter given another file's `u64` constant.
mod sizes;

use sizes::{Buf, Limits};

struct Tag<const V: u64>;

impl<const V: u64> Tag<V> {
    fn value(&self) -> u64 {
        V + 1
    }
}

fn pick<const B: bool>() -> &'static str {
    if B { "signed" } else { "unsigned" }
}

fn main() {
    let a: Buf<{ sizes::WORDS * 3 }> = Buf::new();
    println!("{} {}", a.data.len(), a.total());
    let b = Buf::<{ Limits::ROWS + 1 }>::new();
    println!("{}", b.data.len());
    println!("{}", Tag::<{ sizes::BIG }>.value());
    println!("{}", pick::<{ Limits::SIGNED }>());
    println!("{}", pick::<{ !Limits::SIGNED }>());
}
