// Modules: `mod` loads sibling files; their `pub` functions, structs, enums, traits and constants
// are used through `use` (with renaming and lists) and through `module::item` paths.
mod geometry;
mod util;

use geometry::{Area, Point, Shape, ORIGIN};
use util::{clamp_all, describe as describe_num};

struct Square {
    side: f64,
}

impl Area for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
}

fn main() {
    let p = Point::new(3.0, 4.0);
    println!("{:?} {} {}", p, p.dist(&ORIGIN), geometry::ORIGIN.x);
    let shapes: Vec<Box<dyn Area>> = vec![
        Box::new(Shape::Circle(1.0)),
        Box::new(Shape::Rect { w: 2.0, h: 5.0 }),
        Box::new(Square { side: 3.0 }),
    ];
    for s in &shapes {
        println!("{:.2} {}", s.area(), s.label());
    }
    println!("{:?}", clamp_all(&[-5, 3, 99], 0, 10));
    println!("{} {}", describe_num(7), util::describe(-2));
    println!("{}", util::MAX_THREADS + geometry::sides(&Shape::Rect { w: 1.0, h: 1.0 }));
    let moved = geometry::translate(p, 1.0, -1.0);
    println!("{:?} {:?}", moved, p);
}
