// `?` in a closure without a return type: the closure returns `Option` / `Result`, as rustc
// infers from its body (`try_fold`, `map`, a closure called later).
fn parse_radix(digits: &str, base: u32) -> Option<f64> {
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let i = int
        .chars()
        .try_fold(0f64, |acc, c| Some(acc * base as f64 + c.to_digit(base)? as f64))?;
    let f = frac
        .chars()
        .rev()
        .try_fold(0f64, |acc, c| Some((acc + c.to_digit(base)? as f64) / base as f64))?;
    Some(i + f)
}

fn main() {
    println!("{:?} {:?} {:?}", parse_radix("101.1", 2), parse_radix("ff", 16), parse_radix("19", 8));
    let sum = |a: &str, b: &str| Ok::<i32, std::num::ParseIntError>(a.parse::<i32>()? + b.parse::<i32>()?);
    println!("{:?} {}", sum("2", "40"), sum("x", "1").is_err());
    let firsts: Vec<Option<char>> = ["ab", "", "c"]
        .iter()
        .map(|s| {
            let c = s.chars().next()?;
            Some(c.to_ascii_uppercase())
        })
        .collect();
    println!("{firsts:?}");
}
