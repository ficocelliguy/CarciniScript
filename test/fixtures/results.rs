// Result: Ok/Err, `?` (including inside closures), matching with exhaustiveness, combinators,
// `str::parse` for integers, floats and bools with std's error values, `Option::ok_or`, and an
// enum as the error type.
#[derive(Debug, Clone, PartialEq)]
enum CalcError {
    DivByZero,
    Parse(String),
    Negative(i64),
}

fn parse_num(s: &str) -> Result<i64, CalcError> {
    s.trim().parse::<i64>().map_err(|_| CalcError::Parse(s.to_string()))
}

fn divide(a: i64, b: i64) -> Result<i64, CalcError> {
    if b == 0 {
        return Err(CalcError::DivByZero);
    }
    Ok(a / b)
}

fn calc(a: &str, b: &str) -> Result<i64, CalcError> {
    let x = parse_num(a)?;
    let y = parse_num(b)?;
    let q = divide(x, y)?;
    if q < 0 {
        Err(CalcError::Negative(q))
    } else {
        Ok(q)
    }
}

fn describe(r: &Result<i64, CalcError>) -> String {
    match r {
        Ok(0) => "zero".to_string(),
        Ok(n) if *n > 100 => format!("big {}", n),
        Ok(n) => format!("ok {}", n),
        Err(CalcError::DivByZero) => "division by zero".to_string(),
        Err(CalcError::Parse(s)) => format!("bad number {:?}", s),
        Err(e) => format!("other {:?}", e),
    }
}

fn first_word_len(s: &str) -> Option<usize> {
    let w = s.split_whitespace().next()?;
    Some(w.len())
}

fn sum_all(items: &[&str]) -> Result<i32, std::num::ParseIntError> {
    let mut total = 0;
    for s in items {
        total += s.parse::<i32>()?;
    }
    Ok(total)
}

fn main() {
    for (a, b) in [("10", "2"), ("7", "0"), ("x", "1"), ("-9", "3"), ("1000", "3"), ("0", "5")] {
        let r = calc(a, b);
        println!("{} / {} = {:?} ({})", a, b, r, describe(&r));
    }
    println!("{:?} {:?}", sum_all(&["1", "2", "3"]), sum_all(&["1", "zz", "3"]));
    println!("{:?} {:?}", "42".parse::<u8>(), "300".parse::<u8>());
    println!("{:?} {:?} {:?}", "-1".parse::<u32>(), "".parse::<i32>(), "+7".parse::<i32>());
    println!("{:?} {:?}", "-129".parse::<i8>(), "12a".parse::<i64>());
    println!("{:?} {:?} {:?}", "2.5".parse::<f64>(), "1e3".parse::<f64>(), "abc".parse::<f64>());
    println!("{:?} {:?}", "true".parse::<bool>(), "yes".parse::<bool>());
    match "nope".parse::<i32>() {
        Ok(n) => println!("parsed {}", n),
        Err(e) => println!("error: {}", e),
    }
    let n: i32 = "17".parse().unwrap();
    let f: f64 = "0.25".parse().unwrap_or(0.0);
    println!("{} {}", n, f);

    let r: Result<i32, String> = Ok(3);
    let e: Result<i32, String> = Err("boom".to_string());
    println!("{} {} {} {}", r.is_ok(), e.is_err(), r.clone().unwrap(), e.clone().unwrap_or(-1));
    println!("{:?} {:?}", r.clone().map(|x| x * 2), e.clone().map_err(|s| s.len()));
    println!("{:?} {:?}", r.clone().ok(), e.clone().err());
    println!("{:?}", r.clone().and_then(|x| if x > 5 { Ok(x) } else { Err(format!("{} too small", x)) }));
    println!("{} {}", e.clone().unwrap_or_else(|s| s.len() as i32), r.clone().map_or(0, |x| x + 100));
    println!("{} {}", r.is_ok_and(|x| x == 3), e.is_err_and(|s| s.is_empty()));
    println!("{:?} {:?}", first_word_len("  hello world"), first_word_len("   "));
    println!("{:?} {:?}", Some(5).ok_or("none"), None::<i32>.ok_or("none"));
    let parsed: Vec<Result<u32, String>> = vec!["1", "x", "3"]
        .into_iter()
        .map(|s| s.parse::<u32>().map_err(|e| e.to_string()))
        .collect();
    println!("{:?}", parsed);
    let good: Vec<u32> = parsed.iter().filter_map(|r| r.clone().ok()).collect();
    let one = Ok::<i32, String>(1);
    println!("{:?} {}", good, one == Ok(1));
    let total = calc("100", "4").unwrap();
    println!("{}", total);
    calc("1", "0").expect("dividing went wrong");
}
