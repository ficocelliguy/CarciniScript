// Operator overloading with `std::ops`: `Add`/`Sub` between vectors, `Mul<f64>` and `Mul<Vec2>`
// on one type (picked by the right operand), `Neg`, `Not`, `AddAssign`/`SubAssign` changing the
// left side in place, `-> Self::Output`, `mut self` methods that must not change a `Copy` operand,
// and overloaded operators in chains, closures and folds.
use std::ops::{Add, AddAssign, Mul, Neg, Not, Sub, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vec2 {
    x: f64,
    y: f64,
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x + o.x, y: self.y + o.y }
    }
}

impl Sub for Vec2 {
    type Output = Self;
    // Changes its own copy of the left operand.
    fn sub(mut self, o: Self) -> Self::Output {
        self.x -= o.x;
        self.y -= o.y;
        self
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2 { x: self.x * k, y: self.y * k }
    }
}

// Dot product.
impl Mul<Vec2> for Vec2 {
    type Output = f64;
    fn mul(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2 { x: -self.x, y: -self.y }
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl SubAssign<f64> for Vec2 {
    fn sub_assign(&mut self, k: f64) {
        self.x -= k;
        self.y -= k;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Light {
    On,
    Off,
}

impl Not for Light {
    type Output = Light;
    fn not(self) -> Light {
        match self {
            Light::On => Light::Off,
            Light::Off => Light::On,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Money(u64);

impl Add for Money {
    type Output = Money;
    fn add(self, o: Money) -> Money {
        Money(self.0 + o.0)
    }
}

fn main() {
    let a = Vec2 { x: 1.0, y: 2.0 };
    let b = Vec2 { x: 3.0, y: -1.0 };
    let c = a + b;
    let d = a - b;
    println!("{:?} {:?} a unchanged: {:?}", c, d, a);
    println!("{:?} {} {:?}", a * 2.0, a * b, -a);
    println!("{:?}", a + b * 0.5 - -a);

    let mut p = Vec2 { x: 0.0, y: 0.0 };
    for _ in 0..3 {
        p += a;
    }
    p -= 0.5;
    println!("{:?} a still {:?}", p, a);

    let path = vec![a, b, c];
    let total = path.iter().fold(Vec2 { x: 0.0, y: 0.0 }, |acc, v| acc + *v);
    let scaled: Vec<Vec2> = path.iter().map(|v| *v * 10.0).collect();
    println!("{:?} {:?}", total, scaled);

    let l = Light::On;
    println!("{:?} {:?} {}", !l, !!l, !l == Light::Off);

    let wallet = Money(5) + Money(7) + Money(30);
    println!("{:?}", wallet);
}
