// More Option/Result/bool/iterator combinators: `transpose` both ways, `inspect` /
// `inspect_err`, `bool::then` (lazy) and `then_some`, and `Iterator::cycle`.
fn parse(s: &str) -> Option<Result<i32, String>> {
    if s.is_empty() {
        None
    } else {
        Some(s.parse::<i32>().map_err(|e| e.to_string()))
    }
}

fn main() {
    // Option<Result> -> Result<Option>, and back.
    for s in ["", "12", "x"] {
        let r: Result<Option<i32>, String> = parse(s).transpose();
        println!("{:?} -> {:?}", parse(s), r);
        println!("{:?}", r.transpose());
    }

    // inspect / inspect_err only run on their variant.
    let mut log = Vec::new();
    let a: Result<i32, String> = Ok(3);
    let b: Result<i32, String> = Err("bad".to_string());
    let a = a.inspect(|v| log.push(format!("ok {v}"))).inspect_err(|e| log.push(format!("err {e}")));
    let b = b.inspect(|v| log.push(format!("ok {v}"))).inspect_err(|e| log.push(format!("err {e}")));
    let c = Some(5).inspect(|v| log.push(format!("some {v}")));
    let d: Option<i32> = None;
    let d = d.inspect(|v| log.push(format!("some {v}")));
    println!("{:?} {:?} {:?} {:?} {:?}", a, b, c, d, log);

    // bool::then runs its closure only when true; then_some always evaluates its argument.
    let mut calls = 0;
    let evens: Vec<Option<i32>> = (0..4)
        .map(|i| {
            (i % 2 == 0).then(|| {
                calls += 1;
                i * 10
            })
        })
        .collect();
    println!("{:?} calls={calls}", evens);
    let small: Vec<i32> = (0..6).filter_map(|i| (i < 3).then_some(i)).collect();
    println!("{:?}", small);
    let label = (calls > 1).then_some("many").unwrap_or("few");
    println!("{label}");

    // cycle
    let colors: Vec<&str> = ["r", "g", "b"].iter().cycle().take(7).cloned().collect();
    println!("{:?}", colors);
    let pairs: Vec<(i32, char)> = (1..=5).zip("ab".chars().cycle()).collect();
    println!("{:?}", pairs);
    let none: Vec<i32> = Vec::<i32>::new().into_iter().cycle().take(3).collect();
    println!("{:?}", none);
}
