// Module files in folders, the way rustc lays them out: `geo.rs` declares `pub mod shapes;`
// (`geo/shapes.rs`), which declares `pub mod units;` (`geo/shapes/units.rs`). Items are reached
// through the whole path (`geo::shapes::area`), by `use` through nested modules, and by
// `use geo::shapes;` then `shapes::item`.
mod geo;

use geo::shapes;
use geo::shapes::units::{Unit, label};
use geo::shapes::Square;

fn main() {
    let s = geo::shapes::Square { side: 2.0 };
    println!("{} {}", geo::shapes::area(&s), geo::scale(Square::unit()));
    println!("{} {}", shapes::area(&Square::unit()), label(Unit::Meters));
    println!("{}", geo::shapes::units::convert(3.0, Unit::Feet));
}
