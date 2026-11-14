pub trait Shape {
    fn area(&self) -> f64;
}

#[derive(Debug, Clone)]
pub struct Circle {
    pub r: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        3.14159 * self.r * self.r
    }
}

pub fn circle_area(c: &Circle) -> f64 {
    c.area()
}

pub fn scale(c: &Circle, k: f64) -> Circle {
    Circle { r: c.r * k }
}
