// Your traits on tuples: concrete (`impl Area for (f64, f64)`) and generic (`impl<A: Encode, B:
// Encode> Encode for (A, B)`) impls, nested tuples, default methods, and calls through a generic
// `T: Tr` (a tuple is a JS array; its impl is picked by length).
use std::fmt::Write;

trait Encode {
    fn encode(&self, out: &mut String);
    fn encoded(&self) -> String {
        let mut s = String::new();
        self.encode(&mut s);
        s
    }
}

impl Encode for u8 {
    fn encode(&self, out: &mut String) {
        write!(out, "u{}", self).unwrap();
    }
}

impl Encode for i64 {
    fn encode(&self, out: &mut String) {
        write!(out, "i{}", self).unwrap();
    }
}

impl Encode for String {
    fn encode(&self, out: &mut String) {
        write!(out, "s{}:{}", self.len(), self).unwrap();
    }
}

impl<A: Encode, B: Encode> Encode for (A, B) {
    fn encode(&self, out: &mut String) {
        out.push('(');
        self.0.encode(out);
        out.push(',');
        self.1.encode(out);
        out.push(')');
    }
}

impl<A: Encode, B: Encode, C: Encode> Encode for (A, B, C) {
    fn encode(&self, out: &mut String) {
        out.push('<');
        self.0.encode(out);
        self.1.encode(out);
        self.2.encode(out);
        out.push('>');
    }
}

trait Area {
    fn area(&self) -> f64;
}

impl Area for (f64, f64) {
    fn area(&self) -> f64 {
        self.0 * self.1
    }
}

fn twice<T: Encode>(x: &T) -> String {
    format!("{}{}", x.encoded(), x.encoded())
}

fn total_area<T: Area>(xs: &[T]) -> f64 {
    xs.iter().map(|x| x.area()).sum()
}

fn main() {
    println!("{}", (3u8, 4i64).encoded());
    println!("{}", (String::from("ab"), 9u8, -2i64).encoded());
    println!("{}", twice(&(1u8, String::from("x"))));
    println!("{}", twice(&((1u8, 2u8), 7i64)));
    println!("{}", (2.0, 3.5).area());
    println!("{}", total_area(&[(1.0, 2.0), (3.0, 4.0)]));
    let pairs = vec![(1u8, 2u8), (3, 4)];
    let all: Vec<String> = pairs.iter().map(|p| p.encoded()).collect();
    println!("{}", all.join(" "));
}
