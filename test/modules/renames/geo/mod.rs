mod area;
pub mod units;

pub use self::area::Error as AreaError;
use units::Meters as M;

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
}

pub const ORIGIN: Point = Point { x: 0.0, y: 0.0 };

#[derive(Debug)]
pub enum Kind {
    Small,
    Big,
}

pub trait Shape {
    fn area(&self) -> f64;
}

pub struct Square {
    pub side: f64,
}

impl Shape for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
}

#[derive(Debug)]
pub struct Grid<T> {
    pub cells: Vec<T>,
    pub width: usize,
}

impl<T> Grid<T> {
    pub fn new(width: usize) -> Grid<T> {
        Grid { cells: Vec::new(), width }
    }
}

pub fn to_meters(s: &Square) -> M {
    M(s.area())
}

pub fn check(v: f64) -> Result<f64, AreaError> {
    if v < 0.0 { Err(AreaError::Negative(v)) } else { Ok(v) }
}
