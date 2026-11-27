// Methods of `impl dyn Tr` blocks: on boxes, references and through iterators, calling the
// trait's methods and each other.
use std::any::Any;
use std::fmt::Debug;

trait Shape: Debug {
    fn area(&self) -> f64;
}

#[derive(Debug)]
struct Sq(f64);

impl Shape for Sq {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}

impl dyn Shape {
    fn doubled(&self) -> f64 {
        self.area() * 2.0
    }
    fn describe(&self) -> String {
        format!("{:?} {}", self, self.area())
    }
}

impl dyn Shape + '_ {
    fn tripled(&self) -> f64 {
        self.area() * 3.0
    }
}

trait Pass {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
}

struct A {
    n: i32,
}

impl Pass for A {
    fn name(&self) -> &str {
        "a"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl dyn Pass + '_ {
    fn shout(&self) -> String {
        self.name().to_uppercase()
    }
    fn shout_twice(&self) -> String {
        format!("{}{}", self.shout(), self.shout())
    }
}

fn run(p: &dyn Pass) {
    println!("{} {}", p.shout_twice(), p.as_any().is::<A>());
    if let Some(a) = p.as_any().downcast_ref::<A>() {
        println!("{}", a.n);
    }
}

fn main() {
    let b: Box<dyn Shape> = Box::new(Sq(2.0));
    println!("{}", b.doubled());
    println!("{}", b.describe());
    println!("{}", b.tripled());
    let r: &dyn Shape = &Sq(3.0);
    println!("{}", r.doubled());
    let v: Vec<Box<dyn Shape>> = vec![Box::new(Sq(1.0)), Box::new(Sq(4.0))];
    let t: f64 = v.iter().map(|s| s.doubled()).sum();
    println!("{}", t);

    let ps: Vec<Box<dyn Pass>> = vec![Box::new(A { n: 3 })];
    for p in &ps {
        run(p.as_ref());
    }
    run(&*ps[0]);
}
