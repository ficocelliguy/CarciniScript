macro_rules! rect {
    ($w:expr, $h:expr) => { ($w, $h) };
}

macro_rules! maxed {
    ($x:expr) => { $x };
    ($x:expr, $($rest:expr),+) => { std::cmp::max($x, maxed!($($rest),+)) };
}

pub fn unused() {}
