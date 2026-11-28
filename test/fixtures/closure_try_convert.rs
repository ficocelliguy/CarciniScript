// `?` in a closure with no return type converts into the error its body names, as in rustc;
// left open, the closure's error is the operand's.
use std::num::ParseIntError;

#[derive(Debug)]
struct Big(u8);
impl From<u8> for Big {
    fn from(x: u8) -> Big {
        Big(x * 10)
    }
}

#[derive(Debug)]
enum AppError {
    Parse(ParseIntError),
    Neg(i64),
}
impl From<ParseIntError> for AppError {
    fn from(e: ParseIntError) -> Self {
        AppError::Parse(e)
    }
}

fn main() {
    // The body's turbofish names the error.
    let c = |x: Result<u32, u8>| Ok::<u32, Big>(x? + 1);
    println!("{:?} {:?}", c(Ok(4)), c(Err(3)));

    // `try_fold` with the same shape.
    let v: Vec<Result<u32, u8>> = vec![Ok(1), Ok(2), Err(7), Ok(4)];
    let r = v.into_iter().try_fold(0u32, |acc, x| Ok::<u32, Big>(acc + x?));
    println!("{:?}", r);

    // An `Err` of the body's own error after the `?`s; two `?`s with one conversion.
    let parse = |s: &str| {
        let n: i64 = s.trim().parse()?;
        let m: i64 = "2".parse()?;
        if n < 0 {
            return Err(AppError::Neg(n));
        }
        Ok(n * m)
    };
    println!("{:?}", parse(" 21 "));
    println!("{:?}", parse("-3"));
    println!("{}", matches!(parse("x"), Err(AppError::Parse(_))));

    // Left open: the operand's error.
    let plain = |s: &str| {
        let n: i32 = s.parse()?;
        Ok(n + 1)
    };
    let p: Result<i32, ParseIntError> = plain("41");
    println!("{:?} {}", p, plain("z").is_err());

    // An `async` block names its error the same way.
    let fut = async {
        let n: u32 = "5".parse()?;
        Ok::<u32, AppError>(n)
    };
    let _ = fut;
}
