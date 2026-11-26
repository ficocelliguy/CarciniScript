// `impl Debug for dyn Tr` (no `Box`), and `.to_string()` / `assert_eq!` / `Vec` `{:?}` through
// `impl Display` / `Debug` / `PartialEq for Box<dyn Tr>`.
use std::fmt;

trait Node {
    fn label(&self) -> String;
}
#[derive(Debug)]
struct Leaf(u8);
impl Node for Leaf {
    fn label(&self) -> String {
        format!("leaf{}", self.0)
    }
}
impl fmt::Debug for dyn Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Node({})", self.label())
    }
}

trait Shape {
    fn area(&self) -> u32;
}
#[derive(Debug, PartialEq)]
struct Sq(u32);
impl Shape for Sq {
    fn area(&self) -> u32 {
        self.0 * self.0
    }
}
impl fmt::Display for Box<dyn Shape> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "area {}", self.area())
    }
}
impl fmt::Debug for Box<dyn Shape> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shape[{}]", self.area())
    }
}
impl PartialEq for Box<dyn Shape> {
    fn eq(&self, other: &Self) -> bool {
        self.area() == other.area()
    }
}

fn main() {
    let n: Box<dyn Node> = Box::new(Leaf(3));
    println!("{:?}", n);
    let s: Box<dyn Shape> = Box::new(Sq(2));
    let t: Box<dyn Shape> = Box::new(Sq(2));
    println!("{} {:>4}|", s.to_string(), s.area());
    assert_eq!(&s, &t);
    assert!(&s == &t);
    let v: Vec<Box<dyn Shape>> = vec![s, t];
    println!("{:?}", v);
    ordering();
}

// `<` and `partial_cmp` through `impl PartialOrd for Box<dyn Ranked>`.
trait Ranked {
    fn rank(&self) -> u32;
}
struct R(u32);
impl Ranked for R {
    fn rank(&self) -> u32 {
        self.0
    }
}
impl PartialEq for Box<dyn Ranked> {
    fn eq(&self, o: &Self) -> bool {
        self.rank() == o.rank()
    }
}
impl PartialOrd for Box<dyn Ranked> {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        self.rank().partial_cmp(&o.rank())
    }
}

fn ordering() {
    let a: Box<dyn Ranked> = Box::new(R(1));
    let b: Box<dyn Ranked> = Box::new(R(2));
    println!("{:?} {} {} {} {}", a.partial_cmp(&b), &a < &b, &a >= &b, &b > &a, &a <= &a);
}
