// `write!` into a generic or user `fmt::Write` sink (a closure's parameter typed by a `where`
// bound, a local assigned later), and `&mut` into a container held in a struct
// (`let Some(file) = &mut self.file else { .. }`, `&mut self.0` on an `Option` field,
// `self.stack.last_mut()?`).
use std::fmt::{self, Write};

fn transform<F, G>(s: &str, mut with_word: F, mut boundary: G, f: &mut fmt::Formatter) -> fmt::Result
where
    F: FnMut(&str, &mut fmt::Formatter) -> fmt::Result,
    G: FnMut(&mut fmt::Formatter) -> fmt::Result,
{
    for (i, w) in s.split(' ').enumerate() {
        if i > 0 {
            boundary(f)?;
        }
        with_word(w, f)?;
    }
    Ok(())
}
pub struct Kebab<'a>(pub &'a str);
impl fmt::Display for Kebab<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        transform(self.0, |w, f| write!(f, "{}", w.to_lowercase()), |f| write!(f, "-"), f)
    }
}

pub struct Styled(String);
impl Styled {
    pub fn new() -> Self {
        Styled(String::new())
    }
}
impl fmt::Write for Styled {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.push_str(s);
        Ok(())
    }
}
fn emit<W: Write>(w: &mut W, n: u32) -> fmt::Result {
    write!(w, "n={}", n)?;
    writeln!(w, "!")
}
fn styled(last: bool) -> String {
    let mut styled;
    if last {
        styled = Styled::new();
        let _ = write!(styled, "[--{} ", 1);
    } else {
        styled = Styled::new();
        let _ = write!(styled, "<{}>", 2);
    }
    let _ = emit(&mut styled, 3);
    styled.0
}

pub struct KeyLog {
    file: Option<Vec<u8>>,
    count: Option<u32>,
    buf: Vec<u8>,
}
impl KeyLog {
    pub fn try_write(&mut self, label: &str) -> Result<(), String> {
        let Some(file) = &mut self.file else {
            return Err(format!("no file for {}", label));
        };
        self.buf.clear();
        self.buf.extend_from_slice(label.as_bytes());
        file.extend_from_slice(&self.buf);
        file.push(b'.');
        Ok(())
    }
    pub fn tick(&mut self) -> u32 {
        let Some(n) = &mut self.count else {
            return 0;
        };
        *n += 1;
        *n
    }
}
pub struct Reader(Option<Vec<u8>>, usize);
impl Reader {
    pub fn read(&mut self, out: &mut Vec<u8>) -> usize {
        let Some(src) = &mut self.0 else {
            return 0;
        };
        let n = src.len().min(2);
        out.extend(src.drain(..n));
        self.1 += n;
        n
    }
}

pub struct Frames {
    stack: Vec<std::vec::IntoIter<u32>>,
}
impl Iterator for Frames {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        loop {
            let top = self.stack.last_mut()?;
            match top.next() {
                None => drop(self.stack.pop()),
                Some(0) => self.stack.push(vec![7, 8].into_iter()),
                Some(t) => return Some(t),
            }
        }
    }
}

fn main() {
    println!("{}", Kebab("We Are Going"));
    println!("{:?} {:?}", styled(true), styled(false));
    let mut t = String::new();
    emit(&mut t, 5).unwrap();
    print!("{}", t);

    let mut k = KeyLog { file: Some(vec![]), count: Some(1), buf: vec![] };
    k.try_write("ab").unwrap();
    k.try_write("c").unwrap();
    println!("{:?} {} {} {:?}", String::from_utf8(k.file.clone().unwrap()), k.tick(), k.tick(), k.count);
    k.file = None;
    k.count = None;
    println!("{:?} {}", k.try_write("x"), k.tick());

    let mut r = Reader(Some(vec![1, 2, 3]), 0);
    let mut out = Vec::new();
    let a = r.read(&mut out);
    let b = r.read(&mut out);
    let c = r.read(&mut out);
    println!("{} {} {} {:?} {:?} {}", a, b, c, out, r.0, r.1);

    let f = Frames { stack: vec![vec![1, 0, 2].into_iter()] };
    println!("{:?}", f.collect::<Vec<_>>());
}
