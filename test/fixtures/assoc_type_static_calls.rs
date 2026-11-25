// `T::E::mk(..)`: an associated type's trait function through a type parameter, in a generic
// free function (`Self::Error::custom(..)` in a default method is `assoc_type_fields.rs`).
trait Mk {
    fn mk(n: i32) -> Self;
    fn get(&self) -> i32;
}
struct A(i32);
impl Mk for A {
    fn mk(n: i32) -> Self {
        A(n * 2)
    }
    fn get(&self) -> i32 {
        self.0
    }
}
struct B(String);
impl Mk for B {
    fn mk(n: i32) -> Self {
        B(format!("b{n}"))
    }
    fn get(&self) -> i32 {
        self.0.len() as i32
    }
}
trait Has {
    type E: Mk;
}
struct H;
impl Has for H {
    type E = A;
}
struct G;
impl Has for G {
    type E = B;
}
fn build<T: Has>(n: i32) -> T::E {
    T::E::mk(n)
}
fn twice<T>(n: i32) -> i32
where
    T: Has,
{
    let e = T::E::mk(n);
    e.get() + T::E::mk(n + 1).get()
}
fn main() {
    println!("{}", build::<H>(3).get());
    println!("{}", build::<G>(30).0);
    println!("{} {}", twice::<H>(4), twice::<G>(9));
}
