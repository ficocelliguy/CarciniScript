// `mod shapes;` loads `shapes/mod.rs` (there's no `shapes.rs`), which loads its own
// `shapes/consts.rs` next to it.
mod shapes;
mod util;
use shapes::{area_sum, Circle};

fn main() {
    let cs = vec![Circle { r: 1.0 }, Circle { r: 2.0 }];
    println!("{:.3}", area_sum(&cs));
    println!("{}", util::twice(21));
}
