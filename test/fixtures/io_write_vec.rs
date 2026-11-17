// `io::Write` into a `Vec<u8>` (also one made with `Vec::new()` that `write!` makes a
// `Vec<u8>`), and a generic `W: io::Write` given a `Vec<u8>` or stdout.
use std::io::{self, Write};

fn emit<W: Write>(w: &mut W, n: u32) -> io::Result<()> {
    writeln!(w, "line {n}")?;
    w.write_all(b"raw\n")
}

fn render(rows: &[(u32, &str)]) -> io::Result<String> {
    let mut out = Vec::new();
    for (n, name) in rows {
        write!(out, "{n:>3} {name}")?;
        out.push(b';');
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn main() {
    let mut buf: Vec<u8> = Vec::new();
    write!(buf, "{}-", 5).unwrap();
    writeln!(&mut buf, "x").unwrap();
    buf.write_all(b"ab").unwrap();
    let n = buf.write(b"cd").unwrap();
    buf.flush().unwrap();
    println!("{:?} {n}", String::from_utf8(buf.clone()).unwrap());
    emit(&mut buf, 2).unwrap();
    println!("{} {:?}", buf.len(), &buf[buf.len() - 4..]);
    emit(&mut io::stdout(), 3).unwrap();
    println!("{}", render(&[(1, "a"), (22, "bb")]).unwrap());
    // A `Vec::new()` that only the generic writer call makes a `Vec<u8>`.
    let mut out = Vec::new();
    emit(&mut out, 4).unwrap();
    println!("{:?}", out);
}
