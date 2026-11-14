#[derive(Debug)]
pub struct Meters(pub f64);

#[derive(Debug)]
pub struct Feet(pub f64);

impl From<Feet> for Meters {
    fn from(f: Feet) -> Self {
        Meters(f.0 * 0.3048)
    }
}

impl From<i32> for Meters {
    fn from(n: i32) -> Self {
        Meters(n as f64)
    }
}

#[derive(Debug)]
pub enum UnitError {
    Negative(f64),
}

#[derive(Debug)]
pub struct BadInput(pub String);

impl From<BadInput> for UnitError {
    fn from(b: BadInput) -> Self {
        UnitError::Negative(b.0.len() as f64 * -1.0)
    }
}
