// One trait for several tuples of one length (`(u8, u8)`, `(i32, i32)`, `(String, String)`,
// `(T, bool)`): each impl's methods, and its copy of the trait's default methods, picked by the
// elements, called directly, through a type parameter (by the class the caller passes) and
// through `dyn` (by the values).
use std::fmt::Display;

#[derive(Debug, Clone, Copy)]
struct P(i32);

trait Kind {
    fn kind(&self) -> String;
    fn twice(&self) -> String {
        format!("{0}{0}", self.kind())
    }
}
impl Kind for (u8, u8) {
    fn kind(&self) -> String {
        format!("u8s {} ", self.0 as u32 + self.1 as u32)
    }
}
impl Kind for (i32, i32) {
    fn kind(&self) -> String {
        format!("i32s {} ", self.0 - self.1)
    }
}
impl Kind for (f64, f64) {
    fn kind(&self) -> String {
        format!("f64s {:?} ", self.0)
    }
}
impl Kind for (String, String) {
    fn kind(&self) -> String {
        format!("strs {}{} ", self.0, self.1)
    }
}
impl<T: Display> Kind for (T, bool) {
    fn kind(&self) -> String {
        format!("flag {} {} ", self.0, self.1)
    }
}
impl Kind for (P, P) {
    fn kind(&self) -> String {
        format!("ps {} ", self.1 .0)
    }
}
impl Kind for (u8, u8, u8) {
    fn kind(&self) -> String {
        "three ".to_string()
    }
}

fn by_class<T: Kind>(t: T) -> String {
    t.kind()
}
fn by_ref<T: Kind + ?Sized>(t: &T) -> String {
    t.twice()
}

fn main() {
    println!("{}", (1u8, 2u8).kind());
    println!("{}", (1i32, 2i32).kind());
    // (An unsuffixed integer: rustc's `i32` fallback picks between the impls.)
    println!("{}", (7, 2).kind());
    println!("{}", (1.5, 2.0).kind());
    println!("{}", (String::from("a"), String::from("b")).kind());
    println!("{}", ("s", true).kind());
    println!("{}", (P(3), P(4)).kind());
    println!("{}", (1u8, 2u8, 3u8).kind());
    println!("{}", (5i32, 6i32).twice());
    println!("{}{}{}", by_class((7u8, 8u8)), by_class((7i32, 8i32)), by_class((2.0f64, 3.0)));
    println!("{}{}", by_class((9, false)), by_class(("x".to_string(), "y".to_string())));
    println!("{}{}", by_ref(&(5u8, 6u8)), by_ref(&(5i32, 6i32)));
    let v: Vec<Box<dyn Kind>> = vec![
        Box::new((1u8, 1u8)),
        Box::new((1.5f64, 2.5f64)),
        Box::new(("p".to_string(), "q".to_string())),
        Box::new(('c', true)),
        Box::new((P(9), P(8))),
    ];
    for k in &v {
        println!("{}", k.kind());
    }
}
