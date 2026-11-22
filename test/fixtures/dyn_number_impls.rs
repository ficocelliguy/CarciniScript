// `dyn Trait` on numbers whose trait has impls on several number types: each value reaches its
// own type's impl, a whole `f64` included.
use std::fmt::{Debug, Display};

trait Name: Display + Debug {
    fn name(&self) -> String;
    fn bump(&mut self);
}

impl Name for f64 {
    fn name(&self) -> String {
        format!("f64 {}", self)
    }
    fn bump(&mut self) {
        *self += 0.5;
    }
}

impl Name for i32 {
    fn name(&self) -> String {
        format!("i32 {}", self)
    }
    fn bump(&mut self) {
        *self += 1;
    }
}

impl Name for f32 {
    fn name(&self) -> String {
        format!("f32 {}", self)
    }
    fn bump(&mut self) {
        *self *= 2.0;
    }
}

fn show(x: &dyn Name) {
    println!("{} / {} / {:?}", x.name(), x, x);
}

fn describe<T: Name + ?Sized>(x: &T) -> String {
    x.name()
}

fn make(big: bool) -> Box<dyn Name> {
    if big {
        Box::new(10.0)
    } else {
        Box::new(10)
    }
}

fn main() {
    let mut v: Vec<Box<dyn Name>> = vec![Box::new(1.0f64), Box::new(2i32), Box::new(3.5f32), Box::new(4.0f64)];
    for x in &v {
        println!("{}", x.name());
    }
    for x in v.iter_mut() {
        x.bump();
    }
    for x in &v {
        show(x.as_ref());
    }
    let r: &dyn Name = &2.0f64;
    println!("{}", r.name());
    show(&7);
    show(&8.0);
    println!("{}", describe(r));
    println!("{} {}", make(true).name(), make(false).name());
    let b: Box<dyn Name> = Box::new(6.0_f32);
    println!("{:?}", b);
}
