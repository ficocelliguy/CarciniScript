// A generic type's class passed for a type parameter keeps its own arguments' classes (`go::<W<A>>()`).
trait Mk { fn mk(x: i32) -> i32; fn twice(x: i32) -> i32 { Self::mk(x) * 2 } }
trait Name { fn name() -> String; }
struct A;
struct B;
struct W<T>(T);
struct P<T, U>(T, U);
impl Mk for A { fn mk(x: i32) -> i32 { x + 1 } }
impl Mk for B { fn mk(x: i32) -> i32 { x * 3 } }
impl Name for A { fn name() -> String { "A".to_string() } }
impl Name for B { fn name() -> String { "B".to_string() } }
impl<T: Mk> Mk for W<T> { fn mk(x: i32) -> i32 { T::mk(x) + 10 } }
impl<T: Name> Name for W<T> { fn name() -> String { format!("W<{}>", T::name()) } }
impl<T: Mk, U: Mk> Mk for P<T, U> { fn mk(x: i32) -> i32 { T::mk(x) * 100 + U::mk(x) } }
impl<T: Name, U: Name> Name for P<T, U> { fn name() -> String { format!("P<{}, {}>", T::name(), U::name()) } }
fn go<T: Mk>(x: i32) -> i32 { T::mk(x) + T::twice(x) }
fn nm<T: Name>() -> String { T::name() }
fn both<T: Mk + Name>(x: i32) -> String { format!("{} {}", nm::<T>(), go::<T>(x)) }
fn main() {
    println!("{}", go::<W<A>>(1));
    println!("{}", go::<W<W<B>>>(2));
    println!("{}", go::<P<A, W<B>>>(3));
    println!("{}", W::<A>::mk(4));
    println!("{}", nm::<P<W<A>, B>>());
    println!("{}", both::<W<P<B, A>>>(5));
}
