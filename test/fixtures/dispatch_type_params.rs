// Dispatch through type parameters and associated types: `I::Item` of an impl's `I: Iterator`
// (inline or in `where`), `T::CONST` / `T::f()` in methods with `self`, `Self::Value` in a trait
// method's `where`, a `<T as Tr>::X: Bound` predicate, methods of a bound on an associated type,
// and a method on a type parameter inside a macro expansion.

// `I::Item` of an impl's iterator parameter.
struct Doubled<I>(I);
impl<I: Iterator> Iterator for Doubled<I> {
    type Item = (I::Item, u8);
    fn next(&mut self) -> Option<(I::Item, u8)> {
        self.0.next().map(|x| (x, 2))
    }
}

// `where L: Iterator, R: Iterator<Item = L::Item>`, through a macro.
enum Either<L, R> {
    Left(L),
    Right(R),
}
macro_rules! for_both {
    ($value:expr, $pattern:pat => $result:expr) => {
        match $value {
            Either::Left($pattern) => $result,
            Either::Right($pattern) => $result,
        }
    };
}
impl<L, R> Iterator for Either<L, R>
where
    L: Iterator,
    R: Iterator<Item = L::Item>,
{
    type Item = L::Item;
    fn next(&mut self) -> Option<Self::Item> {
        for_both!(self, inner => inner.next())
    }
}

// `T::ZERO` and `T::zero()` in an impl method with `self`.
trait Zero {
    const ZERO: Self;
    fn zero() -> Self;
    fn bump(&mut self);
}
impl Zero for u32 {
    const ZERO: u32 = 0;
    fn zero() -> u32 {
        0
    }
    fn bump(&mut self) {
        *self += 1;
    }
}
impl Zero for f64 {
    const ZERO: f64 = 0.0;
    fn zero() -> f64 {
        0.0
    }
    fn bump(&mut self) {
        *self += 0.5;
    }
}
struct Acc<T> {
    v: T,
}
impl<T: Zero + Copy + std::fmt::Debug> Acc<T> {
    fn reset(&mut self) {
        self.v = T::ZERO;
    }
    fn fresh(&self) -> T {
        let mut z = T::zero();
        z.bump();
        z
    }
}
fn zero_of<T: Zero>() -> T {
    T::ZERO
}

// `Self::Value` in a provided method's `where`; `<T as Factory>::Parser: Typed<Value = T>`.
trait Typed {
    type Value;
    fn get(&self) -> Self::Value;
    fn map_with<F, U>(self, f: F) -> U
    where
        Self: Sized,
        F: Fn(Self::Value) -> U,
    {
        f(self.get())
    }
}
trait Factory {
    type Parser;
    fn parser() -> Self::Parser;
}
struct Num(i32);
struct NumParser;
impl Typed for NumParser {
    type Value = Num;
    fn get(&self) -> Num {
        Num(42)
    }
}
impl Factory for Num {
    type Parser = NumParser;
    fn parser() -> NumParser {
        NumParser
    }
}
struct Wrapped<T>(T);
impl<T> Wrapped<T>
where
    T: Factory,
    <T as Factory>::Parser: Typed<Value = T>,
{
    fn make() -> Wrapped<T> {
        T::parser().map_with(Wrapped)
    }
}

// Methods of a bound on an associated type.
trait Config {
    fn padding(&self) -> bool;
}
trait Engine {
    type Config: Config;
    fn config(&self) -> &Self::Config;
    fn encode(&self, x: u8) -> u8;
}
struct Pad(bool);
impl Config for Pad {
    fn padding(&self) -> bool {
        self.0
    }
}
struct Std {
    cfg: Pad,
}
impl Engine for Std {
    type Config = Pad;
    fn config(&self) -> &Pad {
        &self.cfg
    }
    fn encode(&self, x: u8) -> u8 {
        x + 1
    }
}
fn pads<E: Engine>(e: &E) -> bool {
    e.config().padding()
}
fn pads2<E: Engine>(e: &E, c: &E::Config) -> bool {
    c.padding() && e.encode(1) == 2
}

fn main() {
    let d: Vec<(char, u8)> = Doubled("ab".chars()).collect();
    println!("{d:?}");
    let l: Either<std::vec::IntoIter<i32>, std::ops::Range<i32>> = Either::Left(vec![7, 8].into_iter());
    let r: Either<std::vec::IntoIter<i32>, std::ops::Range<i32>> = Either::Right(3..5);
    println!("{:?} {:?}", l.collect::<Vec<_>>(), r.collect::<Vec<_>>());

    let mut a = Acc { v: 5u32 };
    let f = a.fresh();
    a.reset();
    println!("{:?} {:?}", a.v, f);
    let mut b = Acc { v: 2.5f64 };
    let g = b.fresh();
    b.reset();
    println!("{:?} {:?} {} {}", b.v, g, zero_of::<u32>(), zero_of::<f64>());

    let w = Wrapped::<Num>::make();
    println!("{}", (w.0).0);

    let e = Std { cfg: Pad(true) };
    println!("{} {}", pads(&e), pads2(&e, &Pad(false)));
}
