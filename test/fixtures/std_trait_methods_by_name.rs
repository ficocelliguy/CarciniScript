// Std trait methods called by name: the operator traits' (`a.add(b)`, `x.shl(3)`, `n.not()`,
// `d.add_assign(e)`), `s.deref()`, `Self::from` as a value, `T::from(x)` of a `T` (std's
// reflexive `From`), `Vec::default()` and `Box::default()`.
use std::ops::{Add, AddAssign, Deref, Mul, Neg, Not, Shl, Sub};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Dt(i64);
#[derive(Clone, Copy, PartialEq, Debug)]
struct Dur(i64);

impl Add<Dur> for Dt {
    type Output = Dt;
    fn add(self, rhs: Dur) -> Dt {
        Dt(self.0 + rhs.0)
    }
}
impl Sub<Dur> for Dt {
    type Output = Dt;
    fn sub(self, rhs: Dur) -> Dt {
        Dt(self.0 - rhs.0)
    }
}
impl AddAssign<Dur> for Dt {
    fn add_assign(&mut self, rhs: Dur) {
        *self = self.add(rhs);
    }
}
impl Neg for Dur {
    type Output = Dur;
    fn neg(self) -> Dur {
        Dur(-self.0)
    }
}
impl From<i64> for Dt {
    fn from(v: i64) -> Dt {
        Dt(v)
    }
}
impl Dt {
    fn many(vs: &[i64]) -> Vec<Dt> {
        vs.iter().copied().map(Self::from).collect()
    }
    fn back(self, d: Dur) -> Dt {
        self.sub(d)
    }
    fn same(&self, o: &Dt) -> bool {
        self.eq(o)
    }
}

// A type with an inherent `rem` of its own keeps it.
struct Bag(Vec<i32>);
impl Bag {
    fn rem(&mut self, x: i32) {
        self.0.retain(|y| *y != x);
    }
}

struct Name(String);
impl Deref for Name {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0
    }
}

fn scale<T: Mul<Output = T> + Copy>(x: T, k: T) -> T {
    x.mul(k)
}

fn main() {
    let mut d = Dt(1);
    d.add_assign(Dur(2));
    d += Dur(1);
    println!("{:?} {:?} {}", d, d.back(Dur(4)), d.same(&Dt(4)));
    println!("{:?} {:?}", Dur(5).neg(), Dt::many(&[1, 2]));
    println!("{:?}", Dt::from(d));

    let x: u32 = 5;
    let n: u8 = 0b1010;
    println!("{} {} {} {}", x.shl(3), n.not(), true.not(), 7i32.sub(10).mul(2));
    println!("{} {}", scale(3, 4), scale(1.5, 2.0));

    let mut b = Bag(vec![1, 2, 3]);
    b.rem(2);
    println!("{:?}", b.0);

    let s = String::from("hello");
    let nm = Name("ann".to_string());
    println!("{} {} {}", s.deref().len(), nm.deref().to_uppercase(), nm.len());

    let w = Duration::from(Duration::from_millis(1500));
    let v: Vec<u8> = Vec::default();
    let bx: Box<Vec<u8>> = Box::default();
    let bn: Box<u32> = Box::default();
    println!("{:?} {:?} {:?} {}", w, v, bx, bn);
}
