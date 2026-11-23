// `Err("usage")?` / `Err(format!(..))?` in a function returning `Box<dyn Error>`.
use std::error::Error;

fn parse(args: &[&str]) -> Result<u32, Box<dyn Error>> {
    if args.is_empty() {
        Err("usage: parse N")?;
    }
    if args.len() > 1 {
        Err(format!("too many arguments: {}", args.len()))?
    }
    Ok(args[0].parse::<u32>()?)
}

fn sync(n: u8) -> Result<u8, Box<dyn Error + Send + Sync>> {
    if n == 0 {
        Err("zero")?
    }
    Ok(n)
}

fn main() {
    for args in [vec![], vec!["1", "2"], vec!["x"], vec!["42"]] {
        match parse(&args) {
            Ok(n) => println!("ok {}", n),
            Err(e) => println!("error: {} / {:?}", e, e.to_string()),
        }
    }
    println!("{:?} {:?}", sync(0).map_err(|e| e.to_string()), sync(3).ok());
}
