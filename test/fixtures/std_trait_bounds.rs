// Std traits as type parameter bounds: `T: TryFrom<X>` (`T::try_from(x)` on integer kinds, `char`,
// your own `TryFrom` impls and through a `From` impl; several bounds picked by the argument;
// `T::Error` in a signature), `B: AsMut<[u8]>` / `S: AsMut<Vec<i32>>` (`b.as_mut()[0] = 7`),
// `UnwindSafe` / `RefUnwindSafe` on functions and structs, and `R: Read` / `W: Write` in
// `where` clauses with `Send + 'static`.
use std::convert::TryFrom;
use std::fmt::{Debug, Display};
use std::io::{self, Cursor, Read, Write};
use std::panic::{RefUnwindSafe, UnwindSafe};

#[derive(Debug)]
struct Even(u32);

impl TryFrom<u32> for Even {
    type Error = String;
    fn try_from(n: u32) -> Result<Self, Self::Error> {
        if n % 2 == 0 {
            Ok(Even(n))
        } else {
            Err(format!("{} is odd", n))
        }
    }
}

impl TryFrom<&str> for Even {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Ok(Even(s.len() as u32 * 2))
    }
}

#[derive(Debug)]
struct Meters(f64);

impl From<i32> for Meters {
    fn from(n: i32) -> Self {
        Meters(n as f64)
    }
}

fn parse<T: TryFrom<u32>>(n: u32) -> Result<T, T::Error> {
    T::try_from(n)
}

fn show<T>(n: u32)
where
    T: TryFrom<u32> + Debug,
    T::Error: Debug,
{
    println!("{:?}", T::try_from(n));
}

fn either<T>(a: u64, b: i64) -> (Option<T>, Option<T>)
where
    T: TryFrom<u64> + TryFrom<i64>,
    <T as TryFrom<u64>>::Error: Display,
    <T as TryFrom<i64>>::Error: Display,
{
    (T::try_from(a).ok(), T::try_from(b).ok())
}

fn named<T: TryFrom<&'static str>>(s: &'static str) -> Option<T> {
    T::try_from(s).ok()
}

fn meters<T: TryFrom<i32>>(n: i32) -> bool {
    T::try_from(n).is_ok()
}

fn fill<B: AsMut<[u8]>>(mut b: B) -> B {
    b.as_mut()[0] = 7;
    b.as_mut()[1] += 1;
    b
}

fn grow<S: AsMut<Vec<i32>>>(s: &mut S) {
    s.as_mut().push(9);
}

fn run<F: FnOnce() -> i32 + UnwindSafe>(f: F) -> i32 {
    f()
}

fn peek<T: Debug + RefUnwindSafe>(t: &T) {
    println!("{:?}", t);
}

struct Guard<T: UnwindSafe + RefUnwindSafe> {
    v: T,
}

impl<T: UnwindSafe + RefUnwindSafe + Debug> Guard<T> {
    fn get(&self) -> &T {
        &self.v
    }
}

fn pipe<R, W>(mut r: R, w: &mut W) -> io::Result<usize>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let mut s = Vec::new();
    let n = r.read_to_end(&mut s)?;
    w.write_all(&s)?;
    Ok(n)
}

fn copy_all<I, O>(r: &mut I, w: &mut O) -> Result<(), io::Error>
where
    I: Read,
    O: Write,
{
    let mut buf = [0u8; 4];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        w.write_all(&buf[..n])?;
    }
}

fn main() {
    let a: Result<Even, String> = parse(4);
    let b: Result<Even, String> = parse(5);
    println!("{:?} {:?}", a, b);
    let c: Result<u8, _> = parse(300);
    let d: Result<u8, _> = parse(30);
    println!("{:?} {:?}", c.is_err(), d);
    show::<i8>(200);
    show::<u16>(200);
    show::<Even>(7);
    show::<char>(65);
    println!("{:?}", either::<u8>(7, -1));
    println!("{:?}", either::<i8>(700, -100));
    println!("{:?}", named::<Even>("abc"));
    println!("{}", meters::<Meters>(3));
    match parse::<i64>(9) {
        Ok(n) => println!("ok {}", n + 1),
        Err(e) => println!("err {:?}", e),
    }
    let mut v = vec![1u8, 2];
    v.as_mut_slice()[1] = 5;
    println!("{:?} {:?}", fill(vec![1u8, 2]), v);
    let mut w = vec![1];
    grow(&mut w);
    println!("{:?}", w);
    println!("{}", run(|| 3));
    peek(&vec![1, 2]);
    let g = Guard { v: vec![1, 2] };
    println!("{:?}", g.get());
    let mut out: Vec<u8> = Vec::new();
    let n = pipe(Cursor::new(b"hello".to_vec()), &mut out).unwrap();
    println!("{} {:?}", n, out);
    let mut src = Cursor::new(b"abcdefghij".to_vec());
    let mut dst: Vec<u8> = Vec::new();
    copy_all(&mut src, &mut dst).unwrap();
    println!("{:?}", String::from_utf8(dst).unwrap());
}
