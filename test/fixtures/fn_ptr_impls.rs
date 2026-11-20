// Your trait on a function pointer type (`impl Tr for fn() -> i32`): called directly, through a
// generic `T: Tr`, through `dyn Tr`, next to the trait's impls for other types.
trait Tr {
    fn call_twice(&self) -> i32;
    fn name() -> &'static str
    where
        Self: Sized,
    {
        "fnptr"
    }
    fn describe(&self) -> String
    where
        Self: Sized,
    {
        format!("{} -> {}", Self::name(), self.call_twice())
    }
}
impl Tr for fn() -> i32 {
    fn call_twice(&self) -> i32 {
        self() + self()
    }
}
trait Apply {
    fn apply_to(&self, x: i32) -> i32;
}
impl Apply for fn(i32) -> i32 {
    fn apply_to(&self, x: i32) -> i32 {
        self(x) * 10
    }
}
impl Apply for i32 {
    fn apply_to(&self, x: i32) -> i32 {
        self + x
    }
}
fn one() -> i32 {
    1
}
fn two() -> i32 {
    2
}
fn double(x: i32) -> i32 {
    x * 2
}
fn generic<T: Tr>(t: &T) -> i32 {
    t.call_twice()
}
fn generic_apply<A: Apply>(a: A, x: i32) -> i32 {
    a.apply_to(x)
}
fn main() {
    let f: fn() -> i32 = one;
    let g: fn() -> i32 = two;
    println!("{} {} {}", f.call_twice(), g.call_twice(), generic(&f));
    println!("{}", f.describe());
    let d: &dyn Tr = &g;
    println!("{}", d.call_twice());
    let h: fn(i32) -> i32 = double;
    println!("{} {}", h.apply_to(3), generic_apply(h, 4));
    println!("{}", generic_apply(5, 4));
    let items: Vec<Box<dyn Apply>> = vec![Box::new(h), Box::new(7i32)];
    for i in &items {
        println!("{}", i.apply_to(1));
    }
}
