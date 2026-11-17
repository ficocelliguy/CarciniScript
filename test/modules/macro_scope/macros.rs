macro_rules! hp {
    ($n:expr) => {
        format!("{}hp", $n)
    };
}

macro_rules! hp_value {
    ($n:expr) => {
        $n * 10
    };
}

// (Named like the bundled `log` crate's macro: this one is used.)
macro_rules! info {
    ($($t:tt)*) => {
        println!("[info] {}", format!($($t)*))
    };
}

#[macro_export]
macro_rules! clamp {
    ($v:expr, $lo:expr, $hi:expr) => {
        if $v < $lo {
            $lo
        } else if $v > $hi {
            $hi
        } else {
            $v
        }
    };
}
