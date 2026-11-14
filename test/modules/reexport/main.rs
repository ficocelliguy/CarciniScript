// `pub use` re-exports: `shapes/mod.rs` re-exports a struct and a renamed function from
// `shapes/circle.rs`, and everything in `shapes/units.rs` with `pub use units::*`.
mod shapes;
use shapes::{area_of, scale, scale_factor, Circle, Shape, UNIT};

fn main() {
    let c = Circle { r: 2.0 };
    println!("{:.3} {:.3} {}", area_of(&c), c.area(), UNIT);
    println!("{:?} {} {}", scale(&c, 1.5), shapes::describe(&c), scale_factor(3.0));
}
