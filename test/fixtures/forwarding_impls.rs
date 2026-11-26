// Impls for `&T` / `&mut T` / `Box<T>` / `Rc<T>` that hand each call on to `T`'s impl, in the
// shapes libraries write them: `(**self).f(a)`, `<T as Tr>::f(*self, a)`, `self.as_ref().f(a)`,
// `Borrow::borrow(self).f()`, `self.clone().f()` for a method taking `self`, and associated
// functions that wrap `T`'s result (`Self::new(T::parse())`, `T::try_parse().map(Self::new)`).
// A default method the forwarding impl leaves while `T`'s impl overrides it runs `T`'s override.
use std::borrow::Borrow;
use std::rc::Rc;

trait Describe {
    fn describe(&self, n: u8) -> String;
    fn shout(&self) -> String {
        self.describe(1).to_uppercase()
    }
}
struct Cat;
impl Describe for Cat {
    fn describe(&self, n: u8) -> String {
        format!("cat x{}", n)
    }
    fn shout(&self) -> String {
        "CAT X1".to_string()
    }
}
impl<'a, T> Describe for &'a T
where
    T: Describe + ?Sized,
{
    fn describe(&self, n: u8) -> String {
        <T as Describe>::describe(*self, n)
    }
}
impl<T: Describe + ?Sized> Describe for &mut T {
    #[inline]
    fn describe(&self, n: u8) -> String {
        (**self).describe(n)
    }
}
impl<T: Describe + ?Sized> Describe for Box<T> {
    fn describe(&self, n: u8) -> String {
        self.as_ref().describe(n)
    }
}
impl<T: Describe + ?Sized> Describe for Rc<T> {
    fn describe(&self, n: u8) -> String {
        Borrow::<T>::borrow(self).describe(n)
    }
}

fn show<D: Describe>(d: D) -> String {
    format!("{} / {}", d.describe(2), d.shout())
}

trait Parse: Sized {
    fn parse() -> Self;
    fn try_parse(s: &str) -> Result<Self, String>;
}
#[derive(Debug)]
struct Num(u32);
impl Parse for Num {
    fn parse() -> Self {
        Num(7)
    }
    fn try_parse(s: &str) -> Result<Self, String> {
        s.parse().map(Num).map_err(|_| format!("bad {}", s))
    }
}
impl<T: Parse> Parse for Box<T> {
    fn parse() -> Self {
        Self::new(<T as Parse>::parse())
    }
    fn try_parse(s: &str) -> Result<Self, String> {
        <T as Parse>::try_parse(s).map(Self::new)
    }
}

trait Element {
    fn into_label(self) -> String;
}
#[derive(Clone)]
struct Node(&'static str);
impl Element for Node {
    fn into_label(self) -> String {
        format!("<{}>", self.0)
    }
}
impl<E: Element + Clone> Element for &'_ E {
    fn into_label(self) -> String {
        self.clone().into_label()
    }
}

trait Write {
    fn write(&mut self, b: &[u8]) -> usize;
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> usize {
        let s = args.to_string();
        self.write(s.as_bytes())
    }
}
struct Sink(Vec<u8>);
impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> usize {
        self.0.extend_from_slice(b);
        b.len()
    }
}
impl<W: Write + ?Sized> Write for &mut W {
    fn write(&mut self, buf: &[u8]) -> usize {
        (**self).write(buf)
    }
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> usize {
        (**self).write_fmt(args)
    }
}
fn put<W: Write>(mut w: W) -> usize {
    w.write(b"ab") + w.write_fmt(format_args!("{}", 12))
}

fn get<T: Parse>(s: &str) -> Result<T, String> {
    T::try_parse(s)
}

fn main() {
    let c = Cat;
    println!("{}", show(Cat));
    println!("{}", show(&c));
    let mut m = Cat;
    println!("{}", show(&mut m));
    println!("{}", show(Box::new(Cat)));
    println!("{}", show(Rc::new(Cat)));
    println!("{}", show(&&c));
    let b: Box<Num> = Parse::parse();
    let ok: Result<Box<Num>, String> = get("42");
    let bad: Result<Box<Num>, String> = get("x");
    println!("{:?} {:?} {:?}", b, ok, bad);
    let n = Node("li");
    println!("{} {}", (&n).into_label(), n.into_label());
    let mut s = Sink(Vec::new());
    println!("{} {:?}", put(&mut s), String::from_utf8(s.0).unwrap());
}
