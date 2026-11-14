use mac::{Describe, make_defaults, square};

#[derive(Debug, Describe, Clone)]
struct Point {
    pub x: i32,
    y: i32,
}

#[derive(mac::Describe)]
struct Empty {}

make_defaults!(count: u32, label: String, ratio: f64);

#[mac::traced]
fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[mac::traced(custom_label)]
fn twice(x: i32) -> i32 {
    x * 2
}

fn main() {
    let n = 7;
    println!("{}", square!(n + 1));
    println!("{}", 2 * square!(3));
    let p = Point { x: 1, y: 2 };
    println!("{:?} {}", p.clone(), Point::describe());
    println!("{}", Empty::describe());
    println!("{} {:?} {}", default_count(), default_label(), default_ratio());
    println!("{}", add(2, twice(3)));
    let _ = p.y;
}
