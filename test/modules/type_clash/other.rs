#[derive(Debug)]
pub enum Mode { Slow }

#[derive(Debug)]
pub struct Config { pub key: String, pub mode: Mode }
impl Config {
    pub fn new(k: &str) -> Config { Config { key: k.to_string(), mode: Mode::Slow } }
}
