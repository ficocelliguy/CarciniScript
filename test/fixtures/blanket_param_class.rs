// A blanket impl's method with `self` that calls its parameter's associated functions
// (`Z::default()`, zeroize's `impl<Z: DefaultIsZeroes> Zeroize for Z`): the receiver's class,
// handed over by each kind of caller.
trait DefaultIsZeroes: Copy + Default + Sized {}

trait Zeroize {
    fn zeroize(&mut self);
}

impl<Z: DefaultIsZeroes> Zeroize for Z {
    fn zeroize(&mut self) {
        *self = Z::default();
    }
}

#[derive(Clone, Copy, Default, Debug)]
struct P(u8, bool);

impl DefaultIsZeroes for P {}
impl DefaultIsZeroes for u32 {}
impl DefaultIsZeroes for f64 {}
impl DefaultIsZeroes for char {}

trait Zero {
    fn zero() -> Self;
}
impl Zero for u16 {
    fn zero() -> Self {
        100
    }
}
impl Zero for P {
    fn zero() -> Self {
        P(42, true)
    }
}

trait Reset {
    fn reset(&mut self);
}

impl<T: Zero + std::fmt::Debug> Reset for T {
    fn reset(&mut self) {
        println!("reset {:?}", self);
        *self = T::zero();
    }
}

trait Fresh {
    fn fresh(&self) -> Self;
}

impl<T: Default> Fresh for T {
    fn fresh(&self) -> Self {
        Self::default()
    }
}

// (`>>` on the blanket impl's parameter: by the receiver's class, `u32` vs `i32`.)
trait Half {
    fn half(self) -> Self;
}

impl<I> Half for I
where
    I: std::ops::Shr<usize, Output = I>,
{
    fn half(self) -> Self {
        self >> 1usize
    }
}

fn halve<T: Half>(t: T) -> T {
    t.half()
}

fn go<T: Zeroize>(t: &mut T) {
    t.zeroize();
}

fn go_reset<T: Reset>(t: &mut T) {
    t.reset();
}

fn main() {
    let mut p = P(5, true);
    p.zeroize();
    let mut n = 7u32;
    n.zeroize();
    let mut f = 2.5f64;
    f.zeroize();
    let mut c = 'x';
    c.zeroize();
    println!("{:?} {} {} {:?}", p, n, f, c);

    let mut q = P(6, false);
    go(&mut q);
    let mut m = 9u32;
    go(&mut m);
    println!("{:?} {}", q, m);

    let mut r = P(8, true);
    Zeroize::zeroize(&mut r);
    let mut s = P(1, true);
    <P as Zeroize>::zeroize(&mut s);
    let mut v = vec![P(1, true), P(2, false)];
    for x in v.iter_mut() {
        x.zeroize();
    }
    println!("{:?} {:?} {:?}", r, s, v);

    let mut h = 3u16;
    h.reset();
    let mut pp = P(0, false);
    pp.reset();
    let mut hh = 4u16;
    go_reset(&mut hh);
    println!("{} {:?} {}", h, pp, hh);

    println!("{} {} {}", 10u32.half(), (-9i64).half(), 4000000000u32.half());
    println!("{} {}", halve(4000000000u32), halve(-7i32));
    println!("{:?} {} {:?}", P(3, true).fresh(), 5u8.fresh(), String::from("x").fresh());
}
