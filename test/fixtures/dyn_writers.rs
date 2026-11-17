// `&mut dyn fmt::Write` / `&mut dyn io::Write` parameters (and `&mut impl ..`): given a
// `String`, a `Formatter`, your own writer, a `Vec<u8>`, `stdout()` or a `Box<dyn io::Write>`;
// `write!` / `writeln!`, `write_str` / `write_char`, `write_all` / `flush` inside. Also in
// methods, trait methods and next to other generics.
use std::fmt::{self, Write};
use std::io;

struct Upper(String);

impl fmt::Write for Upper {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.push_str(&s.to_uppercase());
        Ok(())
    }
}

fn put(w: &mut dyn fmt::Write, n: i32) -> fmt::Result {
    write!(w, "n={n}")?;
    w.write_char(';')?;
    w.write_str(" done")
}

fn put_line(w: &mut dyn Write) {
    writeln!(w, "line").unwrap();
}

fn both<T: fmt::Debug>(a: &mut (dyn fmt::Write + '_), b: &mut impl fmt::Write, x: T) {
    write!(a, "a:{x:?} ").unwrap();
    write!(b, "b:{x:?} ").unwrap();
}

struct Show(i32);

impl fmt::Display for Show {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        put(f, self.0)
    }
}

trait Render {
    fn render(&self, out: &mut dyn fmt::Write) -> fmt::Result;
}

struct Cell(&'static str);

impl Render for Cell {
    fn render(&self, out: &mut dyn fmt::Write) -> fmt::Result {
        write!(out, "[{}]", self.0)
    }
}

struct Table {
    rows: Vec<Cell>,
}

impl Table {
    fn draw<'a>(&self, out: &'a mut dyn fmt::Write) -> fmt::Result {
        for r in &self.rows {
            r.render(out)?;
        }
        out.write_char('|')
    }
}

fn io_put(w: &mut dyn io::Write, s: &str) -> io::Result<()> {
    write!(w, "<{s}>")?;
    w.write_all(b"!")?;
    w.flush()
}

fn main() {
    let mut s = String::new();
    put(&mut s, 1).unwrap();
    put_line(&mut s);
    print!("{s}");
    let mut u = Upper(String::new());
    put(&mut u, 2).unwrap();
    println!("{}", u.0);
    println!("[{}]", Show(3));
    let (mut a, mut b) = (String::new(), Upper(String::new()));
    both(&mut a, &mut b, (1, "x"));
    both(&mut b, &mut a, 2.5);
    println!("{a}/{}", b.0);
    let t = Table { rows: vec![Cell("x"), Cell("y")] };
    let mut drawn = String::new();
    t.draw(&mut drawn).unwrap();
    t.draw(&mut u).unwrap();
    println!("{drawn} {}", u.0);
    let mut buf: Vec<u8> = Vec::new();
    io_put(&mut buf, "x").unwrap();
    io_put(&mut buf, "y").unwrap();
    println!("{:?}", String::from_utf8(buf).unwrap());
    let mut out: Box<dyn io::Write> = Box::new(Vec::new());
    io_put(&mut out, "boxed").unwrap();
    let mut so = io::stdout();
    io_put(&mut so, "z").unwrap();
    println!();
}
