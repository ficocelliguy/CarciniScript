// `write_fmt` overridden in `fmt::Write` and `io::Write` impls (what `write!` calls), and
// `fmt::write(w, args)`.
use std::fmt;
use std::io;

struct Brackets(String);

impl fmt::Write for Brackets {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.push_str(s);
        Ok(())
    }
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        self.0.push('[');
        fmt::write(self, args)?;
        self.0.push(']');
        Ok(())
    }
}

struct Plain(String);

impl fmt::Write for Plain {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.push_str(s);
        Ok(())
    }
}

struct Counting {
    bytes: Vec<u8>,
    calls: usize,
}

impl io::Write for Counting {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> io::Result<()> {
        self.calls += 1;
        self.write_all(args.to_string().as_bytes())
    }
}

fn main() {
    use std::fmt::Write as _;
    use std::io::Write as _;
    let mut b = Brackets(String::new());
    write!(b, "a{}", 1).unwrap();
    writeln!(b, "b").unwrap();
    b.write_str("c").unwrap();
    println!("{:?}", b.0);

    let mut p = Plain(String::new());
    write!(p, "{}-{}", 2, 3).unwrap();
    p.write_char('!').unwrap();
    println!("{}", p.0);

    let mut s = String::new();
    fmt::write(&mut s, format_args!("{} {}", "x", 4)).unwrap();
    println!("{}", s);

    let mut c = Counting { bytes: Vec::new(), calls: 0 };
    write!(c, "n={}", 5).unwrap();
    writeln!(c, ";").unwrap();
    c.write_all(b"raw").unwrap();
    println!("{:?} {}", String::from_utf8(c.bytes).unwrap(), c.calls);
}
