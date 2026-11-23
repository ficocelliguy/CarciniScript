// A trait's own generic function called through a type parameter (`T::select::<u8>(x)`): the
// impls' bodies get the classes they need (`R::default()`), also when `R` is the caller's own.
use std::fmt::Debug;

trait Order {
    fn select<R: Default + Debug>(x: i32) -> R;
    fn name() -> String;
}

struct Fresh;
struct Loud;

impl Order for Fresh {
    fn select<R: Default + Debug>(x: i32) -> R {
        let _ = x;
        R::default()
    }
    fn name() -> String {
        "fresh".to_string()
    }
}

impl Order for Loud {
    fn select<R: Default + Debug>(x: i32) -> R {
        let r = R::default();
        println!("loud {} {:?}", x, r);
        r
    }
    fn name() -> String {
        "loud".to_string()
    }
}

fn go<T: Order>() -> u8 {
    T::select::<u8>(3)
}

fn pass<T: Order, R: Default + Debug>(x: i32) -> R {
    T::select::<R>(x)
}

fn main() {
    println!("{} {}", Fresh::name(), go::<Fresh>());
    println!("{} {}", Loud::name(), go::<Loud>());
    let s: String = pass::<Loud, String>(7);
    let v: Vec<i32> = pass::<Fresh, Vec<i32>>(1);
    let f: f64 = pass::<Loud, f64>(2);
    println!("{:?} {:?} {:?}", s, v, f);
}
