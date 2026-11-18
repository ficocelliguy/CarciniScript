// Nested module paths in `use`: into a `mod` file's inline `pub mod` (`geo::shapes`), through a
// folder module's `pub mod` file (`pointer::invariant`), inside braces, and re-exported.
mod geo;
mod pointer;
mod stamp;

use geo::shapes::Circle;
use geo::{shapes::area, Point};
use pointer::{invariant::Aligned, Ptr};
use stamp::{context::NoContext, Timestamp};

// (A `use` in a block: its names there.)
fn half_unit() -> f64 {
    use geo::shapes::{area as a, Circle as Round};
    a(&Round { r: 0.5 })
}

fn main() {
    println!("{:.4}", half_unit());
    let c = Circle { r: 2.0 };
    println!("{:.2}", area(&c));
    let p = Point { x: 1, y: 2 };
    println!("{:?} {}", p, geo::shapes::unit().r);
    let q = Ptr::new(Aligned(8));
    println!("{}", q.get());
    let t = Timestamp::now(&NoContext);
    println!("{:?}", t);
    println!("{}", stamp::context::describe());
}
