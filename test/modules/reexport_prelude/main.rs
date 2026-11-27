// The `prelude` idiom: an inline module re-exporting a `mod` file's items (and std's), each
// brought in by name (`use prelude::Point;`), renamed, and named by path.
mod geo;
mod prelude {
    pub use crate::geo::Point;
    pub use crate::geo::{origin, Dir, Area};
    pub use crate::geo::Point as P2;
    pub use std::collections::HashMap;
}
use prelude::Point;
use prelude::{origin, Dir};
use prelude::Area;
use prelude::HashMap;

fn f(p: &prelude::P2) -> i32 { p.sum() }

fn main() {
    let p: Point = origin();
    let q = Point { x: 2, y: 3 };
    let d = Dir::S;
    let mut m: HashMap<i32, i32> = HashMap::new();
    m.insert(1, q.sum());
    println!("{} {} {} {} {}", p.x, q.sum(), q.area(), f(&q), matches!(d, Dir::S));
    println!("{:?}", m.get(&1));
}
