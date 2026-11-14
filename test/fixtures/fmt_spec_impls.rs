// User `Display` / `Debug` impls get the format spec through the Formatter: `width()`,
// `precision()`, `fill()`, `sign_plus()`, `alternate()`, and `pad()`, which cuts to the
// precision and pads to the width. An impl that only `write!`s ignores the width, like Rust.
use std::fmt;

struct Plain(u32);

impl fmt::Display for Plain {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "p{}", self.0)
    }
}

impl fmt::Debug for Plain {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "P{}", self.0)
    }
}

struct Name(&'static str);

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.0)
    }
}

struct Spec;

impl fmt::Display for Spec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "w={:?} p={:?} fill={:?} plus={} alt={} zero={}",
            f.width(),
            f.precision(),
            f.fill(),
            f.sign_plus(),
            f.alternate(),
            f.sign_aware_zero_pad()
        )
    }
}

struct Money(f64);

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let digits = f.precision().unwrap_or(2);
        let s = format!("${:.*}", digits, self.0);
        match f.width() {
            Some(w) if w > s.len() => write!(f, "{}{}", " ".repeat(w - s.len()), s),
            _ => write!(f, "{}", s),
        }
    }
}

#[derive(Debug)]
struct Derived {
    x: i32,
}

fn main() {
    let p = Plain(3);
    println!("[{}] [{:>8}] [{:?}] [{:>8?}] [{:.2?}] [{:#?}]", p, p, p, p, p, p);
    let n = Name("carcini");
    println!("[{}] [{:>10}] [{:<10}] [{:^11}] [{:*^11}] [{:.3}] [{:>6.2}]", n, n, n, n, n, n, n);
    let w = 12;
    println!("[{:w$}] [{:>1$}]", n, 9);
    println!("{}", Spec);
    println!("{:>7.3}", Spec);
    println!("{:+#08}", Spec);
    println!("{:_<5}", Spec);
    let m = Money(1234.5);
    println!("[{}] [{:.0}] [{:12}] [{:12.3}]", m, m, m, m);
    let d = Derived { x: 4 };
    println!("[{:?}] [{:#?}]", d, d);
}
