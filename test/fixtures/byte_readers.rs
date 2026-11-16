// `R: io::Read` / `R: BufRead` / `impl BufRead` parameters given bytes (`"abc".as_bytes()`,
// `&data[..]`): the function reads them through a `Cursor` of its own. Also
// `BufReader::new(bytes)`, and a generic method.
use std::io::{self, BufRead, BufReader, Cursor, Read};

fn count<R: Read>(mut r: R) -> usize {
    let mut s = String::new();
    r.read_to_string(&mut s).unwrap();
    s.len()
}

fn lines<R: BufRead>(r: R) -> usize {
    r.lines().count()
}

fn first_line(r: impl BufRead) -> String {
    r.lines().next().unwrap().unwrap()
}

fn head<R>(mut r: R, n: usize) -> io::Result<Vec<u8>>
where
    R: Read,
{
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

fn words(r: Box<dyn BufRead>) -> Vec<String> {
    let mut out = Vec::new();
    for l in r.lines() {
        for w in l.unwrap().split_whitespace() {
            out.push(w.to_string());
        }
    }
    out
}

struct Summer;

impl Summer {
    fn sum<R: Read>(&self, r: R) -> u32 {
        r.bytes().map(|b| b.unwrap() as u32).sum()
    }
}

fn main() {
    println!("{}", count("abc".as_bytes()));
    let bytes = vec![104u8, 105, 10, 33];
    println!("{}", count(&bytes[..]));
    println!("{}", count(Cursor::new(bytes.clone())));
    // (The caller's bytes are untouched by the reads.)
    println!("{:?}", bytes);
    println!("{}", lines("a\nb\nc".as_bytes()));
    let data = "alpha beta\ngamma\n";
    println!("{}", first_line(data.as_bytes()));
    println!("{:?}", head(&b"xyz"[..], 2));
    println!("{}", head("q".as_bytes(), 2).is_err());
    println!("{:?}", words(Box::new(data.as_bytes())));
    println!("{}", Summer.sum(&[1u8, 2, 3][..]));
    let r = BufReader::new(data.as_bytes());
    for l in r.lines() {
        println!("[{}]", l.unwrap());
    }
    let mut br = BufReader::new(&bytes[..]);
    let mut line = String::new();
    br.read_line(&mut line).unwrap();
    println!("{:?}", line);
}
