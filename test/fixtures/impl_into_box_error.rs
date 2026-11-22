// `impl Into<T> for X`, and errors converted to `Box<dyn Error>` with `.into()` / `From::from`.
use std::convert::Into;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct Number {
    value: i32,
}

impl Into<Number> for i32 {
    fn into(self) -> Number {
        Number { value: self }
    }
}

#[derive(Debug, Clone, Copy)]
struct Celsius(f64);
#[derive(Debug)]
struct Fahrenheit(f64);

impl std::convert::Into<Fahrenheit> for Celsius {
    fn into(mut self) -> Fahrenheit {
        self.0 = self.0 * 9.0 / 5.0;
        let c: Self = self;
        Fahrenheit(c.0 + 32.0)
    }
}

fn show<T: Into<Fahrenheit>>(t: T) {
    let f: Fahrenheit = t.into();
    println!("{:?}", f);
}

#[derive(Debug)]
struct EmptyVec;

impl fmt::Display for EmptyVec {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid first item to double")
    }
}

impl Error for EmptyVec {}

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn double_first(vec: Vec<&str>) -> Result<i32> {
    vec.first()
        .ok_or_else(|| EmptyVec.into())
        .and_then(|s| s.parse::<i32>().map_err(|e| e.into()).map(|i| 2 * i))
}

fn first_from(vec: Vec<&str>) -> Result<i32> {
    let s = vec.first().ok_or(EmptyVec)?;
    s.parse::<i32>().map_err(From::from)
}

fn boxed() -> Result<()> {
    Err(EmptyVec.into())
}

fn print(result: Result<i32>) {
    match result {
        Ok(n) => println!("The first doubled is {}", n),
        Err(e) => println!("Error: {}", e),
    }
}

fn main() {
    let int = 5;
    let num: Number = int.into();
    println!("My number is {:?}", num);
    let c = Celsius(100.0);
    let f: Fahrenheit = c.into();
    println!("{:?}", f);
    show(Celsius(-40.0));

    print(double_first(vec!["42", "93", "18"]));
    print(double_first(vec![]));
    print(double_first(vec!["tofu", "93", "18"]));
    print(first_from(vec!["7"]));
    print(first_from(vec![]));
    print(first_from(vec!["x"]));
    if let Err(e) = boxed() {
        println!("boxed: {}", e);
    }
}
