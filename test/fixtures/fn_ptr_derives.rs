// Function pointer fields (`fn(u8) -> u8`, `Option<fn(..)>`) in a `#[derive(Clone, Copy, Debug)]`
// struct.
#[derive(Clone, Copy, Debug)]
struct Ops {
    f: fn(u8) -> u8,
    g: Option<fn(u8, u8) -> u8>,
}

#[derive(Clone, Copy)]
enum Step {
    Map(fn(i32) -> i32),
    Stop,
}

fn dbl(x: u8) -> u8 {
    x * 2
}

fn add(a: u8, b: u8) -> u8 {
    a + b
}

fn run(steps: &[Step], mut x: i32) -> i32 {
    for s in steps {
        match s {
            Step::Map(f) => x = f(x),
            Step::Stop => break,
        }
    }
    x
}

fn main() {
    let o = Ops { f: dbl, g: Some(add) };
    let p = o;
    let q = o.clone();
    println!("{} {} {}", (p.f)(3), (q.g.unwrap())(1, 2), (o.f)(4));
    let steps = [Step::Map(|x| x + 1), Step::Map(|x| x * 10), Step::Stop, Step::Map(|x| x - 1)];
    let again = steps;
    println!("{} {}", run(&steps, 1), run(&again[1..], 2));
    // (`{:?}` of a function pointer is an address: only that it formats.)
    println!("{}", format!("{:?}", o).starts_with("Ops { f: 0x"));
}
