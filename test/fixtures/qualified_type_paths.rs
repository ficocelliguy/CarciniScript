// Qualified type paths without a trait: `<u8>::MAX`, `<Vec<u8>>::new()`, `<T>::method()` on a
// type parameter, `<Self>::..`, and `<$t>::MAX` inside `macro_rules!`, also in macro arguments.
use std::collections::HashMap;

macro_rules! bounds {
    ($($t:ty),*) => {
        $( println!("{} {} {}", stringify!($t), <$t>::MIN, <$t>::MAX); )*
    };
}

macro_rules! make {
    ($t:ty) => {
        <$t>::default()
    };
}

trait Named {
    fn name() -> String;
}

#[derive(Debug, Default)]
struct Point {
    x: i32,
}

impl Named for Point {
    fn name() -> String {
        "Point".to_string()
    }
}

impl Point {
    fn origin() -> Self {
        <Self>::default()
    }
}

fn name_of<T: Named>() -> String {
    <T>::name()
}

fn main() {
    let a = <u8>::MAX;
    let b = <i32>::max(3, 7);
    let mut v = <Vec<u8>>::new();
    v.push(a);
    let w = <Vec<Vec<i32>>>::with_capacity(2);
    let s = <String>::from("hi");
    let mut m = <HashMap<String, i32>>::new();
    m.insert(s.clone(), 1);
    println!("{} {} {:?} {:?} {} {:?}", a, b, v, w, s, m);
    println!("{} {:?} {:?}", name_of::<Point>(), Point::origin(), make!(Point));
    println!("{} {}", <u16>::MAX as u32 + 1, <i64>::MIN < 0 && 1 < 2);
    println!("{}", <u32>::MAX > 1 && <u32>::MIN < 1);
    let x: i32 = make!(i32);
    let y = 3;
    let z = if x < y && y > x { <f64>::sqrt(16.0) } else { 0.0 };
    println!("{} {}", x, z);
    bounds!(i8, u16, i128);
}
