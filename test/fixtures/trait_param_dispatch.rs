// Type parameters asked for their class inside traits and their impls: a trait default method's
// own (`fn visit<E: Kind>(&self)`, also called through a bound), `Self::f()` in an associated
// function of `impl<T> S<T>`, `T::f()` / `<T as Tr>::f()` in `impl<T> Tr for Vec<T>`, a
// `fn name()` of your impl for a number, an impl for `String` with consts, and methods of a
// bound on an associated type reached from a default method.

trait Kind {
    const SIZE: usize;
    fn kind() -> &'static str;
}
impl Kind for u8 {
    const SIZE: usize = 1;
    fn kind() -> &'static str {
        "u8"
    }
}
impl Kind for String {
    const SIZE: usize = 24;
    fn kind() -> &'static str {
        "string"
    }
}

// A default method's own type parameter.
trait Visit {
    fn label(&self) -> String;
    fn visit<E: Kind>(&self) -> String {
        format!("{} as {} ({})", self.label(), E::kind(), E::SIZE)
    }
    fn pair<B: Default>(&self) -> (B, B) {
        (B::default(), B::default())
    }
}
struct Plain;
impl Visit for Plain {
    fn label(&self) -> String {
        "plain".to_string()
    }
}
struct Tagged<T>(T);
impl<T: Kind> Visit for Tagged<T> {
    fn label(&self) -> String {
        format!("tagged<{}>", T::kind())
    }
}
struct Own;
impl Visit for Own {
    fn label(&self) -> String {
        "own".to_string()
    }
    fn visit<X: Kind>(&self) -> String {
        format!("own visit of {}", X::kind())
    }
}
fn visit_u8<V: Visit>(v: &V) -> String {
    v.visit::<u8>()
}

// `Self::f()` in an associated function, and `name()` on a number's impl.
trait Schema {
    fn name() -> String;
    fn declaration() -> String;
}
impl Schema for u8 {
    fn name() -> String {
        "byte".to_string()
    }
    fn declaration() -> String {
        Self::name()
    }
}
struct List<T>(Vec<T>);
impl<T: Schema> Schema for List<T> {
    fn name() -> String {
        format!("List<{}>", T::name())
    }
    fn declaration() -> String {
        format!("{} = [{}]", Self::name(), T::declaration())
    }
}
impl<T: Schema> List<T> {
    fn describe(&self) -> String {
        format!("{} x{}", T::name(), self.0.len())
    }
}

// `T::f()` and `<T as Tr>::f()` in an associated function of an impl for `Vec<T>`.
trait Decode: Sized {
    fn decode(data: &mut &[u8]) -> Self;
}
impl Decode for u8 {
    fn decode(data: &mut &[u8]) -> u8 {
        let b = data[0];
        *data = &data[1..];
        b
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode(data: &mut &[u8]) -> Vec<T> {
        let n = <u8 as Decode>::decode(data);
        (0..n).map(|_| <T as Decode>::decode(data)).collect()
    }
}
fn decode<T: Decode>(data: &mut &[u8]) -> T {
    T::decode(data)
}

// Methods of a bound on an associated type, from a default method and through `T`.
trait MoveMask: Copy {
    fn has_non_zero(self) -> bool;
}
trait Vector: Copy {
    type Mask: MoveMask;
    fn movemask(self) -> Self::Mask;
    fn any_set(self) -> bool {
        self.movemask().has_non_zero()
    }
}
#[derive(Clone, Copy)]
struct Mask(u32);
impl MoveMask for Mask {
    fn has_non_zero(self) -> bool {
        self.0 != 0
    }
}
#[derive(Clone, Copy)]
struct Lanes(u32);
impl Vector for Lanes {
    type Mask = Mask;
    fn movemask(self) -> Mask {
        Mask(self.0 & 1)
    }
}
fn any_set_of<T: Vector>(v: T) -> bool {
    v.movemask().has_non_zero()
}
trait Bound: Copy {
    fn as_i64(self) -> i64;
    fn from_i64(n: i64) -> Self;
}
impl Bound for u8 {
    fn as_i64(self) -> i64 {
        self as i64
    }
    fn from_i64(n: i64) -> u8 {
        n as u8
    }
}
trait Bounds {
    type Primitive: Bound;
    const MIN: Self::Primitive;
    const MAX: Self::Primitive;
    fn check(n: i64) -> Option<Self::Primitive> {
        if Self::MIN.as_i64() <= n && n <= Self::MAX.as_i64() {
            Some(Self::Primitive::from_i64(n))
        } else {
            None
        }
    }
}
struct Digit;
impl Bounds for Digit {
    type Primitive = u8;
    const MIN: u8 = 0;
    const MAX: u8 = 9;
}

fn main() {
    println!("{} | {}", Plain.visit::<u8>(), Plain.visit::<String>());
    println!("{} | {}", Tagged(1u8).visit::<String>(), Own.visit::<String>());
    println!("{} | {} | {}", visit_u8(&Plain), visit_u8(&Own), visit_u8(&Tagged(String::new())));
    let (a, b): (Vec<u8>, String) = (Plain.pair::<Vec<u8>>().0, Plain.pair::<String>().1);
    println!("{:?} {:?}", a, b);

    println!("{} | {}", u8::declaration(), List::<u8>::declaration());
    println!("{}", List(vec![1u8, 2, 3]).describe());

    let bytes = [2u8, 7, 9, 4];
    let mut rest = &bytes[..];
    let v: Vec<u8> = decode(&mut rest);
    let last: u8 = decode(&mut rest);
    println!("{:?} {}", v, last);

    println!("{} {} {}", Lanes(3).any_set(), Lanes(2).any_set(), any_set_of(Lanes(1)));
    println!("{:?} {:?}", Digit::check(5), Digit::check(12));
}
