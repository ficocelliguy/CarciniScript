// `impl Debug` / `Display` / `PartialEq for Box<dyn Shape>`: `{:?}`, `{}`, `==` and `.eq(..)` on a
// `dyn Shape` (and in a derived `Debug` / `PartialEq`, a `Vec` or an `Option` of one) call those, not
// the boxed value's own. (rustc rejects `a == b` by value and a derived `PartialEq` over a bare
// `Box<dyn Shape>` field with such an impl: they move out of the operands.)
use std::fmt;

trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> String;
}

#[derive(Debug, Clone, PartialEq)]
struct Circle {
    r: f64,
}
impl Shape for Circle {
    fn area(&self) -> f64 {
        4.0 * self.r * self.r
    }
    fn name(&self) -> String {
        "circle".to_string()
    }
}

#[derive(Debug)]
struct Square(f64);
impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn name(&self) -> String {
        format!("square {}", self.0)
    }
}

impl fmt::Debug for Box<dyn Shape> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} of area {}>", self.name(), self.area())
    }
}

impl PartialEq for Box<dyn Shape> {
    fn eq(&self, other: &Self) -> bool {
        self.area() == other.area()
    }
}

impl fmt::Display for Box<dyn Shape> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, PartialEq)]
struct Scene {
    others: Vec<Box<dyn Shape>>,
}

#[derive(Debug)]
struct Holder {
    main: Box<dyn Shape>,
    n: u8,
}

fn main() {
    let a: Box<dyn Shape> = Box::new(Circle { r: 1.0 });
    let b: Box<dyn Shape> = Box::new(Square(3.0));
    let c: Box<dyn Shape> = Box::new(Circle { r: 1.0 });
    println!("{:?} {:?}", a, b);
    println!("{} {}", a, b);
    println!("{} {} {}", &a == &b, &a == &c, &a != &b);
    let s = Scene { others: vec![Box::new(Circle { r: 2.0 }), Box::new(Square(1.0))] };
    let t = Scene { others: vec![Box::new(Square(4.0)), Box::new(Circle { r: 0.5 })] };
    println!("{:?}", s);
    println!("{:#?}", s.others);
    println!("{}", s == t);
    let v: Vec<Box<dyn Shape>> = vec![Box::new(Circle { r: 1.0 })];
    let w: Vec<Box<dyn Shape>> = vec![Box::new(Square(2.0))];
    println!("{}", v == w);
    let maybe: Option<Box<dyn Shape>> = Some(Box::new(Square(4.0)));
    println!("{:?} {}", maybe, a.eq(&c));
    println!("{:?}", Holder { main: Box::new(Circle { r: 0.5 }), n: 1 });
}
