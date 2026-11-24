use std::fmt;

pub struct Shlex { data: Vec<u8> }
impl Shlex {
    pub fn new(b: &[u8]) -> Shlex { Shlex { data: b.to_vec() } }
    pub fn count(&self) -> usize { self.data.iter().filter(|&&c| c == b' ').count() }
}

#[derive(Debug, Clone)]
pub struct Config { pub level: i32 }
impl Config {
    pub fn describe(&self) -> String { format!("level {}", self.level) }
    pub fn default_level() -> i32 { Self::new().level }
    fn new() -> Self { Config { level: 5 } }
}
impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "<{}>", self.level) }
}
pub fn make() -> Config { Config { level: 7 } }

#[derive(Debug)]
pub enum Mode { Fast, Slow }
impl Mode {
    pub fn speed(&self) -> i32 { match self { Mode::Fast => 10, Self::Slow => 1 } }
}
