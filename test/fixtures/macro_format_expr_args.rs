// A format string that reaches `println!` / `format!` / `write!` through a macro's `$x:expr`
// fragment (num_cpus' `debug!` forwarding `$($args:expr),*`).
use std::fmt::Write;
macro_rules! debug {
    ($($args:expr),*) => ({ if true { println!($($args),*); } });
}
macro_rules! some {
    ($e:expr) => {{ match $e { Some(v) => v, None => { debug!("NONE: {:?}", stringify!($e)); return None; } } }};
}
macro_rules! fmt_one {
    ($f:expr, $v:expr) => { format!($f, $v) };
}
macro_rules! put {
    ($s:expr, $f:expr) => { write!($s, $f).unwrap() };
}
fn inc(x: Option<i32>) -> Option<i32> {
    let v = some!(x);
    debug!("v = {}", v);
    debug!("plain");
    Some(v + 1)
}
fn main() {
    println!("{:?} {:?}", inc(Some(1)), inc(None));
    let mut s = String::new();
    put!(s, "abc");
    println!("{} {}", fmt_one!("<{:>4}>", 7), s);
}
