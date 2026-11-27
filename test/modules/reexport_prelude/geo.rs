pub struct Point {
    pub x: i32,
    pub y: i32,
}
impl Point { pub fn sum(&self) -> i32 { self.x + self.y } }
pub fn origin() -> Point {
    Point { x: 0, y: 0 }
}
pub enum Dir { N, S }
pub trait Area { fn area(&self) -> i32; }
impl Area for Point { fn area(&self) -> i32 { self.x * self.y } }
