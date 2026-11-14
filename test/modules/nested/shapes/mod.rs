mod consts;
use consts::PI;

#[derive(Debug, Clone)]
pub struct Circle {
    pub r: f64,
}

impl Circle {
    pub fn area(&self) -> f64 {
        PI * self.r * self.r
    }
}

pub fn area_sum(cs: &[Circle]) -> f64 {
    cs.iter().map(|c| c.area()).sum()
}
