#[derive(Debug, Clone, Copy)]
pub enum Unit {
    Meters,
    Feet,
}

pub fn label(u: Unit) -> String {
    format!("{:?}", u).to_lowercase()
}

pub fn convert(x: f64, u: Unit) -> f64 {
    match u {
        Unit::Meters => x,
        Unit::Feet => x * 0.3048,
    }
}
