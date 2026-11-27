// An inherent method with the name of a trait-impl one on the same type, the trait impl written
// first: `s.name()` and `S::name(&s)` are the inherent one, `Named::name(&s)`, `<S as
// Named>::name(&s)`, a generic bound, `dyn Named` and the trait's default methods the trait's.
trait Ided {
    const ID: u32;
}

trait Named {
    fn name(&self) -> String;
    fn shout(&self) -> String {
        self.name().to_uppercase()
    }
}

struct S {
    n: u8,
}

impl Ided for S {
    const ID: u32 = 1;
}

impl Named for S {
    fn name(&self) -> String {
        format!("trait {}", self.n)
    }
}

impl S {
    fn name(&self) -> &'static str {
        "inherent"
    }
}

fn generic<T: Named + Ided>(t: &T) -> String {
    format!("{} {}", t.name(), T::ID)
}

fn dynamic(t: &dyn Named) -> String {
    t.name()
}

fn main() {
    let s = S { n: 4 };
    println!("{}", s.name());
    println!("{}", Named::name(&s));
    println!("{}", <S as Named>::name(&s));
    println!("{}", S::name(&s));
    println!("{}", generic(&s));
    println!("{}", dynamic(&s));
    println!("{}", s.shout());
    println!("{}", <S as Ided>::ID);
}
