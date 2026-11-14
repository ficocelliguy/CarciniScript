// User traits on built-in types ("extension traits"): `impl Money for f64`, `for i32`, `for str`
// (called on `&str` and `String`), `for Vec<T>`, `for bool`; a default method that calls the
// required one on `self`; `&mut self` methods on a `Vec`; and std methods still winning over a
// trait method of the same name.
trait Money {
    fn money(&self) -> String;
    fn money_line(&self, label: &str) -> String {
        format!("{label}: {}", self.money())
    }
}

impl Money for f64 {
    fn money(&self) -> String {
        if *self >= 1e6 {
            format!("${:.2}m", self / 1e6)
        } else if *self >= 1e3 {
            format!("${:.2}k", self / 1e3)
        } else {
            format!("${:.2}", self)
        }
    }
}

trait Squares {
    fn squared(&self) -> Self;
    fn is_square(&self) -> bool;
}

impl Squares for i32 {
    fn squared(&self) -> i32 {
        self * self
    }
    fn is_square(&self) -> bool {
        let r = (*self as f64).sqrt() as i32;
        r * r == *self
    }
}

trait Shout {
    fn shout(&self) -> String;
    fn len(&self) -> usize;
}

impl Shout for str {
    fn shout(&self) -> String {
        format!("{}!", self.to_uppercase())
    }
    fn len(&self) -> usize {
        999
    }
}

trait Stats {
    fn mean(&self) -> f64;
    fn push_twice(&mut self, x: i32);
}

impl Stats for Vec<i32> {
    fn mean(&self) -> f64 {
        if self.is_empty() {
            return 0.0;
        }
        self.iter().sum::<i32>() as f64 / self.len() as f64
    }
    fn push_twice(&mut self, x: i32) {
        self.push(x);
        self.push(x);
    }
}

trait YesNo {
    fn yes_no(&self) -> &'static str;
}

impl YesNo for bool {
    fn yes_no(&self) -> &'static str {
        if *self { "yes" } else { "no" }
    }
}

fn main() {
    let cash: f64 = 1_234_567.0;
    println!("{} {} {}", cash.money(), 950.5f64.money(), (cash / 100.0).money_line("income"));
    let n: i32 = 12;
    println!("{} {} {}", n.squared(), 49i32.is_square(), n.is_square());
    let s = String::from("hack");
    println!("{} {} {}", "grow".shout(), s.shout(), s.len());
    let mut v = vec![1, 2];
    v.push_twice(6);
    println!("{:?} {:.2} {}", v, v.mean(), Vec::<i32>::new().mean());
    println!("{} {}", (3 > 2).yes_no(), v.is_empty().yes_no());
}
