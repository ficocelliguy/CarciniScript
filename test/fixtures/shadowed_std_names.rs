// Your own types named like std's (`Ordering`, `Duration`, `Entry`, `Error`, a `Result` alias,
// `Instant`) shadow them everywhere, patterns included; a struct of shared references is `Copy`
// whatever they point at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ordering {
    Less,
    Greater,
}
pub enum CmpOp {
    Eq,
    Ord { ordering: Ordering, strict: bool },
}
fn show(c: CmpOp) -> &'static str {
    match c {
        CmpOp::Eq => "==",
        CmpOp::Ord { ordering: Ordering::Less, strict: false } => "<=",
        CmpOp::Ord { ordering: Ordering::Less, strict: true } => "<",
        CmpOp::Ord { ordering: Ordering::Greater, strict: false } => ">=",
        CmpOp::Ord { ordering: Ordering::Greater, strict: true } => ">",
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
struct Duration {
    secs: u64,
}
impl Duration {
    fn from_secs(secs: u64) -> Duration {
        Duration { secs }
    }
    fn as_secs(&self) -> u64 {
        self.secs * 10
    }
}
#[derive(Debug)]
enum Entry {
    Vacant(u32),
    Occupied { k: u32 },
}
#[derive(Debug)]
struct Error {
    msg: String,
}
type Result<T> = std::result::Result<T, Error>;
fn parse(s: &str) -> Result<u32> {
    s.parse::<u32>().map_err(|_| Error { msg: format!("bad {}", s) })
}
#[derive(Debug, Clone, Copy)]
struct Instant(u64);
impl Instant {
    fn now() -> Instant {
        Instant(7)
    }
    fn elapsed(&self) -> Duration {
        Duration { secs: self.0 }
    }
}
fn which(e: &Entry) -> u32 {
    match e {
        Entry::Vacant(n) => *n,
        Entry::Occupied { k } => *k + 100,
    }
}
#[derive(Clone, Copy)]
struct W<'a> {
    s: &'a [u8],
    n: &'a String,
    v: &'a Vec<i32>,
}
#[derive(Clone, Copy, Debug)]
enum Tok<'a> {
    Word(&'a str),
    Num(Option<&'a i32>),
}
fn main() {
    let n = 5;
    let toks = [Tok::Word("hi"), Tok::Num(Some(&n)), Tok::Num(None)];
    let copy = toks[1];
    println!("{:?} {:?}", toks, copy);
    let o = Ordering::Less;
    let p: Ordering = Ordering::Greater;
    println!("{} {} {:?} {:?}", show(CmpOp::Ord { ordering: o, strict: true }), show(CmpOp::Eq), o, p);
    println!("{}", show(CmpOp::Ord { ordering: Ordering::Greater, strict: false }));
    if o == Ordering::Less { println!("less"); }
    match p { Ordering::Less => println!("L"), Ordering::Greater => println!("G") }

    let d = Duration::from_secs(3);
    println!("{:?} {}", d, d.as_secs());
    let e = Entry::Vacant(4);
    let f = Entry::Occupied { k: 1 };
    println!("{} {} {:?}", which(&e), which(&f), f);
    match parse("x") {
        Ok(v) => println!("{}", v),
        Err(Error { msg }) => println!("err {}", msg),
    }
    println!("{:?}", parse("12"));
    let i = Instant::now();
    println!("{:?} {:?}", i, i.elapsed());
    let ds: Vec<Duration> = vec![Duration::from_secs(1)];
    println!("{}", ds.iter().map(|d| d.as_secs()).sum::<u64>());

    let b = vec![1u8, 2];
    let st = String::from("x");
    let vv = vec![3];
    let w = W { s: &b, n: &st, v: &vv };
    let w2 = w;
    println!("{} {} {} {}", w.s.len(), w2.s[1], w.n, w2.v[0]);
}
