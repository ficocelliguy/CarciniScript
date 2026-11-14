// `Box<dyn Error>`: `?` passes a std error (ParseIntError) and your own `impl Error` type through
// unchanged, `Err(Box::new(..))`, `{}` of the boxed error, and `ok_or_else` on a lookup.
use std::error::Error;
use std::fmt;
#[derive(Debug)]
enum ConfigError { Missing(String), Bad { key: String, value: String } }
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::Missing(k) => write!(f, "missing key {k}"),
            ConfigError::Bad { key, value } => write!(f, "bad value {value:?} for {key}"),
        }
    }
}
impl Error for ConfigError {}
fn get<'a>(pairs: &'a [(&str, &str)], key: &str) -> Result<&'a str, ConfigError> {
    pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).ok_or_else(|| ConfigError::Missing(key.to_string()))
}
fn threads(pairs: &[(&str, &str)]) -> Result<u32, Box<dyn Error>> {
    let raw = get(pairs, "threads")?;
    let n: u32 = raw.parse()?;
    if n == 0 { return Err(Box::new(ConfigError::Bad { key: "threads".into(), value: raw.into() })); }
    Ok(n * 2)
}
fn main() {
    let ok = [("threads", "4")];
    let bad_num = [("threads", "four")];
    let zero = [("threads", "0")];
    let none: [(&str, &str); 0] = [];
    for cfg in [&ok[..], &bad_num[..], &zero[..], &none[..]] {
        match threads(cfg) {
            Ok(n) => println!("ok {n}"),
            Err(e) => println!("error: {e}"),
        }
    }
}
