// Impls of one trait for `Rc<T>`, `Box<T>` and `Arc<T>` (one value in JS: the box is erased)
// that behave differently, as in borsh's schema: `<Rc<u8> as Schema>::declaration()`,
// `dep::<Box<u8>>()` and a boxed value passed for `S` each reach their own impl.
use std::rc::Rc;
use std::sync::Arc;

pub trait Schema {
    fn declaration() -> String;
    fn show(&self) -> String;
}
impl Schema for u8 {
    fn declaration() -> String { "u8".to_string() }
    fn show(&self) -> String { format!("{}u8", self) }
}
impl<T: Schema> Schema for Rc<T> {
    fn declaration() -> String { format!("Rc<{}>", T::declaration()) }
    fn show(&self) -> String { format!("rc({})", (**self).show()) }
}
impl<T: Schema> Schema for Box<T> {
    fn declaration() -> String { format!("Box<{}>", T::declaration()) }
    fn show(&self) -> String { format!("box({})", (**self).show()) }
}
impl<T: Schema> Schema for Arc<T> {
    fn declaration() -> String { format!("Arc<{}>", T::declaration()) }
    fn show(&self) -> String { format!("arc({})", (**self).show()) }
}
pub fn dep<S: Schema>() -> String { S::declaration() }
pub fn shown<S: Schema>(s: S) -> String { s.show() }
fn main() {
    println!("{} {}", <Rc<u8> as Schema>::declaration(), <Box<u8> as Schema>::declaration());
    println!("{} {}", <Rc<u8>>::declaration(), Box::<u8>::declaration());
    println!("{} {} {}", dep::<Box<u8>>(), dep::<Rc<u8>>(), dep::<Arc<Box<u8>>>());
    println!("{} {}", dep::<u8>(), dep::<Rc<Box<u8>>>());
    println!("{} {} {}", shown(Rc::new(1u8)), shown(Box::new(2u8)), shown(Arc::new(Rc::new(3u8))));
}
