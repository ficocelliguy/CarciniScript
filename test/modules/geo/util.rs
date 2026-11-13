pub const MAX_THREADS: u32 = 64;

pub fn clamp_all(xs: &[i32], lo: i32, hi: i32) -> Vec<i32> {
    xs.iter().map(|&x| x.clamp(lo, hi)).collect()
}

pub fn describe(n: i32) -> String {
    if n < 0 { format!("negative {}", -n) } else { format!("positive {}", n) }
}
