#[macro_export]
macro_rules! square_all {
    ($($x:expr),*) => { vec![$($x * $x),*] };
}

macro_rules! private_double {
    ($x:expr) => { $x * 2 };
}

pub fn total(v: &[i32]) -> i32 {
    private_double!(v.iter().sum::<i32>())
}
