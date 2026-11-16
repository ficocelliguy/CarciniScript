// `std::io`: your own `Write` impl (`write!` / `writeln!` / `write_all` into it), `stdout()` and
// its lock and a `BufWriter` over it, `flush`, `io::Error` (`new`, `other`, `kind`, `{}`,
// `{:?}`, from an `ErrorKind`), `io::Result`, `?` into `Box<dyn Error>`, and `stdin()` at its end.
use std::error::Error;
use std::io::{self, BufRead, BufWriter, ErrorKind, Write};

struct Collect {
    data: Vec<u8>,
    writes: usize,
}

impl Write for Collect {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        self.data.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn check(n: i32) -> io::Result<i32> {
    if n < 0 {
        return Err(io::Error::new(ErrorKind::InvalidInput, format!("negative: {}", n)));
    }
    Ok(n * 2)
}

fn report(out: &mut impl Write, items: &[&str]) -> io::Result<()> {
    for (i, it) in items.iter().enumerate() {
        writeln!(out, "{}. {}", i + 1, it)?;
    }
    Ok(())
}

fn boxed(n: i32) -> Result<i32, Box<dyn Error>> {
    let v = check(n)?;
    Ok(v)
}

fn main() -> io::Result<()> {
    let mut c = Collect {
        data: Vec::new(),
        writes: 0,
    };
    write!(c, "x={}", 5)?;
    writeln!(c, "!")?;
    c.write_all(b"raw")?;
    println!("{:?} {}", String::from_utf8(c.data.clone()).unwrap(), c.writes > 0);
    report(&mut c, &["a", "b"])?;
    println!("{}", String::from_utf8_lossy(&c.data));

    let stdout = io::stdout();
    let mut lock = stdout.lock();
    writeln!(lock, "to stdout {}", 1)?;
    lock.flush()?;
    let mut buffered = BufWriter::new(io::stdout());
    writeln!(buffered, "buffered {}", 2)?;
    write!(buffered, "no newline, ")?;
    writeln!(buffered)?;
    buffered.flush()?;
    print!("partial ");
    io::stdout().flush()?;
    println!("line");

    println!("{:?}", check(4));
    match check(-1) {
        Ok(_) => {}
        Err(e) => println!("{} | {:?} | {:?}", e, e, e.kind()),
    }
    let other = io::Error::other("custom");
    println!("{} {:?}", other, other.kind() == ErrorKind::Other);
    let k: io::Error = ErrorKind::NotFound.into();
    println!("{} | {:?}", k, k);
    match boxed(-3) {
        Ok(_) => {}
        Err(e) => println!("boxed: {}", e),
    }

    let mut line = String::new();
    let n = io::stdin().read_line(&mut line)?;
    println!("read {} {:?}", n, line);
    let count = io::stdin().lock().lines().count();
    println!("lines {}", count);
    eprintln!("to stderr");
    Ok(())
}
