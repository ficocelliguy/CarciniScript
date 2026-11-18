// edition: 2015
// A 2015-edition file's own `macro_rules! try` (autocfg's, to avoid the deprecated `std::try!`).
macro_rules! try {
    ($result:expr) => {
        match $result {
            Ok(value) => value,
            Err(error) => return Err(format!("own: {}", error)),
        }
    };
}

fn double(s: &str) -> Result<i32, String> {
    let n: i32 = try!(s.parse::<i32>());
    Ok(n * 2)
}

fn halve(s: &str) -> Result<f64, String> {
    let x = try!(s.trim().parse::<f64>());
    Ok(x / 2.0)
}

fn main() {
    println!("{:?} {:?}", double("21"), double("z"));
    println!("{:?} {:?}", halve(" 5 "), halve("five"));
}
