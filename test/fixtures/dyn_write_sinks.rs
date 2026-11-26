// `write!` into a writer held in a variable, a struct field (`&mut dyn fmt::Write` over a
// `String`, a `Formatter` or your own writer), a closure's `&mut dyn Write` parameter, a generic
// sink taken by reference or by value, and a `Box<dyn Write>`.
use std::fmt::{self, Write};

struct Rep<'a> { out: &'a mut dyn fmt::Write, n: u32 }
impl<'a> Rep<'a> {
    fn line(&mut self, s: &str) -> fmt::Result {
        self.n += 1;
        writeln!(self.out, "{} {}", self.n, s)
    }
}

struct Holder<'a> { s: &'a mut String }

fn genw<W: Write>(w: &mut W, x: i32) -> fmt::Result { write!(w, "[{}]", x) }
fn imp(w: &mut impl Write, x: i32) { write!(w, "<{}>", x).unwrap(); }
fn dynp(w: &mut dyn Write, x: i32) { write!(w, "({})", x).unwrap(); }
fn owned<W: Write>(mut w: W) -> W { write!(w, "!").unwrap(); w }

fn main_a() {
    let mut s = String::new();
    {
        let out = &mut s;
        write!(out, "a{}", 1).unwrap();
        out.push('-');
        write!(out, "b").unwrap();
    }
    genw(&mut s, 2).unwrap();
    imp(&mut s, 3);
    dynp(&mut s, 4);
    {
        let r: &mut String = &mut s;
        genw(r, 5).unwrap();
    }
    let s = owned(s);
    println!("{}", s);

    let mut buf = String::new();
    {
        let mut rep = Rep { out: &mut buf, n: 0 };
        rep.line("x").unwrap();
        rep.line("y").unwrap();
    }
    print!("{}", buf);

    let mut t = String::new();
    {
        let h = Holder { s: &mut t };
        write!(h.s, "held{}", 9).unwrap();
    }
    println!("{}", t);

    let mut u = String::new();
    let mut emit = |w: &mut dyn Write, k: i32| { write!(w, "{};", k).unwrap(); };
    emit(&mut u, 1);
    emit(&mut u, 2);
    println!("{}", u);

    let mut v = String::new();
    let mut f = |x: i32| write!(v, "{}", x * 2);
    f(1).unwrap(); f(2).unwrap();
    println!("{}", v);

    let mut parts: Vec<String> = vec![String::new(), String::new()];
    write!(parts[1], "p").unwrap();
    for p in parts.iter_mut() { write!(p, "q").unwrap(); }
    println!("{:?}", parts);
}

struct Ind<'a> { out: &'a mut dyn Write, depth: usize }
impl<'a> Ind<'a> {
    fn line(&mut self, s: &str) -> fmt::Result {
        for _ in 0..self.depth { self.out.write_str("  ")?; }
        self.out.write_str(s)?;
        self.out.write_char('\n')
    }
}

struct Tree(Vec<(usize, &'static str)>);
impl fmt::Display for Tree {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut ind = Ind { out: f, depth: 0 };
        for (d, s) in &self.0 {
            ind.depth = *d;
            ind.line(s)?;
        }
        Ok(())
    }
}

struct Count { n: usize }
impl Write for Count {
    fn write_str(&mut self, s: &str) -> fmt::Result { self.n += s.len(); Ok(()) }
}

fn render(out: &mut String, items: &[i32]) {
    let mut ind = Ind { out, depth: 1 };
    for i in items { ind.line(&i.to_string()).unwrap(); }
}

fn main_b() {
    let t = Tree(vec![(0, "root"), (1, "a"), (2, "b"), (1, "c")]);
    print!("{}", t);
    let s = format!("{}", t);
    println!("{}", s.len());
    let mut c = Count { n: 0 };
    {
        let mut ind = Ind { out: &mut c, depth: 2 };
        ind.line("xyz").unwrap();
    }
    println!("{}", c.n);
    let mut out = String::new();
    render(&mut out, &[1, 2]);
    print!("{}", out);
    let mut sinks: Vec<Box<dyn Write>> = vec![Box::new(Count { n: 0 })];
    for s in sinks.iter_mut() { write!(s, "abc").unwrap(); }
}
fn main() {
    main_a();
    main_b();
}
