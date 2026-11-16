// The bundled `anyhow` crate: `Result`, `anyhow!` / `bail!` / `ensure!`, `.context(..)` on results and
// options, `?` wrapping std and user errors, `chain()` / `root_cause()`, and `main`'s "Caused by" report.
use anyhow::{anyhow, bail, ensure, Context, Result};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
struct NotFound(String);

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} not found", self.0)
    }
}

impl std::error::Error for NotFound {}

fn parse_port(s: &str) -> Result<u16> {
    let n: u32 = s.parse().with_context(|| format!("port {:?} isn't a number", s))?;
    ensure!(n > 0, "port must be positive, got {}", n);
    if n > 65535 {
        bail!("port {} out of range", n);
    }
    Ok(n as u16)
}

fn lookup(m: &HashMap<&str, &str>, key: &str) -> Result<u16> {
    let raw = m.get(key).context("missing key")?;
    parse_port(raw).context(format!("reading {}", key))
}

fn find(name: &str) -> Result<i32> {
    if name == "x" {
        return Ok(1);
    }
    Err(NotFound(name.to_string()))?
}

fn check(n: i32) -> Result<()> {
    ensure!(n % 2 == 0 && n > 0);
    Ok(())
}

fn main() -> Result<()> {
    let mut m = HashMap::new();
    m.insert("a", "8080");
    m.insert("b", "nope");
    m.insert("c", "70000");
    m.insert("d", "0");
    for k in ["a", "b", "c", "d", "e"] {
        match lookup(&m, k) {
            Ok(p) => println!("{} -> {}", k, p),
            Err(e) => {
                println!("{} failed: {}", k, e);
                println!("{:?}", e);
                for (i, c) in e.chain().enumerate() {
                    println!("  {}: {}", i, c);
                }
                println!("  root: {}", e.root_cause());
            }
        }
    }
    println!("{:?}", find("x").ok());
    let e = find("y").unwrap_err();
    println!("{} / {:?}", e, e);
    println!("{}", check(3).unwrap_err());
    let e2 = anyhow!("plain {}", 5);
    println!("{}", e2.context("outer"));
    let e3: anyhow::Error = anyhow!(NotFound("z".into()));
    println!("{:?}", e3);
    let r: Result<i32> = Err(anyhow!("deep")).context("mid").context("top");
    println!("{:?}", r.unwrap_err());
    lookup(&m, "b")?;
    Ok(())
}
