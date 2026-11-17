// `std::any`: `TypeId::of::<T>()` / `x.type_id()` (equal, hashable), `type_name::<T>()`, and
// `dyn Any` (`&dyn Any` from an `as_any()`, `Box<dyn Any>`) with `is::<T>()`, `downcast_ref`,
// `downcast_mut` and `downcast`.
use std::any::{type_name, Any, TypeId};
use std::collections::HashMap;

#[derive(Debug)]
struct Circle { r: f64 }
#[derive(Debug)]
struct Square(f64);
#[derive(Debug, Clone, Copy)]
enum Mode { Fast, Slow }

trait Shape: Any {
    fn area(&self) -> f64;
    fn as_any(&self) -> &dyn Any;
}
impl Shape for Circle {
    fn area(&self) -> f64 { 3.0 * self.r * self.r }
    fn as_any(&self) -> &dyn Any { self }
}
impl Shape for Square {
    fn area(&self) -> f64 { self.0 * self.0 }
    fn as_any(&self) -> &dyn Any { self }
}

fn describe(x: &dyn Any) -> String {
    if let Some(c) = x.downcast_ref::<Circle>() {
        format!("circle r={}", c.r)
    } else if let Some(s) = x.downcast_ref::<String>() {
        format!("string {s:?}")
    } else if let Some(n) = x.downcast_ref::<i64>() {
        format!("i64 {n}")
    } else if x.is::<Mode>() {
        "a mode".to_string()
    } else if x.is::<bool>() {
        "a bool".to_string()
    } else if let Some(v) = x.downcast_ref::<Vec<i32>>() {
        format!("vec of {}", v.len())
    } else {
        "something else".to_string()
    }
}

fn main() {
    println!("{}", TypeId::of::<i32>() == TypeId::of::<i32>());
    println!("{}", TypeId::of::<i32>() == TypeId::of::<u32>());
    println!("{}", TypeId::of::<Circle>() == TypeId::of::<Square>());
    println!("{}", 5u8.type_id() == TypeId::of::<u8>());
    let mut names: HashMap<TypeId, &str> = HashMap::new();
    names.insert(TypeId::of::<Circle>(), "circle");
    names.insert(TypeId::of::<Square>(), "square");
    println!("{:?}", names.get(&TypeId::of::<Square>()));
    println!("{}", type_name::<i32>());
    println!("{}", type_name::<String>());
    println!("{}", type_name::<&str>());
    println!("{}", type_name::<Vec<Option<f64>>>());
    println!("{}", type_name::<Circle>());
    println!("{}", type_name::<(u8, bool)>());
    println!("{}", std::any::type_name::<HashMap<String, Vec<u8>>>());
    println!("{}", std::any::type_name_of_val(&1.5f32));
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Circle { r: 1.0 }), Box::new(Square(2.0))];
    for s in &shapes {
        let any = s.as_any();
        println!("{} {} {:?} {:?}", s.area(), any.is::<Circle>(), any.downcast_ref::<Square>().map(|q| q.0), any.downcast_ref::<Circle>().is_some());
    }
    let things: Vec<Box<dyn Any>> = vec![
        Box::new(Circle { r: 2.0 }),
        Box::new(String::from("hi")),
        Box::new(7i64),
        Box::new(Mode::Slow),
        Box::new(true),
        Box::new(vec![1, 2, 3]),
        Box::new(Square(1.0)),
    ];
    for t in &things {
        println!("{}", describe(t.as_ref()));
    }
    let b: Box<dyn Any> = Box::new(Mode::Fast);
    match b.downcast::<Mode>() {
        Ok(m) => println!("got {:?}", m),
        Err(_) => println!("not a mode"),
    }
    let b: Box<dyn Any> = Box::new(Square(3.0));
    println!("{}", b.downcast::<Circle>().is_err());
    let mut c: Box<dyn Any> = Box::new(Circle { r: 1.0 });
    if let Some(c) = c.downcast_mut::<Circle>() {
        c.r = 9.0;
    }
    println!("{:?}", c.downcast_ref::<Circle>());
}
