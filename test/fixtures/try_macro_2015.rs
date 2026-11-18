// edition: 2015
// 2015-edition code: std's `try!(x)` (`x?`), where `try` isn't a keyword yet
// (`own_try_macro_2015.rs` has a file's own `macro_rules! try`).
use std::num::ParseIntError;

#[derive(Debug)]
enum AppError {
    Parse(ParseIntError),
}

impl From<ParseIntError> for AppError {
    fn from(e: ParseIntError) -> Self {
        AppError::Parse(e)
    }
}

fn parse(s: &str) -> Result<i32, ParseIntError> {
    let n = try!(s.parse::<i32>());
    Ok(n + 1)
}

// `try!` converts the error with `From`, like `?`.
fn sum(a: &str, b: &str) -> Result<i32, AppError> {
    let x = try!(a.parse::<i32>());
    let y = try!(parse(b));
    Ok(x + y)
}

fn main() {
    println!("{:?} {}", parse("41"), parse("x").is_err());
    println!("{:?}", sum("1", "2"));
    match sum("1", "q") {
        Ok(_) => println!("ok?"),
        Err(AppError::Parse(e)) => println!("parse error: {}", e),
    }
    let try_count = 3;
    println!("{}", try_count);
}
