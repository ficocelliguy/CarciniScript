// `unwrap()` / `expect()` of a `Result<_, T::Er>` with `T: Tr` and your `trait Tr { type Er; }`
// needs `T::Er: Debug`: met by a `where` clause (also through a supertrait of that bound, or with
// the bound in `where T: Tr`), an `Er = String` binding, the bound reached through a subtrait, or
// the trait's own `type Er: Dbg;`.
use std::fmt::Debug;

trait Dbg: Debug {}
impl Dbg for String {}

trait Tr {
    type Er;
    fn mk() -> Result<u8, Self::Er>;
}
trait Sub: Tr {}

fn a<T: Tr>() -> u8 where T::Er: Dbg { T::mk().unwrap() }
fn b<T: Tr<Er = String>>() -> u8 { T::mk().unwrap() }
fn c<T: Sub>() -> u8 where T::Er: Debug { T::mk().expect("c") }
fn e<T>() -> u8 where T: Tr, T::Er: Debug { T::mk().unwrap() }
fn g<T: Tr>() -> u8 where T::Er: Debug + Clone { T::mk().map_err(|e| e.clone()).unwrap() }
fn h<T: Tr>() -> u8 { T::mk().unwrap_or(0) }

trait Tr3 {
    type Er: Dbg;
    fn mk() -> Result<u8, Self::Er>;
}
fn f<T: Tr3>() -> u8 { T::mk().unwrap() }

struct A;
impl Tr for A {
    type Er = String;
    fn mk() -> Result<u8, String> { Ok(4) }
}
impl Sub for A {}
struct B;
impl Tr3 for B {
    type Er = String;
    fn mk() -> Result<u8, String> { Ok(6) }
}

fn main() {
    println!("{} {} {} {} {} {} {}", a::<A>(), b::<A>(), c::<A>(), e::<A>(), f::<B>(), g::<A>(), h::<A>());
}
