// `&mut self` methods calling one another on `self`: in a blanket impl (where `self` may be a
// number), through a trait default of a blanket impl, and a default on an immutable enum.
use std::fmt::Debug;

trait Reset {
    fn reset(&mut self);
    fn reset_twice(&mut self) {
        self.reset();
        self.reset();
    }
}

impl<T: Debug + Copy> Reset for T {
    fn reset(&mut self) {
        println!("reset {:?}", self);
        let v = *self;
        *self = v;
    }
}

trait Bump {
    fn bump(&mut self);
    fn bump_both(&mut self);
}

impl<T: Copy + std::ops::Add<Output = T> + From<u8>> Bump for T {
    fn bump(&mut self) {
        *self = *self + T::from(1);
    }
    fn bump_both(&mut self) {
        self.bump();
        self.bump();
    }
}

trait Turn {
    fn turn(&mut self);
    fn turn_twice(&mut self) {
        self.turn();
        self.turn();
    }
}

#[derive(Debug, Clone, Copy)]
enum Dir {
    N,
    E,
    S,
    W,
}

impl Turn for Dir {
    fn turn(&mut self) {
        *self = match self {
            Dir::N => Dir::E,
            Dir::E => Dir::S,
            Dir::S => Dir::W,
            Dir::W => Dir::N,
        };
    }
}

impl Turn for u8 {
    fn turn(&mut self) {
        *self += 1;
    }
}

#[derive(Debug, Clone, Copy)]
struct P(u8, bool);

fn go<T: Turn>(t: &mut T) {
    t.turn_twice();
}

fn bump_it<T: Bump>(t: &mut T) {
    t.bump_both();
}

fn main() {
    let mut p = P(3, true);
    p.reset_twice();
    let mut b: u8 = 7;
    b.reset_twice();
    println!("{:?} {}", p, b);

    let mut n = 1u32;
    n.bump_both();
    bump_it(&mut n);
    let mut f = 0.5f64;
    f.bump_both();
    println!("{} {}", n, f);

    let mut d = Dir::N;
    d.turn_twice();
    println!("{:?}", d);
    go(&mut d);
    println!("{:?}", d);
    let mut x = 1u8;
    x.turn_twice();
    go(&mut x);
    println!("{}", x);
}
