// An inline module re-exporting a `mod` file's items (`mod prelude { pub use crate::geo::Point; }`):
// reached through the module's name and by a glob.
mod geo;
mod prelude {
    pub use crate::geo::Point;
    pub use crate::geo::origin;
}
use prelude::*;

fn main() {
    let p: Point = origin();
    let q = prelude::Point { x: 2, y: 3 };
    println!("{} {} {}", p.x, q.x, q.y);
}
