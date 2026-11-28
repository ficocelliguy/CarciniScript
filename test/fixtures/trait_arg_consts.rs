// A generic trait's associated consts per trait argument, as in time-core's `MultipleOf`: one
// const per impl (`VALUE` for `<Nano, u16>`, `<Nano, u128>`, `<Nano, f64>`), picked by the trait
// arguments of `<Micro as MultipleOf<Nano, f64>>::VALUE`, and through a type parameter by its
// bound's (`T: MultipleOf<Nano, O>`, also beside `+ Copy`).
#[derive(Clone, Copy)]
pub struct Nano;
#[derive(Clone, Copy)]
pub struct Micro;

mod sealed {
    pub trait MultipleOf<T, Output> {
        const VALUE: Output;
    }
}
use sealed::MultipleOf;

impl MultipleOf<Nano, u16> for Micro {
    const VALUE: u16 = 1_000;
}
impl MultipleOf<Nano, u128> for Micro {
    const VALUE: u128 = 1_000;
}
impl MultipleOf<Nano, f64> for Micro {
    const VALUE: f64 = 1_000.5;
}
impl MultipleOf<Nano, u16> for Nano {
    const VALUE: u16 = 1;
}
impl MultipleOf<Nano, f64> for Nano {
    const VALUE: f64 = 1.;
}

macro_rules! show {
    ($e:expr) => {
        println!("{:?}", $e)
    };
}

fn value_of<T: MultipleOf<Nano, O>, O>() -> O {
    T::VALUE
}

const fn multiple_of_value<T, U, Output>(_: T) -> Output
where
    T: MultipleOf<U, Output> + Copy,
{
    T::VALUE
}

fn main() {
    let a: u16 = <Micro as MultipleOf<Nano, u16>>::VALUE;
    let b: u128 = <Micro as MultipleOf<Nano, u128>>::VALUE;
    let c: f64 = <Micro as MultipleOf<Nano, f64>>::VALUE;
    println!("{} {} {}", a, b, c);
    println!("{}", <Micro as MultipleOf<Nano, u128>>::VALUE + 1);
    show!(<Micro as MultipleOf<Nano, f64>>::VALUE);
    show!(<Nano as MultipleOf<Nano, f64>>::VALUE + 0.5);

    let d: f64 = value_of::<Micro, f64>();
    let e: u16 = value_of::<Micro, u16>();
    let f: u128 = value_of::<Micro, u128>();
    println!("{} {} {}", d, e, f);
    let g: f64 = value_of::<Nano, f64>();
    println!("{}", g);

    let h: f64 = multiple_of_value::<_, Nano, _>(Micro);
    let i: u128 = multiple_of_value::<_, Nano, _>(Micro);
    let j: u16 = multiple_of_value::<_, Nano, _>(Nano);
    println!("{} {} {}", h, i, j);
}
