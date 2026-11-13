// `const` and `static` items: scalars, strings, arrays, tuples and structs; constants built from
// other constants (declared in any order); constants in patterns; Copy constants read by value.
const HACK_FRACTION: f64 = 0.25;
const LIMIT: u32 = BASE * 4;
const BASE: u32 = 16;
const NAME: &str = "n00dles";
const PORTS: [u16; 3] = [21, 22, 80];
const ORIGIN: Point = Point { x: 0, y: 0 };
const PAIR: (i32, char) = (7, 'z');
static GREETING: &str = "hello";

#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

fn classify(n: u32) -> &'static str {
    match n {
        0 => "zero",
        BASE => "base",
        LIMIT => "limit",
        _ => "other",
    }
}

fn main() {
    println!("{} {} {} {}", HACK_FRACTION, LIMIT, NAME, GREETING);
    println!("{:?} {} {:?}", PORTS, PORTS.len(), PAIR);
    let mut p = ORIGIN;
    p.x += 5;
    println!("{:?} {:?} {}", p, ORIGIN, p == ORIGIN);
    for n in [0, 16, 64, 3] {
        println!("{} {}", n, classify(n));
    }
    let money = 1000.0 * HACK_FRACTION;
    println!("{}", money);
    let total: u32 = PORTS.iter().map(|&p| p as u32).sum();
    println!("{}", total);
}
