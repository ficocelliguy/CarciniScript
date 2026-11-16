// `{:?}` of a value whose type is a type parameter prints as the caller's type says: `Some(..)`,
// whole floats with `.0`, chars and strings quoted, also inside `Vec<T>` / `Option<T>` /
// tuples, through a chain of generic functions, a method's own type parameter, and a generic
// struct's derived `Debug`.
use std::fmt::Debug;

fn show<T: Debug>(x: T) -> String {
    format!("{:?}", x)
}

fn both<A: Debug, B: Debug>(a: A, b: B) -> String {
    format!("({:?}, {:?})", a, b)
}

fn relay<T: Debug + Clone>(x: &T) -> String {
    let inner = show(x.clone());
    format!("[{}] {:?}", inner, vec![x.clone(), x.clone()])
}

fn first<T: Debug + Copy>(xs: &[T]) -> String {
    format!("{:?} {:?}", xs.first(), xs.iter().map(|x| (*x, 1)).collect::<Vec<_>>())
}

#[derive(Debug, Clone)]
struct Pair<T> {
    a: T,
    b: Option<T>,
}

struct Logger {
    prefix: String,
}

impl Logger {
    fn log<T: Debug>(&self, x: T) -> String {
        format!("{}: {:?}", self.prefix, x)
    }
}

fn describe<T: Debug>(p: &Pair<T>) -> String {
    format!("{:?}", p)
}

fn main() {
    println!("{} {} {} {}", show(Some("a")), show(1.0), show(vec![Some(2)]), show('x'));
    println!("{} {}", show(None::<i32>), show((2.0, "s", Some(3).is_some())));
    println!("{}", both(Some(1.5), vec![None, Some('c')]));
    println!("{}", relay(&Some(4.0)));
    println!("{}", first(&[1.0, 2.5]));
    let log = Logger { prefix: "dbg".into() };
    println!("{} {}", log.log(Some(String::from("s"))), log.log([0.5, 1.0]));
    let p = Pair { a: 1.0, b: Some(2.0) };
    println!("{} {:?}", describe(&p), p);
    println!("{}", show(p.clone()));
}
