// Destructured parameters: tuples, structs, `&(a, _)` and slice patterns, in functions and methods.
#[derive(Debug, Clone, Copy)]
struct Point {
    x: i32,
    y: i32,
}

fn manhattan((x1, y1): (i32, i32), Point { x, y }: Point) -> i32 {
    (x1 - x).abs() + (y1 - y).abs()
}

fn first(&(a, _): &(u8, u8), [b, ..]: [u8; 3]) -> u8 {
    a + b
}

struct Acc(i64);

impl Acc {
    fn add(&mut self, (k, v): (i64, i64)) {
        self.0 += k * v;
    }
}

fn main() {
    println!("{}", manhattan((1, 2), Point { x: 4, y: -2 }));
    println!("{}", first(&(3, 9), [10, 20, 30]));
    let mut a = Acc(0);
    for pair in [(1, 2), (3, 4)] {
        a.add(pair);
    }
    let total: i32 = vec![(1, 2), (3, 4)].iter().map(|&(p, q)| p * q).sum();
    println!("{} {}", a.0, total);
}
