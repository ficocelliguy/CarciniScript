// Blanket impls (`impl<T: Bound> Tr for T`): extension traits on iterators, `?Sized` bounds,
// a method returning the impl's `T`, associated functions (`i32::zeroed()`, `T::zeroed()` in a
// generic function, `Zeroed::zeroed()` by the expected type), and trait-qualified calls on
// built-in types.
use std::fmt::{Debug, Display};

// An iterator extension trait, with `Self::Item` and default methods over `self`.
trait Stats: Iterator {
    fn every_other(self) -> Vec<Self::Item>
    where
        Self: Sized;
    fn mean(self) -> f64
    where
        Self: Sized,
        Self::Item: Into<f64>,
    {
        let (mut n, mut s) = (0, 0.0);
        for x in self {
            n += 1;
            s += x.into();
        }
        if n == 0 { 0.0 } else { s / n as f64 }
    }
    fn tally(self) -> usize
    where
        Self: Sized,
    {
        self.count()
    }
}
impl<I: Iterator> Stats for I {
    fn every_other(self) -> Vec<I::Item> {
        self.step_by(2).collect()
    }
}

trait SumSq {
    fn sum_sq(self) -> u64;
}
impl<I: Iterator<Item = u32>> SumSq for I {
    fn sum_sq(self) -> u64 {
        self.map(|x| (x as u64) * (x as u64)).sum()
    }
}

// `?Sized`: also for `str`, and for your types through generics.
trait Shout {
    fn shout(&self) -> String;
}
impl<T: Display + ?Sized> Shout for T {
    fn shout(&self) -> String {
        format!("{}!", self).to_uppercase()
    }
}
struct P(i32, i32);
impl Display for P {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "p({}, {})", self.0, self.1)
    }
}
fn loud<T: Shout + ?Sized>(x: &T) -> String {
    x.shout()
}

// One user trait over another; the method returns the impl's `T`.
trait Source {
    type Item;
    fn items(&self) -> Vec<Self::Item>;
}
trait IntoSource {
    type Out: Source;
    fn into_source(self) -> Self::Out;
}
impl<T: Source> IntoSource for T {
    type Out = T;
    fn into_source(self) -> T {
        self
    }
}
struct Nums(Vec<i32>);
impl Source for Nums {
    type Item = i32;
    fn items(&self) -> Vec<i32> {
        self.0.clone()
    }
}
trait Area {
    fn area(&self) -> f64;
}
trait Describe {
    fn describe(&self) -> String;
}
impl<T: Area> Describe for T {
    fn describe(&self) -> String {
        format!("area {:.1}", self.area())
    }
}
struct Sq(f64);
impl Area for Sq {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}
struct Own;
impl Area for Own {
    fn area(&self) -> f64 {
        1.0
    }
}
impl Own {
    // (An inherent method beats the blanket impl's.)
    fn describe(&self) -> String {
        "own".into()
    }
}
fn all<T: Describe>(xs: &[T]) -> Vec<String> {
    xs.iter().map(|x| x.describe()).collect()
}

// Associated functions only the blanket impl gives.
trait Zeroed: Sized {
    fn zeroed() -> Self;
    fn zeroed_n(n: usize) -> Vec<Self> {
        (0..n).map(|_| Self::zeroed()).collect()
    }
    fn pair() -> (Self, Self) {
        (Self::zeroed(), Self::zeroed())
    }
}
impl<T: Default> Zeroed for T {
    fn zeroed() -> Self {
        T::default()
    }
}
#[derive(Debug, Default, PartialEq)]
struct Cfg {
    level: u8,
    name: String,
}
fn fresh<T: Zeroed + Debug>() -> String {
    format!("{:?}", T::zeroed_n(2))
}

// A blanket method with its own type parameter.
trait Tagged {
    fn tag<U: Display>(&self, u: U) -> String;
}
impl<T: Debug + ?Sized> Tagged for T {
    fn tag<U: Display>(&self, u: U) -> String {
        format!("{u}={self:?}")
    }
}

// Your impls on built-in types, called through the trait.
trait Make {
    fn make() -> Self;
    fn twice() -> (Self, Self)
    where
        Self: Sized,
    {
        (Self::make(), Self::make())
    }
}
impl Make for i32 {
    fn make() -> Self {
        3
    }
}
impl Make for String {
    fn make() -> Self {
        "s".to_string()
    }
}
trait Inc {
    fn inc(&mut self);
}
impl Inc for i32 {
    fn inc(&mut self) {
        *self += 1;
    }
}

fn main() {
    let v = vec![1i64, 2, 3, 4, 5];
    println!("{:?} {:?}", (1..=7).every_other(), v.iter().map(|x| x * 10).every_other());
    println!("{} {}", vec![1u8, 2, 3, 4].into_iter().mean(), [2.5f32, 3.5].iter().copied().mean());
    println!("{} {}", (1..=10).filter(|x| x % 3 == 0).tally(), "a b c".split(' ').tally());
    println!("{} {}", [1u32, 2, 3].into_iter().sum_sq(), (1..4u32).rev().sum_sq());

    println!("{} {} {}", "hi".shout(), 5.shout(), P(1, 2).shout());
    println!("{} {} {}", loud("yo"), loud(&2.5), loud(&P(3, 4)));
    println!("{:?}", Nums(vec![3, 4]).into_source().items());
    println!("{} {} {:?}", Sq(3.0).describe(), Own.describe(), all(&[Sq(1.0), Sq(2.0)]));

    println!("{} {:?} {}", i64::zeroed(), String::zeroed(), bool::zeroed());
    println!("{:?} {:?} {:?}", Vec::<u8>::zeroed(), Cfg::zeroed(), <Option<i8> as Zeroed>::zeroed());
    println!("{} {} {}", fresh::<u32>(), fresh::<String>(), fresh::<Cfg>());
    let zs: Vec<f64> = Zeroed::zeroed_n(3);
    let c: Cfg = Zeroed::zeroed();
    println!("{zs:?} {} {:?} {:?}", c == Cfg::default(), <Cfg as Zeroed>::pair(), u8::pair());
    println!("{} {} {}", 5.tag("five"), "s".tag(1), Cfg { level: 2, name: "x".into() }.tag('c'));

    let a = i32::make();
    let b: i32 = Make::make();
    let (s1, s2) = <String as Make>::twice();
    let t: (i32, i32) = Make::twice();
    let mut n = 1;
    Inc::inc(&mut n);
    <i32 as Inc>::inc(&mut n);
    println!("{a} {b} {s1}{s2} {t:?} {n}");
    println!("{} {} {}", Shout::shout(&7), <str as Shout>::shout("x"), Describe::describe(&Sq(2.0)));
}
