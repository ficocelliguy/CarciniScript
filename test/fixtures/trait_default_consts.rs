// Associated consts with defaults in traits: impls that keep and override them, `Self::N` in
// default methods and other defaults, `T::N` through a bound, and required consts next to them.
trait Shape {
    const SIDES: usize = 4;
    const NAME: &'static str;
    const LABEL: &'static str = "shape";
    const SCALE: f64 = 1.0;
    const DOUBLE: usize = Self::SIDES * 2;

    fn describe(&self) -> String {
        format!("{} {} has {} sides (x{})", Self::LABEL, Self::NAME, Self::SIDES, Self::SCALE)
    }

    fn twice() -> usize {
        Self::DOUBLE
    }
}

struct Square;
struct Triangle;
struct Hexagon(u32);

impl Shape for Square {
    const NAME: &'static str = "square";
}

impl Shape for Triangle {
    const SIDES: usize = 3;
    const NAME: &'static str = "triangle";
    const SCALE: f64 = 0.5;
}

impl Shape for Hexagon {
    const SIDES: usize = 6;
    const NAME: &'static str = "hexagon";
    const LABEL: &'static str = "polygon";
    const DOUBLE: usize = 100;
}

fn sides<T: Shape>(_: &T) -> usize {
    T::SIDES + T::DOUBLE
}

fn main() {
    println!("{} {} {}", Square::SIDES, Triangle::SIDES, Hexagon::SIDES);
    println!("{} {} {}", Square::DOUBLE, Triangle::DOUBLE, Hexagon::DOUBLE);
    println!("{}", Square.describe());
    println!("{}", Triangle.describe());
    println!("{} {}", Hexagon(1).describe(), Hexagon(2).0);
    println!("{} {} {}", Square::twice(), Triangle::twice(), Hexagon::twice());
    println!("{} {} {}", sides(&Square), sides(&Triangle), sides(&Hexagon(3)));
    println!("{} {}", Square::LABEL, Square::SCALE * 2.0);
}
