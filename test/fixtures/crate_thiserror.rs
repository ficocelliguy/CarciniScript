// The bundled `thiserror`: `#[derive(Error)]` with `#[error(..)]` (positional, named and extra
// arguments), `#[from]` (`From` impls and `?`), `#[source]` and `transparent`, compared with the real crate.
use std::num::ParseIntError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("missing key `{0}`")]
    Missing(String),
    #[error("bad value for {key}: expected {expected:?}, got {got}")]
    Bad {
        key: String,
        expected: Vec<i32>,
        got: i64,
    },
    #[error("not a number")]
    Parse(#[from] ParseIntError),
    #[error("{} entries is too many (max {})", .0.len(), .1)]
    TooMany(Vec<u8>, #[allow(unused)] usize),
    #[error("empty config")]
    Empty,
    #[error(transparent)]
    Io(#[from] IoLike),
}

#[derive(Error, Debug, PartialEq)]
#[error("io failed: {msg}")]
pub struct IoLike {
    msg: String,
}

#[derive(Error, Debug)]
#[error("wrapped at step {step}")]
pub struct Wrapped {
    step: u32,
    #[source]
    cause: ConfigError,
}

fn parse(s: &str) -> Result<i64, ConfigError> {
    if s.is_empty() {
        return Err(ConfigError::Empty);
    }
    Ok(s.parse::<i64>()?)
}

fn chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        out.push_str(" <- ");
        out.push_str(&c.to_string());
        cur = c.source();
    }
    out
}

fn main() {
    let max = 3;
    let errs: Vec<ConfigError> = vec![
        ConfigError::Missing("port".into()),
        ConfigError::Bad {
            key: "mode".into(),
            expected: vec![1, 2],
            got: 7,
        },
        ConfigError::TooMany(vec![1, 2, 3, 4], max),
        ConfigError::Empty,
        IoLike { msg: "disk".into() }.into(),
    ];
    for e in &errs {
        println!("{}", e);
    }
    println!("{:?}", parse("12"));
    println!("{}", parse("x").unwrap_err());
    println!("{}", parse("").unwrap_err());
    let w = Wrapped {
        step: 2,
        cause: parse("zz").unwrap_err(),
    };
    println!("{}", chain(&w));
    println!("{}", chain(&ConfigError::from(IoLike { msg: "net".into() })));
}
