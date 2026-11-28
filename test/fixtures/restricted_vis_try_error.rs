// `pub(in crate::a)` / `pub(in crate::a::b)` naming enclosing modules of this file (used from
// inside them), and `T::Error` of a `T: TryFrom<X>` printed or unwrapped where a `where
// T::Error: Debug`, a `<T as TryFrom<X>>::Error: Debug` or an `Error = E` binding allows it.
use std::convert::TryFrom;
use std::fmt::Debug;

mod a {
    pub mod b {
        pub(in crate::a) fn f() -> u8 {
            1
        }
        pub(in crate::a::b) fn g() -> u8 {
            2
        }
        pub fn both() -> u8 {
            f() + g()
        }
    }
    pub fn h() -> u8 {
        b::f() * 10 + b::both()
    }
}

#[derive(Debug)]
struct Negative(i64);

struct Small(u8);

impl TryFrom<i64> for Small {
    type Error = Negative;
    fn try_from(x: i64) -> Result<Small, Negative> {
        if x >= 0 { Ok(Small(x as u8)) } else { Err(Negative(x)) }
    }
}

fn conv<T: TryFrom<i64>>(x: i64) -> Option<T>
where
    T::Error: Debug,
{
    match T::try_from(x) {
        Ok(v) => Some(v),
        Err(e) => {
            println!("err {:?}", e);
            None
        }
    }
}

fn conv_qualified<T>(x: i64) -> T
where
    T: TryFrom<i64>,
    <T as TryFrom<i64>>::Error: Debug,
{
    T::try_from(x).unwrap()
}

fn bound_error<T: TryFrom<i64, Error = Negative>>(x: i64) -> T {
    T::try_from(x).expect("non-negative")
}

fn show<E: Debug>(r: Result<u8, E>) -> u8 {
    r.unwrap()
}

fn main() {
    println!("{}", a::h());
    let x: Option<u8> = conv(300);
    let y: Option<u8> = conv(30);
    let z: Option<Small> = conv(-2);
    println!("{:?} {:?} {}", x, y, z.is_none());
    let c: i16 = conv_qualified(-5);
    println!("{c}");
    let s: Small = bound_error(9);
    println!("{}", s.0);
    println!("{}", show::<String>(Ok(4)));
}
