// An inherent `const` with the name of a trait-impl `const` on the same type: `S::ID` and
// `Self::ID` (even inside the trait impl) are the inherent one; `<S as Tr>::ID`, a generic
// `T::ID` and the trait's default methods read the trait's.
trait Tr {
    const ID: u32;
    const NAME: &'static str = "tr";
    fn id_of(&self) -> u32 {
        Self::ID * 10
    }
    fn both(&self) -> u32;
}

struct S;

impl Tr for S {
    const ID: u32 = 7;
    fn both(&self) -> u32 {
        // (`Self::ID` here is the inherent constant, as in rustc.)
        Self::ID + <Self as Tr>::ID * 100
    }
}

impl S {
    const ID: u32 = 3;
    const NAME: &'static str = "inherent";
    fn inherent_id(&self) -> u32 {
        Self::ID
    }
}

struct U;

impl U {
    const ID: u32 = 1;
}

impl Tr for U {
    const ID: u32 = 2;
    fn both(&self) -> u32 {
        U::ID + <U as Tr>::ID
    }
}

fn g<T: Tr>(t: &T) -> u32 {
    T::ID + t.id_of()
}

fn name<T: Tr>() -> &'static str {
    T::NAME
}

fn main() {
    println!("{} {} {}", S::ID, <S as Tr>::ID, g(&S));
    println!("{} {}", S.inherent_id(), S.both());
    println!("{} {}", S::NAME, name::<S>());
    println!("{} {} {} {}", U::ID, <U as Tr>::ID, g(&U), U.both());
    println!("{}", S.id_of());
    match 3 {
        S::ID => println!("inherent pattern"),
        _ => println!("other"),
    }
}
