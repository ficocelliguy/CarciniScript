// `FromStr`, `From<X>`, `Sum` / `Product` and marker bounds on type parameters: numbers, chars,
// strings and your own types through the caller's type.
use std::fmt::Debug;
use std::iter::Sum;
use std::ops::Add;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Cents(i64);

impl FromStr for Cents {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (d, c) = s.split_once('.').ok_or(format!("no dot in {:?}", s))?;
        let d: i64 = d.parse().map_err(|_| "bad dollars".to_string())?;
        let c: i64 = c.parse().map_err(|_| "bad cents".to_string())?;
        Ok(Cents(d * 100 + c))
    }
}

impl From<u8> for Cents {
    fn from(x: u8) -> Self {
        Cents(x as i64)
    }
}

impl From<&str> for Cents {
    fn from(s: &str) -> Self {
        s.parse().unwrap()
    }
}

impl Add for Cents {
    type Output = Cents;
    fn add(self, o: Cents) -> Cents {
        Cents(self.0 + o.0)
    }
}

impl Sum for Cents {
    fn sum<I: Iterator<Item = Cents>>(iter: I) -> Cents {
        iter.fold(Cents(0), |a, b| a + b)
    }
}

fn parse_all<T: FromStr>(s: &str) -> Vec<T>
where
    T::Err: Debug,
{
    s.split(',')
        .map(|x| x.trim().parse::<T>().unwrap())
        .collect()
}

fn try_parse<T: FromStr + Debug>(s: &str) -> String {
    match s.parse::<T>() {
        Ok(v) => format!("ok {:?}", v),
        Err(_) => "error".to_string(),
    }
}

fn widen<T: From<u8>>(x: u8) -> T {
    T::from(x)
}

fn from_text<T: From<&'static str>>(s: &'static str) -> T {
    T::from(s)
}

fn total<T: Sum<T> + Copy>(xs: &[T]) -> T {
    xs.iter().copied().sum()
}

fn product<T: std::iter::Product<T> + Copy>(xs: &[T]) -> T {
    xs.iter().copied().product()
}

fn keep<T: Send + Sync + 'static>(x: T) -> T {
    x
}

fn main() {
    let v: Vec<i64> = parse_all("1, 2, 3");
    let f: Vec<f64> = parse_all("1.5,2");
    let b: Vec<bool> = parse_all("true,false");
    let c: Vec<Cents> = parse_all("1.25, 0.05");
    println!("{:?} {:?} {:?} {:?}", v, f, b, c);
    println!(
        "{} {} {} {}",
        try_parse::<u8>("255"),
        try_parse::<u8>("256"),
        try_parse::<char>("x"),
        try_parse::<Cents>("3")
    );
    let a: u32 = widen(7);
    let d: f64 = widen(7);
    let big: u64 = widen(200);
    let ch: char = widen(65);
    let cents: Cents = widen(9);
    println!("{} {} {} {} {:?}", a, d, big, ch, cents);
    let t: String = from_text("hi");
    let m: Cents = from_text("2.50");
    println!("{} {:?}", t, m);
    println!(
        "{} {} {:?}",
        total(&[1, 2, 3]),
        total(&[0.5, 0.25]),
        total(&[Cents(5), Cents(7)])
    );
    println!(
        "{} {} {}",
        total(&[200u8, 50]),
        product(&[2, 3, 4]),
        product(&[1.5, 2.0])
    );
    println!("{}", keep(5));
}
