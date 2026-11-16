// `fmt::Write`: your own (`write!` into it, `write_str` / `write_char`, `Err(fmt::Error)`), a
// generic `W: fmt::Write` given a `String`, a `Formatter` or your own writer, and `String`'s own
// `write_str` / `write_char`.
use std::fmt::{self, Write};

struct Upper(String);

impl Write for Upper {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.push_str(&s.to_uppercase());
        Ok(())
    }
}

// Counts what it's given, and refuses past a limit.
struct Capped {
    n: usize,
    cap: usize,
}

impl fmt::Write for Capped {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.n + s.len() > self.cap {
            return Err(fmt::Error);
        }
        self.n += s.len();
        Ok(())
    }
}

fn row<W: Write>(w: &mut W, name: &str, hp: i32) -> fmt::Result {
    write!(w, "{name:<6}")?;
    w.write_char('|')?;
    w.write_str(" hp ")?;
    writeln!(w, "{hp:>4}")
}

fn table<W>(w: &mut W, rows: &[(&str, i32)]) -> fmt::Result
where
    W: fmt::Write,
{
    for (name, hp) in rows {
        row(w, name, *hp)?;
    }
    Ok(())
}

struct Report(Vec<(&'static str, i32)>);

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "report:")?;
        table(f, &self.0)
    }
}

struct Holder {
    text: String,
}

fn main() {
    let rows = [("alice", 12), ("bob", 340)];

    // A `String`, through the generic.
    let mut s = String::new();
    table(&mut s, &rows).unwrap();
    print!("{s}");

    // A field.
    let mut h = Holder { text: String::from(">") };
    row(&mut h.text, "carol", 7).unwrap();
    row(&mut h.text, "dave", 8).unwrap();
    print!("{}", h.text);

    // Your own writers.
    let mut u = Upper(String::new());
    write!(u, "hi {}", 5).unwrap();
    u.write_char('!').unwrap();
    table(&mut u, &rows[..1]).unwrap();
    print!("{}", u.0);
    let mut c = Capped { n: 0, cap: 20 };
    // (How many `write_str` calls `{:<6}` makes is up to std, so only the outcome.)
    println!("{:?} {}", table(&mut c, &rows), c.n <= 20);

    // A `Formatter`, through `Display`.
    print!("{}", Report(vec![("eve", 1)]));

    // `String`'s own methods.
    let mut t = String::new();
    t.write_str("ab").unwrap();
    t.write_char('c').unwrap();
    write!(t, "{}", 1).unwrap();
    println!("{t}");
}
