// `Option<T>` ordering: `None` is below every `Some`, `Some`s compare by their values.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct Server {
    hack_difficulty: Option<f64>,
    min_difficulty: Option<f64>,
}

fn main() {
    let s = Server { hack_difficulty: Some(5.5), min_difficulty: Some(3.0) };
    println!("{}", s.hack_difficulty > s.min_difficulty.map(|m| m + 0.5));
    let none: Option<i32> = None;
    println!("{} {} {} {}", none < Some(1), Some(-5) > none, Some(2) <= Some(2), Some(3) < Some(2));
    println!("{} {}", none >= None, Some("b") > Some("a"));
    let mut v = vec![Some(3), None, Some(-1), None, Some(2)];
    v.sort();
    println!("{v:?} {}", v[0] < v[4]);
    let t = Server { hack_difficulty: None, ..s };
    println!("{}", t < s);
}
