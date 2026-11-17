use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct Failed {
    step: &'static str,
    cause: Box<dyn Error>,
}

impl fmt::Display for Failed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} failed: {}", self.step, self.cause)
    }
}

impl Error for Failed {}

fn parse(s: &str) -> Result<i32, Box<dyn Error>> {
    if s.is_empty() {
        return Err("empty input".into());
    }
    let n: i32 = s.parse()?;
    if n < 0 {
        return Err(format!("negative: {n}").into());
    }
    Ok(n)
}

fn main() {
    println!("{:?} {:?}", parse("7"), parse("").map_err(|e| e.to_string()));
    let e: Box<dyn Error> = "boom".into();
    let f = Failed { step: "hack", cause: e };
    println!("{f}");
    println!("{:?}", f);
    let g: Box<dyn Error + Send + Sync> = String::from("owned").into();
    println!("{g} {:?}", parse("-3").unwrap_err().to_string());
}
