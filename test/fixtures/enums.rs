// Enums: unit, tuple and struct variants, methods on enums, `match` with exhaustiveness over
// variants, nested patterns, `if let` / `while let` on variants, derives, C-like enums with
// discriminants and `as` casts, enums in collections and in `Option`.
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Circle(f64),
    Rect { w: f64, h: f64 },
    Empty,
}

impl Shape {
    fn area(&self) -> f64 {
        match self {
            Shape::Circle(r) => 3.0 * r * r,
            Shape::Rect { w, h } => w * h,
            Shape::Empty => 0.0,
        }
    }

    fn scale(&mut self, k: f64) {
        *self = match self {
            Shape::Circle(r) => Shape::Circle(*r * k),
            Shape::Rect { w, h } => Shape::Rect { w: *w * k, h: *h * k },
            Shape::Empty => Shape::Empty,
        };
    }

    fn is_round(&self) -> bool {
        matches!(self, Shape::Circle(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Level {
    Low = 1,
    Mid = 5,
    High = 10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
enum Dir {
    #[default]
    North,
    East,
    South,
    West,
}

impl Dir {
    fn turn(self) -> Dir {
        match self {
            Dir::North => Dir::East,
            Dir::East => Dir::South,
            Dir::South => Dir::West,
            Dir::West => Dir::North,
        }
    }
}

#[derive(Debug)]
enum Cmd {
    Move { dir: Dir, steps: i32 },
    Say(String),
    Wait(u32, Option<String>),
    Quit,
}

fn run(cmds: &[Cmd]) -> (i32, i32) {
    let (mut x, mut y) = (0, 0);
    for c in cmds {
        match c {
            Cmd::Move { dir: Dir::North, steps } => y += steps,
            Cmd::Move { dir: Dir::South, steps } => y -= steps,
            Cmd::Move { dir, steps } if *dir == Dir::East => x += steps,
            Cmd::Move { steps, .. } => x -= steps,
            Cmd::Say(s) => println!("say {}", s),
            Cmd::Wait(n, Some(why)) => println!("wait {} for {}", n, why),
            Cmd::Wait(n, None) => println!("wait {}", n),
            Cmd::Quit => break,
        }
    }
    (x, y)
}

fn main() {
    let mut shapes = vec![Shape::Circle(1.0), Shape::Rect { w: 2.0, h: 3.0 }, Shape::Empty];
    for s in &shapes {
        println!("{:?} {} {}", s, s.area(), s.is_round());
    }
    shapes[1].scale(2.0);
    println!("{:?} {}", shapes[1], shapes[0] == Shape::Circle(1.0));
    let total: f64 = shapes.iter().map(|s| s.area()).sum();
    println!("{}", total);
    if let Shape::Rect { w, .. } = &shapes[1] {
        println!("w {}", w);
    }

    println!("{} {} {:?}", Level::Mid as i32, Level::High as u8 + 1, Level::Low);
    println!("{} {:?}", Level::Low < Level::High, [Level::High, Level::Low, Level::Mid].iter().max());
    let mut d = Dir::default();
    for _ in 0..5 {
        d = d.turn();
    }
    println!("{:?} {} {}", d, d as i32, d == Dir::East);

    let cmds = vec![
        Cmd::Move { dir: Dir::North, steps: 3 },
        Cmd::Say("hi".to_string()),
        Cmd::Move { dir: Dir::East, steps: 2 },
        Cmd::Wait(5, None),
        Cmd::Wait(1, Some("coffee".to_string())),
        Cmd::Move { dir: Dir::West, steps: 7 },
        Cmd::Quit,
        Cmd::Say("unreachable".to_string()),
    ];
    println!("{:?}", run(&cmds));
    println!("{:?}", cmds[0]);

    let mut counts: HashMap<Dir, u32> = HashMap::new();
    for dir in [Dir::North, Dir::East, Dir::North, Dir::West] {
        *counts.entry(dir).or_insert(0) += 1;
    }
    let mut cv: Vec<_> = counts.into_iter().collect();
    cv.sort();
    println!("{:?}", cv);

    let mut stack = vec![Dir::South, Dir::North, Dir::West];
    while let Some(top) = stack.pop() {
        match top {
            Dir::West => println!("west"),
            other => println!("{:?}", other),
        }
    }
    let lookup = |d: Dir| -> Option<Level> {
        match d {
            Dir::North => Some(Level::High),
            Dir::South => Some(Level::Low),
            _ => None,
        }
    };
    println!("{:?} {:?}", lookup(Dir::North), lookup(Dir::East));
}
