// Type aliases: plain, generic (`Grid<T>`), of std collections (`Counts::new()`), of an alias,
// of a fn pointer type, of a tuple, and of a struct (`Bot { .. }`); `use Dir::*` and
// `use Dir::{East as E, ..}` inside functions; `matches!` as a value inside other macros.
use std::collections::HashMap;

type Grid<T> = Vec<Vec<T>>;
type Counts = HashMap<String, usize>;
type Score = u64;
type Total = Score;
type Op = fn(i64, i64) -> i64;
type Pos = (usize, usize);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Dir {
    North,
    East,
    South,
    West,
}

#[derive(Debug)]
struct Robot {
    at: Pos,
    facing: Dir,
    score: Total,
}

type Bot = Robot;

fn step(p: Pos, d: Dir) -> Pos {
    use Dir::*;
    match d {
        North => (p.0, p.1 + 1),
        East => (p.0 + 1, p.1),
        South => (p.0, p.1.saturating_sub(1)),
        West => (p.0.saturating_sub(1), p.1),
    }
}

fn turn(d: Dir) -> Dir {
    use Dir::{East as E, North as N, South as S, West as W};
    match d {
        N => E,
        E => S,
        S => W,
        W => N,
    }
}

fn apply(op: Op, a: i64, b: i64) -> i64 {
    op(a, b)
}

fn main() {
    let mut g: Grid<char> = vec![vec!['.'; 3]; 2];
    g[1][2] = '#';
    let rows: Vec<String> = g.iter().map(|r| r.iter().collect()).collect();
    println!("{}", rows.join("/"));
    let mut c = Counts::new();
    for w in "a b a c a".split(' ') {
        *c.entry(w.to_string()).or_insert(0) += 1;
    }
    let mut keys: Vec<_> = c.iter().collect();
    keys.sort();
    println!("{:?}", keys);
    let mut r = Bot { at: (0, 0), facing: Dir::North, score: 0 };
    for _ in 0..5 {
        r.at = step(r.at, r.facing);
        r.facing = turn(r.facing);
        r.score += r.at.0 as Score;
    }
    println!("{:?}", r);
    let add: Op = |a, b| a + b;
    println!("{} {}", apply(add, 2, 3), apply(|a, b| a * b, 4, 5));
    let empty: Grid<i32> = Grid::new();
    println!("{} {}", empty.len(), matches!(turn(Dir::West), Dir::North));
}
