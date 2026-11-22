// Operator traits called by path: `Add::add(a, b)`, `<V as Add>::add`, `i32::add`, `V::add` and
// `$trait::$method(..)` from a macro, and as function values (Rust by Example's `macros/dry`).
use std::ops::{Add, Mul, Neg, Not, Sub};

macro_rules! op {
    ($name:ident, $t:ident, $m:ident) => {
        fn $name(a: u32, b: u32) -> u32 { $t::$m(a, b) }
    };
}
op!(apply_add, Add, add);
op!(apply_mul, Mul, mul);

#[derive(Debug, Clone, Copy, PartialEq)]
struct V(i32, i32);
impl Add for V { type Output = V; fn add(self, o: V) -> V { V(self.0 + o.0, self.1 + o.1) } }
impl Neg for V { type Output = V; fn neg(self) -> V { V(-self.0, -self.1) } }

#[derive(Debug, Clone, Copy)]
struct W(i32);
impl Sub for W { type Output = W; fn sub(self, o: W) -> W { W(self.0 - o.0) } }
impl W { fn sub(self, o: W) -> W { W(self.0 * 100 - o.0) } }

fn main() {
    println!("{}", Add::add(1, 2));
    println!("{}", Mul::mul(3.5, 2.0));
    println!("{}", Sub::sub(10u8, 4));
    println!("{} {}", apply_add(5, 6), apply_mul(5, 6));
    println!("{:?}", Add::add(V(1, 2), V(3, 4)));
    println!("{:?}", <V as Add>::add(V(1, 2), V(3, 4)));
    println!("{:?}", V::add(V(1, 2), V(3, 4)));
    println!("{:?}", V::neg(V(1, 2)));
    println!("{:?}", W::sub(W(5), W(1)));
    println!("{:?}", Sub::sub(W(5), W(1)));
    println!("{}", i32::add(1, 2));
    println!("{}", u8::add(200, 55));
    println!("{}", <i32 as Add>::add(1, 2,));
    println!("{}", std::ops::Add::add(1, 2 * 3));
    println!("{}", <i64 as std::ops::Add<i64>>::add(1, 2));
    println!("{} {}", Neg::neg(5), Not::not(true));
    println!("{}", i32::not(5));
    println!("{} {}", PartialEq::eq(&V(1, 2), &V(1, 2)), PartialOrd::lt(&1, &2));
    let f: fn(i32, i32) -> i32 = Add::add;
    println!("{}", f(4, 5));
    println!("{}", vec![1, 2, 3].into_iter().fold(0, Add::add));
    println!("{}", vec![1, 2, 3].into_iter().reduce(i32::add).unwrap());
    println!("{}", vec![1, 2, 3].into_iter().map(i32::neg).fold(1, i32::mul));
    println!("{}", vec![2.0, 4.0].into_iter().fold(1.0, Mul::mul));
}
