// Inline modules: functions, structs, enums, traits and consts in `mod` blocks (also nested),
// reached as `geo::area`, through `use` (with `as` and `*`), `self::` and `crate::` paths;
// `super::` from inside a module (also `super::super::`), `use super::*`, same-named items in
// different modules, impls and derives on module types (`{:?}` keeps the written names), and a
// `#[cfg(test)] mod tests` that's left out.
const SCALE: f64 = 2.0;

fn banner(s: &str) -> String {
    format!("== {s} ==")
}

mod geo {
    pub const UNIT: &str = "m";

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    impl Point {
        pub fn new(x: f64, y: f64) -> Self {
            Point { x, y }
        }
        pub fn scaled(&self) -> Point {
            Point::new(self.x * super::SCALE, self.y * super::SCALE)
        }
    }

    pub fn dist(a: &Point, b: &Point) -> f64 {
        ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
    }

    pub fn describe(p: &Point) -> String {
        format!("{:?} is {:.2}{} from the origin", p, self::dist(p, &Point::new(0.0, 0.0)), UNIT)
    }

    pub mod shapes {
        use super::Point;

        #[derive(Debug)]
        pub enum Shape {
            Circle { center: Point, r: f64 },
            Square(Point, f64),
        }

        pub fn area(s: &Shape) -> f64 {
            match s {
                Shape::Circle { r, .. } => 3.0 * r * r,
                Shape::Square(_, side) => side * side,
            }
        }

        pub fn title() -> String {
            super::super::banner("shapes")
        }
    }
}

mod text {
    use super::*;

    pub fn describe(n: usize) -> String {
        format!("{} words at scale {}", n, SCALE)
    }

    pub fn shout(s: &str) -> String {
        banner(&s.to_uppercase())
    }
}

use geo::shapes::{self as sh, Shape};
use geo::{Point, dist as distance};

fn main() {
    let a = Point::new(1.0, 2.0);
    let b = geo::Point { x: 4.0, y: 6.0 };
    println!("{} {}", distance(&a, &b), geo::describe(&b));
    println!("{:?} {}", a.scaled(), a == crate::geo::Point::new(1.0, 2.0));
    let shapes = vec![
        Shape::Circle { center: a, r: 2.0 },
        sh::Shape::Square(b, 3.0),
    ];
    for s in &shapes {
        println!("{:?} area {}", s, sh::area(s));
    }
    println!("{} {}", sh::title(), self::banner("main"));
    println!("{} | {}", text::describe(3), text::shout("hi"));
    println!("{}", geo::UNIT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dist_works() {
        assert_eq!(geo::dist(&Point::new(0.0, 0.0), &Point::new(3.0, 4.0)), 5.0);
    }
}
