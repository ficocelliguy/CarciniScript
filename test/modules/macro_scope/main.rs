// Macros reach module files the way Rust's textual scoping says: a `#[macro_use] mod`'s macros
// and the declaring file's own macros above the `mod` line, also further down the tree, and the
// crate's `#[macro_export]` macros anywhere (`crate::clamp!`).
#[macro_use]
mod macros;

macro_rules! twice {
    ($e:expr) => {
        $e * 2
    };
}

mod deep;
mod report;

fn main() {
    report::run();
    deep::inner::go();
    println!("{}", twice!(hp_value!(4)));
}
