// `&mut x` of a number, string, bool or fieldless enum as a `&mut dyn Trait`: a `&mut self`
// method through it writes `x`.
use std::fmt::Display;

trait Turn {
    fn turn(&mut self);
    fn show(&self) -> String;
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Dir {
    N,
    E,
    S,
}

impl Turn for Dir {
    fn turn(&mut self) {
        *self = match self {
            Dir::N => Dir::E,
            Dir::E => Dir::S,
            Dir::S => Dir::N,
        };
    }
    fn show(&self) -> String {
        format!("{:?}", self)
    }
}

impl Turn for u32 {
    fn turn(&mut self) {
        *self += 1;
    }
    fn show(&self) -> String {
        format!("u32 {}", self)
    }
}

impl Turn for f64 {
    fn turn(&mut self) {
        *self *= 1.5;
    }
    fn show(&self) -> String {
        format!("f64 {}", self)
    }
}

impl Turn for String {
    fn turn(&mut self) {
        self.push('!');
    }
    fn show(&self) -> String {
        format!("str {}", self)
    }
}

impl Turn for bool {
    fn turn(&mut self) {
        *self = !*self;
    }
    fn show(&self) -> String {
        format!("bool {}", self)
    }
}

fn spin(t: &mut dyn Turn) {
    t.turn();
}

fn spin_twice(t: &mut dyn Turn) -> String {
    spin(t);
    t.turn();
    t.show()
}

fn shout(d: &mut dyn Display) -> String {
    format!("<{}>", d)
}

fn main() {
    let mut d = Dir::N;
    spin(&mut d);
    println!("{:?}", d);
    println!("{}", spin_twice(&mut d));
    println!("{:?} {}", d, d == Dir::E);

    let mut n = 5u32;
    spin(&mut n);
    let t = spin_twice(&mut n);
    println!("{} {}", n, t);
    println!("{}", n);

    let mut x = 2.0f64;
    spin(&mut x);
    println!("{}", x);
    println!("{}", spin_twice(&mut x));

    let mut s = String::from("hi");
    spin(&mut s);
    let t = spin_twice(&mut s);
    println!("{} {}", s, t);

    let mut b = false;
    spin(&mut b);
    println!("{}", b);

    {
        let r: &mut dyn Turn = &mut d;
        r.turn();
        println!("{}", r.show());
    }
    println!("{:?}", d);

    let mut k = 7u32;
    println!("{}", shout(&mut k));

    let mut ds = [Dir::N, Dir::S];
    spin(&mut ds[1]);
    println!("{:?}", ds);
}
