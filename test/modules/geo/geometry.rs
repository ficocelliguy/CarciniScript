#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

pub const ORIGIN: Point = Point { x: 0.0, y: 0.0 };

impl Point {
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    pub fn dist(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

pub enum Shape {
    Circle(f64),
    Rect { w: f64, h: f64 },
}

pub trait Area {
    fn area(&self) -> f64;

    fn label(&self) -> String {
        if self.area() > 5.0 { "big".to_string() } else { "small".to_string() }
    }
}

impl Area for Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle(r) => 3.0 * r * r,
            Shape::Rect { w, h } => w * h,
        }
    }
}

pub fn sides(s: &Shape) -> u32 {
    match s {
        Shape::Circle(_) => 0,
        Shape::Rect { .. } => 4,
    }
}

pub fn translate(mut p: Point, dx: f64, dy: f64) -> Point {
    p.x += dx;
    p.y += dy;
    p
}
