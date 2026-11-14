pub mod units;

pub struct Square {
    pub side: f64,
}

impl Square {
    pub fn unit() -> Square {
        Square { side: 1.0 }
    }
}

pub fn area(s: &Square) -> f64 {
    s.side * s.side
}
