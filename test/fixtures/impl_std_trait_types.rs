// `impl` / `dyn` of std's formatting and comparison traits in parameter and return types.
use std::fmt::{Debug, Display};

fn show(val: impl Display) -> String {
    format!("<{val}>")
}

fn show_ref(t: &impl Display) -> String {
    t.to_string()
}

fn show_dyn(t: &dyn Display) -> String {
    format!("{}", t)
}

fn pair(a: impl Display, b: impl Display) -> String {
    format!("{a}-{b}")
}

fn dbg_it(x: u8) -> impl Debug {
    (x, "y")
}

fn tos(x: impl ToString) -> String {
    x.to_string()
}

fn largest(v: &[impl Ord + Debug + Copy]) -> String {
    let mut m = v[0];
    for &x in v {
        if x > m {
            m = x;
        }
    }
    format!("{m:?}")
}

struct Pt(i32, i32);

impl Display for Pt {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "({}, {})", self.0, self.1)
    }
}

struct Log {
    lines: Vec<String>,
}

impl Log {
    fn add(&mut self, x: impl Display) {
        self.lines.push(x.to_string());
    }
}

fn main() {
    let items: Vec<Box<dyn Display>> = vec![Box::new(1), Box::new("two"), Box::new(3.5), Box::new(Pt(1, 2))];
    for i in &items {
        print!("{} ", i);
    }
    println!();
    println!("{} {} {} {}", show(5), show_ref(&"s"), show_dyn(&7), show(Pt(3, 4)));
    println!("{} {:?} {} {}", pair(1, "x"), dbg_it(2), tos('c'), largest(&[3, 9, 2]));
    let mut log = Log { lines: vec![] };
    log.add(1.5);
    log.add(Pt(0, 0));
    println!("{:?}", log.lines);
}
