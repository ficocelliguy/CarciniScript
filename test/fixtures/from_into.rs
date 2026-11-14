// `From` / `Into`: several `impl From<X> for Y` on one type (picked by the argument's type),
// `Y::from(x)`, `x.into()` from the expected type (let annotations, arguments, returns, struct
// fields), std conversions (`&str` -> `String`, lossless numeric widening), and `?` converting
// error types through `From`.
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
struct Celsius(f64);

#[derive(Debug, Clone, PartialEq)]
struct Fahrenheit(f64);

impl From<Celsius> for Fahrenheit {
    fn from(c: Celsius) -> Self {
        Fahrenheit(c.0 * 9.0 / 5.0 + 32.0)
    }
}

#[derive(Debug)]
enum Value {
    Num(i64),
    Text(String),
    Flag(bool),
    List(Vec<Value>),
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Num(n)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Text(s.to_string())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Flag(b)
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::List(v)
    }
}

#[derive(Debug)]
struct ParseErr {
    line: usize,
}

#[derive(Debug)]
enum AppError {
    Parse(ParseErr),
    Missing(String),
}

impl From<ParseErr> for AppError {
    fn from(e: ParseErr) -> Self {
        AppError::Parse(e)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::Parse(p) => write!(f, "parse error on line {}", p.line),
            AppError::Missing(what) => write!(f, "missing {what}"),
        }
    }
}

fn parse_line(n: usize, s: &str) -> Result<i64, ParseErr> {
    s.trim().parse::<i64>().map_err(|_| ParseErr { line: n })
}

fn sum_lines(text: &str) -> Result<i64, AppError> {
    if text.is_empty() {
        return Err(AppError::Missing("input".into()));
    }
    let mut total = 0;
    for (i, l) in text.lines().enumerate() {
        total += parse_line(i + 1, l)?;
    }
    Ok(total)
}

fn describe(v: Value) -> String {
    match v {
        Value::Num(n) => format!("num {n}"),
        Value::Text(t) => format!("text {t:?}"),
        Value::Flag(b) => format!("flag {b}"),
        Value::List(items) => format!(
            "list [{}]",
            items.into_iter().map(describe).collect::<Vec<_>>().join(", ")
        ),
    }
}

struct Labeled {
    name: String,
    temp: Fahrenheit,
}

fn widen(x: i32) -> i64 {
    x.into()
}

fn main() {
    let boiling = Fahrenheit::from(Celsius(100.0));
    println!("{:?}", boiling);
    let body: Fahrenheit = Celsius(37.0).into();
    println!("{:?} {}", body, body == Fahrenheit(98.60000000000001));

    println!("{}", describe(Value::from(42)));
    println!("{}", describe(Value::from("hi")));
    println!("{}", describe(true.into()));
    let v: Value = vec![Value::from(1), "two".into(), false.into()].into();
    println!("{}", describe(v));

    let l = Labeled {
        name: "oven".into(),
        temp: Celsius(200.0).into(),
    };
    println!("{} {:?}", l.name, l.temp);

    let s: String = "owned".into();
    let big: i64 = widen(7) + i64::from(3u8);
    let f: f64 = 5i32.into();
    let u: u32 = 200u8.into();
    println!("{s} {big} {f} {u}");

    for input in ["1\n2\n3", "4\nx\n6", ""] {
        match sum_lines(input) {
            Ok(n) => println!("sum = {n}"),
            Err(e) => println!("error: {e} ({:?})", e),
        }
    }
}
