// Renamed imports of other files' types, traits, enums and constants: `use geo::Point as P`,
// `use crate::geo::Shape as Sh` (a trait), `use geo::{Kind as K, ORIGIN as O}`, `use
// self::geo::Grid as G` (generic), in `geo/mod.rs` `use units::Meters as M`, and a type that
// `geo/mod.rs` re-exports under a new name (`pub use self::area::Error as AreaError`), imported
// here by that name.
mod geo;

use crate::geo::Shape as Sh;
use geo::units::Meters;
use geo::{AreaError, Kind as K, ORIGIN as O, Point as P};
use self::geo::Grid as G;

struct Dot(P);

impl Sh for Dot {
    fn area(&self) -> f64 {
        0.0
    }
}

fn kind_of(s: &dyn Sh) -> K {
    if s.area() > 1.0 { K::Big } else { K::Small }
}

fn describe(e: &AreaError) -> String {
    match e {
        AreaError::Negative(v) => format!("negative: {v}"),
    }
}

fn main() {
    let p = P::new(1.0, 2.0);
    let q = P { x: 3.0, y: 4.0 };
    let P { x, y } = q;
    println!("{:?} {} {} {}", p, x, y, O.x);
    let sq = geo::Square { side: 2.0 };
    println!("{:?} {:?}", kind_of(&sq), kind_of(&Dot(p)));
    let mut g: G<i32> = G::new(2);
    g.cells.push(5);
    println!("{:?} {}", g, geo::Point::new(0.5, 0.5).x);
    println!("{}", geo::to_meters(&sq).0 + Meters(1.0).0);
    match geo::check(-1.0) {
        Ok(v) => println!("ok {v}"),
        Err(e) => println!("err {:?} / {}", e, describe(&e)),
    }
    let e: AreaError = AreaError::Negative(2.5);
    println!("{}", describe(&e));
}
