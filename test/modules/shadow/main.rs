// A local fn named like a `pub fn` of a `mod` file keeps its own name (and the module's is
// still reachable as `util::helper`).
mod util;

fn helper() -> i32 {
    2
}

fn main() {
    println!("{} {} {}", helper(), util::helper(), util::main());
}
