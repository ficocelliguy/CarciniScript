// Methods and associated functions of std types named as values or called by path: `Result::is_ok`,
// `Result::unwrap_err`, `From::from`, `u64::from_str` (Rust by Example's `error/iter_result`, `flow_control/let_else`).
use std::num::ParseIntError;
use std::str::FromStr;

fn main() {
    let strings = vec!["42", "tofu", "93", "18"];
    let (numbers, errors): (Vec<_>, Vec<_>) = strings
        .into_iter()
        .map(|s| s.parse::<i32>())
        .partition(Result::is_ok);
    let numbers: Vec<_> = numbers.into_iter().map(Result::unwrap).collect();
    let errors: Vec<_> = errors.into_iter().map(Result::unwrap_err).collect();
    println!("Numbers: {:?}", numbers);
    println!("Errors: {:?}", errors);

    let v: Vec<i32> = vec!["1", "x", "3"].iter().map(|s| s.parse::<i32>()).filter_map(Result::ok).collect();
    println!("{:?}", v);
    let bad = vec!["1", "x"].iter().map(|s| s.parse::<u8>()).filter(Result::is_err).count();
    println!("{bad}");

    let r: Result<u64, ParseIntError> = Some("12").map(u64::from_str).unwrap();
    println!("{:?}", r);
    println!("{:?} {:?} {:?}", u64::from_str("7"), f64::from_str("2.5"), bool::from_str("true"));
    let w: Vec<Option<i32>> = vec![Ok::<i32, String>(1), Err("e".to_string())].into_iter().map(Result::ok).collect();
    println!("{:?}", w);
    let ws: Vec<Option<String>> = vec![Ok::<i32, String>(1), Err("e".to_string())].into_iter().map(Result::err).collect();
    println!("{:?}", ws);
    let s: Vec<String> = vec!["a", "b"].into_iter().map(From::from).collect();
    println!("{:?}", s);
    let t: String = From::from("x");
    println!("{}", t);
    let o: Vec<bool> = vec![Some(1), None].iter().map(Option::is_some).collect();
    println!("{:?}", o);
    let one: Result<i32, ()> = Ok(1);
    println!("{} {}", std::result::Result::is_ok(&one), Result::unwrap_or(Err::<i32, ()>(()), 9));
}
