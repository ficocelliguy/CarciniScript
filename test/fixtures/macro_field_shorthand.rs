// A `macro_rules!` body's own locals in a struct literal's shorthand fields (`$name { ids }`):
// the field keeps its name, the value is the expansion's local.
#[derive(Debug)]
struct Ids {
    ids: Vec<u32>,
    n: u8,
}

#[derive(Debug)]
struct P {
    x: i32,
    y: i32,
}

const MAX: i32 = 2;

macro_rules! ids {
    ($name:ident) => {{
        let ids = vec![1, 2];
        let n = 3;
        $name { ids, n: n }
    }};
}

macro_rules! points {
    ($($f:ident),*) => {{
        let x = 1;
        let y = 2;
        let a = P { x: x, y };
        // (A block after a comparison isn't a struct literal.)
        let b = if y == MAX { x } else { y };
        // (The caller's `x` and `y`.)
        let c = P { $($f),* };
        (a, b, c)
    }};
}

fn main() {
    let x = 10;
    let y = 20;
    println!("{:?}", ids!(Ids));
    println!("{:?}", points!(x, y));
}
