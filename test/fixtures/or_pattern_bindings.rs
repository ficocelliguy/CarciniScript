// Or-patterns that bind variables: each alternative binds the same names.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Circle(f64),
    Square(f64),
    Rect(f64, f64),
    Dot,
}

enum Cmd {
    Move { dx: i32, dy: i32 },
    Jump { dy: i32, dx: i32 },
    Stop,
}

fn size(s: Shape) -> f64 {
    match s {
        Shape::Circle(r) | Shape::Square(r) => r,
        Shape::Rect(w, _) | Shape::Rect(_, w) if w > 100.0 => w,
        Shape::Rect(w, h) => w * h,
        Shape::Dot => 0.0,
    }
}

fn first_some(pair: (Option<i32>, Option<i32>)) -> Option<i32> {
    match pair {
        (Some(x), _) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

fn classify(v: &[i32]) -> String {
    match v {
        [x] | [x, _] | [_, _, x] => format!("small, picked {}", x),
        [first, .., last] => format!("{}..{}", first, last),
        [] => "empty".into(),
    }
}

fn delta(c: &Cmd) -> (i32, i32) {
    match c {
        Cmd::Move { dx, dy } | Cmd::Jump { dx, dy } => (*dx, *dy),
        Cmd::Stop => (0, 0),
    }
}

fn main() {
    for s in [
        Shape::Circle(1.5),
        Shape::Square(2.0),
        Shape::Rect(3.0, 4.0),
        Shape::Rect(2.0, 150.0),
        Shape::Dot,
    ] {
        println!("{:?} -> {}", s, size(s));
    }
    println!(
        "{:?} {:?} {:?}",
        first_some((Some(1), Some(2))),
        first_some((None, Some(5))),
        first_some((None, None))
    );
    for v in [vec![7], vec![1, 2], vec![1, 2, 3], vec![1, 2, 3, 4], vec![]] {
        println!("{}", classify(&v));
    }
    println!(
        "{:?} {:?} {:?}",
        delta(&Cmd::Move { dx: 1, dy: 2 }),
        delta(&Cmd::Jump { dy: 3, dx: 4 }),
        delta(&Cmd::Stop)
    );
    let words = ["a1", "b", "c3"];
    for w in words {
        if let (Some(d), _) | (None, Some(d)) = (w.chars().nth(1), w.chars().next()) {
            println!("{}", d);
        }
    }
    let (Ok(n) | Err(n)) = "42".parse::<i32>().map_err(|_| -1);
    println!("{}", n);
    let mut total = 0;
    let mut stack = vec![(1, 0), (0, 2), (3, 0), (0, 0)];
    while let Some((x, 0) | (0, x)) = stack.pop() {
        total += x;
    }
    println!("{} {:?}", total, stack);
}
