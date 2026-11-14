// `From` impls declared in a module, used from the file that loads it.
mod units;
use units::{BadInput, Feet, Meters, UnitError};

fn check(s: &str) -> Result<f64, BadInput> {
    if s.is_empty() { Err(BadInput("empty".to_string())) } else { Ok(s.len() as f64) }
}

fn length(s: &str) -> Result<Meters, UnitError> {
    let n = check(s)?;
    Ok(Meters(n))
}

fn main() {
    let m: Meters = Feet(10.0).into();
    println!("{:?}", m);
    println!("{:?}", Meters::from(3));
    println!("{:?} {:?}", length("abc"), length(""));
}
