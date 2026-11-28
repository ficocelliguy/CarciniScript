// `Self::Output` naming a supertrait's associated type, in the subtrait and its impls, and a
// method of an impl whose `where Self: Tr<A>` clause gives `self` the bound's generic method.
pub trait FnOnce1<A> {
    type Output;
    fn call_once(self, arg: A) -> Self::Output;
}
pub trait FnMut1<A>: FnOnce1<A> {
    fn call_mut(&mut self, arg: A) -> Self::Output;
}

#[derive(Default, Clone, Copy)]
pub struct MergeResultFn;

impl<T> FnOnce1<Result<T, T>> for MergeResultFn {
    type Output = T;
    fn call_once(self, arg: Result<T, T>) -> T {
        match arg {
            Ok(x) => x,
            Err(x) => x,
        }
    }
}

impl<A> FnMut1<A> for MergeResultFn
where
    Self: FnOnce1<A>,
{
    fn call_mut(&mut self, arg: A) -> Self::Output {
        self.call_once(arg)
    }
}

trait Shape {
    type Unit;
    fn area(&self) -> Self::Unit;
}
trait Solid: Shape {
    fn volume(&self, depth: Self::Unit) -> Self::Unit;
}
trait Labeled: Solid {
    fn label(&self) -> String;
    fn describe(&self, depth: Self::Unit) -> String
    where
        Self::Unit: std::fmt::Debug,
    {
        format!("{} {:?}", self.label(), self.volume(depth))
    }
}

struct Sq(f64);
impl Shape for Sq {
    type Unit = f64;
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}
impl Solid for Sq {
    fn volume(&self, depth: Self::Unit) -> Self::Unit {
        self.area() * depth
    }
}
impl Labeled for Sq {
    fn label(&self) -> String {
        format!("sq{}", self.0)
    }
}

fn total<S: Solid<Unit = f64>>(s: &S, d: f64) -> f64 {
    s.volume(d) + s.area()
}

fn first_big(v: &[u32]) -> u32 {
    let mut o = None;
    for &x in v {
        if x > 2 && o.is_none() {
            o = Some(x);
        }
    }
    o.unwrap_or_default() + 1
}

fn main() {
    let mut m = MergeResultFn;
    let a: u8 = m.call_mut(Ok(3));
    let b: u8 = m.call_mut(Err(4));
    let s: String = m.call_mut(Err::<String, String>("e".to_string()));
    println!("{a} {b} {s}");
    let q = Sq(2.0);
    println!("{} {} {}", q.volume(3.0), total(&q, 0.5), q.describe(1.3));
    println!("{} {}", first_big(&[1, 5, 7]), first_big(&[1]));
}
