// `Box<dyn Tr>` / `&dyn Tr` of a value that has `Tr` only through a blanket impl
// (`impl<T: Debug> Tr for T`): numbers, strings, `Vec`s, your types, and a type parameter
// whose bounds meet the blanket impl's.
use std::fmt::{Debug, Display};

trait Tr {
    fn name(&self) -> String;
}
impl<T: Debug> Tr for T {
    fn name(&self) -> String {
        format!("{:?}", self)
    }
}
trait Shout {
    fn shout(&self) -> String;
}
impl<T: Display + ?Sized> Shout for T {
    fn shout(&self) -> String {
        format!("{}!", self).to_uppercase()
    }
}
fn make<T: Debug + 'static>(t: T) -> Box<dyn Tr> {
    Box::new(t)
}
fn show(t: &dyn Tr) -> String {
    t.name()
}
#[derive(Debug, Clone)]
struct S(u8);

trait AnyClone: 'static {
    fn clone_box(&self) -> Box<dyn AnyClone>;
    fn label(&self) -> String;
}
impl<T: Clone + Debug + 'static> AnyClone for T {
    fn clone_box(&self) -> Box<dyn AnyClone> {
        Box::new(self.clone())
    }
    fn label(&self) -> String {
        format!("<{:?}>", self)
    }
}
struct Ext {
    items: Vec<Box<dyn AnyClone>>,
}
impl Ext {
    fn insert<T: Clone + Debug + 'static>(&mut self, val: T) {
        self.items.push(Box::new(val));
    }
}

fn main() {
    let a: Box<dyn Tr> = Box::new(5u8);
    let b = make("hi");
    let v: Vec<Box<dyn Tr>> = vec![a, b, Box::new(vec![1, 2]), Box::new(S(4)), Box::new(2.5)];
    for x in &v {
        println!("{}", x.name());
    }
    println!("{} {}", show(&7i64), show(&S(1)));
    let s: Vec<Box<dyn Shout>> = vec![Box::new("ab"), Box::new(12), Box::new('c')];
    println!("{:?}", s.iter().map(|x| x.shout()).collect::<Vec<_>>());
    let mut e = Ext { items: Vec::new() };
    e.insert(3u32);
    e.insert(String::from("x"));
    e.insert(S(9));
    let copies: Vec<Box<dyn AnyClone>> = e.items.iter().map(|i| i.clone_box()).collect();
    println!("{:?}", copies.iter().map(|i| i.label()).collect::<Vec<_>>());
}
