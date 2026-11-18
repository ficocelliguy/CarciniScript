// `impl From<X> for Box<dyn Trait>` (`x.into()`, `Box::from(x)`, `<Box<dyn Trait>>::from(x)`,
// several source types), and your traits on `Range<usize>`: methods with defaults (also on a
// literal range, through a generic `W: Width`, and in an array), `impl PartialEq<Span> for
// Range<usize>` (`r == span`) and `impl From<Span> for Range<usize>`.
use std::ops::Range;

trait Shape {
    fn area(&self) -> f64;
}

struct Sq(f64);

impl Shape for Sq {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}

struct Circle(f64);

impl Shape for Circle {
    fn area(&self) -> f64 {
        3.0 * self.0 * self.0
    }
}

impl From<Sq> for Box<dyn Shape> {
    fn from(s: Sq) -> Self {
        println!("boxing a square");
        Box::new(s)
    }
}

impl From<f64> for Box<dyn Shape> {
    fn from(r: f64) -> Self {
        Box::new(Circle(r))
    }
}

fn total(shapes: Vec<Box<dyn Shape>>) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    lo: usize,
    hi: usize,
}

impl PartialEq<Span> for Range<usize> {
    fn eq(&self, other: &Span) -> bool {
        self.start == other.lo && self.end == other.hi
    }
}

trait Width {
    fn width(&self) -> usize;
    fn is_wide(&self) -> bool {
        self.width() > 3
    }
}

impl Width for Range<usize> {
    fn width(&self) -> usize {
        self.end - self.start
    }
}

impl From<Span> for Range<usize> {
    fn from(s: Span) -> Self {
        s.lo..s.hi
    }
}

fn widest<W: Width>(xs: &[W]) -> usize {
    xs.iter().map(|w| w.width()).max().unwrap_or(0)
}

fn one<W: Width>(w: &W) -> usize {
    w.width()
}

fn main() {
    let b: Box<dyn Shape> = Sq(3.0).into();
    println!("{}", b.area());
    let c = <Box<dyn Shape>>::from(2.0);
    println!("{}", c.area());
    let d: Box<dyn Shape> = Box::from(Sq(1.0));
    println!("{}", d.area());
    println!("{}", total(vec![Sq(1.0).into(), 1.0.into()]));

    let r = 2..5usize;
    println!("{} {}", r == Span { lo: 2, hi: 5 }, r == Span { lo: 1, hi: 5 });
    println!("{} {} {}", r.width(), r.is_wide(), (0..10).is_wide());
    println!("{} {}", widest(&[0..2, 3..9, 4..5]), one(&(1..2)));
    let q: Range<usize> = Span { lo: 7, hi: 9 }.into();
    println!("{:?} {}", q, q.width());
    for i in r.clone() {
        print!("{i} ");
    }
    println!();
}
