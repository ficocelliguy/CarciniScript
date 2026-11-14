mod circle;
mod units;
pub use circle::{circle_area as area_of, scale, Circle, Shape};
pub use units::*;

pub fn describe(c: &Circle) -> String {
    format!("circle of radius {}{}", c.r, UNIT)
}
