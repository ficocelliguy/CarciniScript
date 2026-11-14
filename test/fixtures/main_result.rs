// `fn main() -> Result<(), Box<dyn Error>>` returning `Err`: the program fails like rustc's does
// (exit code 1, `Error: ParseIntError { kind: InvalidDigit }`), after printing what came before.
use std::error::Error;
fn main() -> Result<(), Box<dyn Error>> {
    let a: i32 = "12".parse()?;
    println!("got {a}");
    let b: i32 = "x1".parse()?;
    println!("never {b}");
    Ok(())
}
