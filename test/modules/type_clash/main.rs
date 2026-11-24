// A struct or enum named like one of a `mod` file this file loads (`pub struct Shlex(bytes::Shlex)`),
// and two `mod` files exporting types of one name: each is its own type.
mod bytes;
mod other;
use other::{Config as OtherConfig, Mode};
use bytes::Mode as BMode;

pub struct Shlex(bytes::Shlex);
impl Shlex {
    fn new(s: &str) -> Self { Shlex(bytes::Shlex::new(s.as_bytes())) }
    fn count(&self) -> usize { self.0.count() + 100 }
}

#[derive(Debug, Clone, PartialEq)]
enum Mode2 { A }

#[derive(Debug)]
struct Config { name: String, inner: bytes::Config }

fn level(c: &bytes::Config) -> i32 { c.level }

fn main() {
    let s = Shlex::new("a b c");
    println!("{}", s.count());
    let c = Config { name: "top".to_string(), inner: bytes::Config { level: 3 } };
    println!("{:?} {} {}", c, c.inner.describe(), level(&c.inner));
    let d = bytes::make();
    println!("{} {}", d, bytes::Config::default_level());
    let o = OtherConfig::new("x");
    println!("{:?} {:?} {:?}", o, o.mode, Mode2::A);
    let m: bytes::Mode = bytes::Mode::Fast;
    println!("{:?} {} {:?}", m, m.speed(), Mode::Slow);
    let b: BMode = BMode::Slow;
    println!("{} {:?}", b.speed(), b);
    let v: Vec<bytes::Config> = vec![bytes::make(), bytes::Config { level: 1 }];
    println!("{}", v.iter().map(|c| c.level).sum::<i32>());
}
