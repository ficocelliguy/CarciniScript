// `use Enum::*` and `use Enum::{A as Alpha}` at the top of the file, in expressions, patterns
// and macro arguments; `matches!` as a value (in `let`, `println!`, `assert!`, `format!`, with a
// guard), also from `main`.
#[derive(Debug, PartialEq)]
enum K {
    A,
    B(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum D {
    N,
    E,
}

use K::*;
use K::{A as Alpha};

fn more() {
    let d = D::E;
    let b = matches!(d, D::N);
    println!("{} {}", b, matches!(d, D::E));
    println!("{}", vec![1, 2].len() + if matches!(d, D::N | D::E) { 1 } else { 0 });
    assert!(matches!(d, D::E));
    let s = format!("{:?}", matches!(Some(3), Some(x) if x > 2));
    println!("{s}");
}

fn main() {
    let v = vec![A, B(2), Alpha];
    for k in &v {
        match k {
            A => println!("a"),
            B(n) => println!("b{n}"),
        }
    }
    println!("{:?} {}", v, v[0] == Alpha);
    more();
}
