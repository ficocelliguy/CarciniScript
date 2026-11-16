// Associated types and generic trait parameters through type parameters: `S::Unit` is its own
// type inside `fn f<S: Shape>` (with the bounds `type Unit: ..` declares), `S: Shape<Unit = f64>`
// and `C: Convert<String>` fix them, and a generic call's `S::Unit` is the impl's type.
use std::fmt::Debug;
use std::ops::Add;

trait Shape {
    type Unit: Debug + Copy + Add<Output = Self::Unit>;
    fn area(&self) -> Self::Unit;
    fn name(&self) -> String;
}

struct Square(f64);

struct Grid {
    w: u32,
    h: u32,
}

impl Shape for Square {
    type Unit = f64;
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn name(&self) -> String {
        "square".into()
    }
}

impl Shape for Grid {
    type Unit = u32;
    fn area(&self) -> u32 {
        self.w * self.h
    }
    fn name(&self) -> String {
        "grid".into()
    }
}

fn area_of<S: Shape>(s: &S) -> S::Unit {
    s.area()
}

fn doubled<S: Shape>(s: &S) -> S::Unit {
    let a = s.area();
    a + a
}

fn pair<S: Shape>(s: &S) -> (String, S::Unit) {
    (s.name(), s.area())
}

fn float_area<S: Shape<Unit = f64>>(s: &S) -> f64 {
    s.area() * 10.0
}

fn show<S>(s: &S) -> String
where
    S: Shape,
{
    format!("{} {:?}", s.name(), s.area())
}

fn relay<S: Shape>(s: &S) -> String {
    format!("<{}> {:?}", show(s), pair(s))
}

trait Convert<T> {
    fn convert(&self) -> T;
}

struct Celsius(f64);

impl Convert<String> for Celsius {
    fn convert(&self) -> String {
        format!("{}C", self.0)
    }
}

impl Convert<f64> for Square {
    fn convert(&self) -> f64 {
        self.0 * 2.0
    }
}

fn label<C: Convert<String>>(c: &C) -> String {
    let s = c.convert();
    s.to_uppercase()
}

fn halve<C: Convert<f64>>(c: &C) -> f64 {
    c.convert() / 2.0
}

fn main() {
    let s = Square(1.5);
    let g = Grid { w: 3, h: 4 };
    let a: f64 = area_of(&s);
    let b: u32 = area_of(&g) + 1;
    println!("{} {}", a.sqrt(), b);
    println!("{} {}", doubled(&s), doubled(&g) * 2);
    println!("{:?} {:?}", pair(&s), pair(&g));
    println!("{} {} {}", float_area(&s), show(&s), show(&g));
    println!("{} {}", label(&Celsius(3.0)), halve(&s));
    println!("{}", area_of(&Square(2.0)).powi(2));
    println!("{} {}", show(&Square(2.0)), relay(&Square(3.0)));
}
