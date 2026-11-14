// Hand-written `impl Ord` / `PartialOrd` / `PartialEq` (a priority queue with a tie-break and
// `cmp` / `<` / `==` on the type), a boxed recursive enum evaluator, a closure in a struct
// field, `HashMap::get_mut` on struct values, and sorting floats with `partial_cmp`.
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
#[derive(Debug, PartialEq, Eq)]
struct Task { pri: u32, name: String }
impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering { self.pri.cmp(&other.pri).then_with(|| other.name.cmp(&self.name)) }
}
impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
#[derive(Debug)]
enum Expr { Num(f64), Add(Box<Expr>, Box<Expr>), Mul(Box<Expr>, Box<Expr>), Neg(Box<Expr>) }
fn eval(e: &Expr) -> f64 {
    match e { Expr::Num(n) => *n, Expr::Add(a, b) => eval(a) + eval(b), Expr::Mul(a, b) => eval(a) * eval(b), Expr::Neg(a) => -eval(a) }
}
struct Button { label: String, on_click: Box<dyn Fn(u32) -> String> }
#[derive(Debug, Clone)]
struct Srv { ram: f64, used: f64 }
fn main() {
    let mut heap = BinaryHeap::new();
    heap.push(Task { pri: 2, name: "b".into() });
    heap.push(Task { pri: 5, name: "z".into() });
    heap.push(Task { pri: 5, name: "a".into() });
    heap.push(Task { pri: 1, name: "c".into() });
    while let Some(t) = heap.pop() { print!("{}:{} ", t.pri, t.name); }
    println!();
    let e = Expr::Add(Box::new(Expr::Num(2.0)), Box::new(Expr::Mul(Box::new(Expr::Num(3.0)), Box::new(Expr::Neg(Box::new(Expr::Num(4.0)))))));
    println!("{} {:?}", eval(&e), Expr::Neg(Box::new(Expr::Num(1.0))));
    let b = Button { label: "go".into(), on_click: Box::new(|n| format!("clicked {n}x")) };
    println!("{} {}", b.label, (b.on_click)(3));
    let mut servers: HashMap<String, Srv> = HashMap::new();
    servers.insert("a".into(), Srv { ram: 8.0, used: 2.0 });
    servers.insert("b".into(), Srv { ram: 32.0, used: 30.5 });
    if let Some(s) = servers.get_mut("a") { s.used += 4.0; }
    let mut free: Vec<(String, f64)> = servers.iter().map(|(k, s)| (k.clone(), s.ram - s.used)).collect();
    free.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
    println!("{:?}", free);
    let t1 = Task { pri: 1, name: "x".into() };
    let t2 = Task { pri: 1, name: "y".into() };
    println!("{:?} {} {}", t1.cmp(&t2), t1 > t2, t1 == t2);
}
