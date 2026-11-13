// `impl Display` with `write!` / `writeln!` / `f.write_str`, `?` on `fmt::Result`, `{}` and
// `to_string()` on user types (nested and in collections), `impl Default` written by hand, and
// `write!` into a `String`.
use std::fmt;
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Money(u64);

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let dollars = self.0 / 100;
        let cents = self.0 % 100;
        write!(f, "${}.{:02}", dollars, cents)
    }
}

#[derive(Debug)]
enum Status {
    Idle,
    Hacking { target: String, threads: u32 },
    Failed(String),
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Idle => f.write_str("idle"),
            Status::Hacking { target, threads } => write!(f, "hacking {} with {} threads", target, threads),
            Status::Failed(why) => write!(f, "failed: {}", why),
        }
    }
}

struct Report {
    title: String,
    rows: Vec<(String, Money)>,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "== {} ==", self.title)?;
        for (name, amount) in &self.rows {
            writeln!(f, "{:<10}{}", name, amount)?;
        }
        write!(f, "{} rows", self.rows.len())
    }
}

struct Config {
    threads: u32,
    host: String,
}

impl Default for Config {
    fn default() -> Self {
        Config { threads: 4, host: "home".to_string() }
    }
}

fn main() {
    let m = Money(123456);
    println!("{} {:?} [{}]", m, m, Money(5));
    let s = m.to_string();
    println!("{} {}", s.len(), s);
    for st in [
        Status::Idle,
        Status::Hacking { target: "n00dles".to_string(), threads: 8 },
        Status::Failed("no root".to_string()),
    ] {
        println!("{} / {:?}", st, st);
    }
    let r = Report {
        title: "income".to_string(),
        rows: vec![("hack".to_string(), Money(99)), ("grow".to_string(), Money(250075))],
    };
    println!("{}", r);
    let labels: Vec<String> = vec![Money(1), Money(1000)].iter().map(|m| m.to_string()).collect();
    println!("{}", labels.join(", "));
    let msg = format!("total: {}", Money(42));
    println!("{}", msg);
    let c = Config::default();
    let d = Config { threads: 16, ..Default::default() };
    println!("{} {} {} {}", c.threads, c.host, d.threads, d.host);
    let mut out = String::new();
    write!(out, "{}-{}", 1, 2).unwrap();
    writeln!(out, "!").unwrap();
    write!(&mut out, "{}", Money(7)).unwrap();
    println!("{:?}", out);
}
