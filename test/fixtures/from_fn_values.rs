// `Type::from` named as a function value: `.map(PathBuf::from)`, `.map(Id::from)`.
use std::path::PathBuf;

#[derive(Debug)]
struct Id(u32);

impl From<u32> for Id {
    fn from(n: u32) -> Self {
        Id(n)
    }
}

fn apply<T, U>(x: T, f: impl Fn(T) -> U) -> U {
    f(x)
}

fn main() {
    let args = vec!["a/b", "c"];
    let paths: Vec<PathBuf> = args.iter().map(PathBuf::from).collect();
    let owned: Vec<String> = args.into_iter().map(String::from).collect();
    let ids: Vec<Id> = vec![1u32, 2].into_iter().map(Id::from).collect();
    let floats: Vec<f64> = vec![1u8, 2].into_iter().map(f64::from).collect();
    let wide: Vec<i64> = [3i32].iter().copied().map(i64::from).collect();
    let p = apply("x/y", std::path::PathBuf::from);
    println!("{:?} {:?} {:?} {:?} {:?} {:?}", paths, owned, ids, floats, wide, p);
}
