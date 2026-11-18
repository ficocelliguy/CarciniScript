#[derive(Debug)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

pub mod shapes {
    pub struct Circle {
        pub r: f64,
    }

    pub fn area(c: &Circle) -> f64 {
        3.14159 * c.r * c.r
    }

    pub fn unit() -> Circle {
        Circle { r: 1.0 }
    }
}
