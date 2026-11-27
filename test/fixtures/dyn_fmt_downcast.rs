// `.fmt(f)` on a `&dyn Display` and a `Box<dyn Error>` (formatting with the caller's
// `Formatter`), and `downcast_ref` / `is` on a `dyn Error`.
use std::error::Error;
use std::fmt;

struct Wrap<'a>(&'a dyn fmt::Display);
impl fmt::Display for Wrap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        self.0.fmt(f)?;
        write!(f, "]")
    }
}

struct Boxed(Box<dyn Error>);
impl fmt::Display for Boxed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug)]
struct MyErr(u8);
impl fmt::Display for MyErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "my {}", self.0)
    }
}
impl Error for MyErr {}

fn go(b: bool) -> Result<(), Box<dyn Error>> {
    if b { Err(Box::new(MyErr(3))) } else { Err("plain".into()) }
}

fn main() {
    println!("{}", Wrap(&5));
    println!("{}", Wrap(&"text"));
    let e: Box<dyn Error> = "x".parse::<i32>().unwrap_err().into();
    println!("{}", Boxed(e));
    for b in [true, false] {
        let e = go(b).unwrap_err();
        match e.downcast_ref::<MyErr>() {
            Some(m) => println!("mine {}", m.0),
            None => println!("other {}", e),
        }
        let r: &dyn Error = e.as_ref();
        println!("{} {}", r.downcast_ref::<MyErr>().is_some(), e.is::<MyErr>());
    }
}
