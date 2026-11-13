// User traits: required and default methods, supertraits, `impl Trait for` structs and enums,
// trait-bounded generics (`T: Trait`, `impl Trait`), `dyn Trait` behind `&` and `Box`, `Vec<Box<dyn
// Trait>>`, and default methods calling required ones.
use std::fmt;

trait Named {
    fn name(&self) -> String;
}

trait Shape: Named {
    fn area(&self) -> f64;
    fn scale(&mut self, k: f64);

    fn describe(&self) -> String {
        format!("{} with area {:.2}", self.name(), self.area())
    }

    fn is_big(&self) -> bool {
        self.area() > 10.0
    }
}

#[derive(Debug, Clone)]
struct Circle {
    r: f64,
}

#[derive(Debug, Clone)]
struct Rect {
    w: f64,
    h: f64,
}

enum Blob {
    Dot,
    Splat(f64),
}

impl Named for Circle {
    fn name(&self) -> String {
        "circle".to_string()
    }
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        3.0 * self.r * self.r
    }
    fn scale(&mut self, k: f64) {
        self.r *= k;
    }
}

impl Named for Rect {
    fn name(&self) -> String {
        format!("{}x{} rect", self.w, self.h)
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.w * self.h
    }
    fn scale(&mut self, k: f64) {
        self.w *= k;
        self.h *= k;
    }
    fn is_big(&self) -> bool {
        self.w > 5.0 || self.h > 5.0
    }
}

impl Named for Blob {
    fn name(&self) -> String {
        match self {
            Blob::Dot => "dot".to_string(),
            Blob::Splat(_) => "splat".to_string(),
        }
    }
}

impl Shape for Blob {
    fn area(&self) -> f64 {
        match self {
            Blob::Dot => 0.0,
            Blob::Splat(a) => *a,
        }
    }
    fn scale(&mut self, k: f64) {
        if let Blob::Splat(a) = self {
            *a *= k;
        }
    }
}

impl fmt::Display for Circle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(r={})", self.r)
    }
}

fn total_area(shapes: &[Box<dyn Shape>]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

fn biggest<T: Shape>(items: &[T]) -> String {
    let mut best = &items[0];
    for s in items {
        if s.area() > best.area() {
            best = s;
        }
    }
    best.describe()
}

fn shout(n: &impl Named) -> String {
    n.name().to_uppercase()
}

fn grow(s: &mut dyn Shape) {
    s.scale(2.0);
}

fn main() {
    let mut shapes: Vec<Box<dyn Shape>> = vec![
        Box::new(Circle { r: 1.0 }),
        Box::new(Rect { w: 2.0, h: 3.0 }),
        Box::new(Blob::Splat(4.5)),
        Box::new(Blob::Dot),
    ];
    for s in &shapes {
        println!("{} / big: {}", s.describe(), s.is_big());
    }
    println!("{:.1}", total_area(&shapes));
    for s in shapes.iter_mut() {
        s.scale(2.0);
    }
    println!("{:.1}", total_area(&shapes));
    let circles = vec![Circle { r: 1.0 }, Circle { r: 3.0 }, Circle { r: 2.0 }];
    println!("{}", biggest(&circles));
    println!("{} {}", shout(&Rect { w: 1.0, h: 1.0 }), shout(&circles[0]));
    let mut c = Circle { r: 0.5 };
    grow(&mut c);
    println!("{} {:?}", c, c);
    let b: Box<dyn Named> = Box::new(Blob::Dot);
    println!("{}", b.name());
}
