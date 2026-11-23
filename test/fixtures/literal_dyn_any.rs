// Unsuffixed literals passed as `&dyn Any` are `i32` / `f64`, as rustc's fallback types.
use std::any::Any;

fn kind(v: &dyn Any) -> &'static str {
    if v.is::<i32>() {
        "i32"
    } else if let Some(_) = v.downcast_ref::<f64>() {
        "f64"
    } else if v.is::<String>() {
        "String"
    } else {
        "?"
    }
}

fn main() {
    println!("{} {} {} {}", kind(&123), kind(&1.5), kind(&String::from("s")), kind(&true));
    let b: Box<dyn Any> = Box::new(7);
    println!("{:?}", b.downcast_ref::<i32>());
}
