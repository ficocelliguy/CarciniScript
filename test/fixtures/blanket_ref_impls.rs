// Blanket impls on references and boxes: `impl<T: Tr + ?Sized> Tr for &T` (and `&mut T`,
// `Box<T>`, `Box<dyn Tr>`) whose methods hand each call on to `T`'s impl are left out, as `&T` is
// `T` in JS. `impl<T: Display> Tr for &T` and `impl<I: Iterator> Tr for &mut I` are blanket impls
// over `T`.
use std::fmt::Display;
trait Shape {
    fn area(&self) -> f64;
    fn scaled(&self, k: f64) -> f64;
    fn name(&self) -> String {
        format!("shape({})", self.area())
    }
}
struct Sq(f64);
impl Shape for Sq {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn scaled(&self, k: f64) -> f64 {
        self.area() * k
    }
}
struct Circle {
    r: f64,
}
impl Shape for Circle {
    fn area(&self) -> f64 {
        3.0 * self.r * self.r
    }
    fn scaled(&self, k: f64) -> f64 {
        self.area() * k * 2.0
    }
}
impl<T: Shape + ?Sized> Shape for &T {
    fn area(&self) -> f64 {
        (**self).area()
    }
    fn scaled(&self, k: f64) -> f64 {
        T::scaled(self, k)
    }
}
impl<'a, T> Shape for &'a mut T
where
    T: Shape,
{
    fn area(&self) -> f64 {
        Shape::area(&**self)
    }
    fn scaled(&self, k: f64) -> f64 {
        (**self).scaled(k)
    }
}
impl<T: Shape + ?Sized> Shape for Box<T> {
    fn area(&self) -> f64 {
        self.as_ref().area()
    }
    fn scaled(&self, k: f64) -> f64 {
        (**self).scaled(k)
    }
}
fn total<S: Shape>(s: S) -> f64 {
    s.area() + s.scaled(10.0)
}
fn named<S: Shape + ?Sized>(s: &S) -> String {
    s.name()
}

trait Loud {
    fn loud(&self) -> String;
}
impl<T: Display + ?Sized> Loud for &T {
    fn loud(&self) -> String {
        format!("{}!", self).to_uppercase()
    }
}

trait Summed {
    fn summed(self) -> i64;
}
impl<I: Iterator<Item = i64>> Summed for &mut I {
    fn summed(self) -> i64 {
        let mut t = 0;
        for x in self.take(2) {
            t += x;
        }
        t
    }
}

trait Named {
    fn label(&self) -> String;
}
impl Named for Sq {
    fn label(&self) -> String {
        format!("sq {}", self.0)
    }
}
impl Named for Box<dyn Named> {
    fn label(&self) -> String {
        self.as_ref().label()
    }
}
fn show<N: Named>(n: N) -> String {
    n.label()
}

trait Sink {
    fn put(&mut self, s: &str);
}
struct Log(Vec<String>);
impl Sink for Log {
    fn put(&mut self, s: &str) {
        self.0.push(s.to_uppercase());
    }
}
impl<S: Sink + ?Sized> Sink for &mut S {
    fn put(&mut self, s: &str) {
        (**self).put(s);
    }
}
fn fill<S: Sink>(mut sink: S) {
    sink.put("a");
    sink.put("b");
}

fn main() {
    let s = Sq(2.0);
    println!("{}", total(&s));
    println!("{}", total(&&s));
    println!("{}", total(Box::new(Sq(3.0))));
    let mut c = Circle { r: 1.0 };
    println!("{}", total(&mut c));
    let r = &s;
    println!("{} {}", r.area(), r.name());
    println!("{}", (&s).name());
    println!("{}", named(&c));
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Sq(1.5)), Box::new(Circle { r: 2.0 })];
    for b in &shapes {
        println!("{} {}", total(b), b.name());
    }
    let dynref: &dyn Shape = &c;
    println!("{}", total(dynref));
    println!("{}", (&"hi").loud());
    println!("{}", (&5).loud());
    let mut it = vec![1i64, 2, 3, 4, 5].into_iter();
    println!("{}", (&mut it).summed());
    println!("{}", it.next().unwrap());
    let named: Vec<Box<dyn Named>> = vec![Box::new(Sq(4.0)), Box::new(Sq(0.5))];
    for n in named {
        println!("{}", show(n));
    }
    let mut log = Log(Vec::new());
    fill(&mut log);
    fill(&mut log);
    println!("{:?}", log.0);
}
