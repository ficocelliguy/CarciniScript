// Impls of one trait for function pointers of different arities (`fn() -> R`, `fn(A) -> R`,
// `fn(A, B) -> R`, as in js-sys' `impl_fn!`), one class in JS: each with its own consts and
// methods, picked by the arity a function value's type knows, also through a type parameter.
pub trait JsFunction {
    const ARITY: usize;
    fn describe(&self) -> String;
}
impl<R> JsFunction for fn() -> R {
    const ARITY: usize = 0;
    fn describe(&self) -> String { format!("nullary/{}", Self::ARITY) }
}
impl<A, R> JsFunction for fn(A) -> R {
    const ARITY: usize = 1;
    fn describe(&self) -> String { format!("unary/{}", Self::ARITY) }
}
impl<A, B, R> JsFunction for fn(A, B) -> R {
    const ARITY: usize = 2;
    fn describe(&self) -> String { format!("binary/{}", Self::ARITY) }
}
fn arity_of<F: JsFunction>(_: &F) -> usize { F::ARITY }
fn describe_it<F: JsFunction>(f: &F) -> String { f.describe() }
fn zero() -> u8 { 0 }
fn one(x: u8) -> u8 { x }
fn two(x: u8, y: u8) -> u8 { x + y }
fn main() {
    let a: fn() -> u8 = zero;
    let b: fn(u8) -> u8 = one;
    let c: fn(u8, u8) -> u8 = two;
    println!("{} {} {}", arity_of(&a), arity_of(&b), arity_of(&c));
    println!("{} {} {}", a.describe(), b.describe(), c.describe());
    println!("{} {} {}", describe_it(&a), describe_it(&b), describe_it(&c));
    println!("{}", <fn(u8, u8) -> u8 as JsFunction>::ARITY);
    println!("{}", <fn(u8) -> u8 as JsFunction>::ARITY);
    println!("{}", <fn() -> u8 as JsFunction>::ARITY);
    println!("{} {}", a(), c(1, b(2)));
}
