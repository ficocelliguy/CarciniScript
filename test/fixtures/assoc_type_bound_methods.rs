// Methods from the bounds a trait declares on its associated type (`type P: Parser;`) on a
// `T::P` value: a parameter, `t.get()`, `T::make()` (whose `Self::P` is `T::P`), and with
// `where <T as Fac>::P: Parser`; and `type P = T::P;` in a generic impl.
trait Parser { fn parse(&self, s: &str) -> i32; }
struct Len;
impl Parser for Len { fn parse(&self, s: &str) -> i32 { s.len() as i32 } }
struct Count(char);
impl Parser for Count { fn parse(&self, s: &str) -> i32 { s.matches(self.0).count() as i32 } }
trait Fac { type P: Parser; fn make() -> Self::P; fn get(&self) -> Self::P; }
struct LenFac;
impl Fac for LenFac { type P = Len; fn make() -> Len { Len } fn get(&self) -> Len { Len } }
struct CountFac(char);
impl Fac for CountFac { type P = Count; fn make() -> Count { Count('a') } fn get(&self) -> Count { Count(self.0) } }
struct Wrap<T>(T);
impl<T: Fac> Fac for Wrap<T> {
    type P = T::P;
    fn make() -> Self::P { T::make() }
    fn get(&self) -> Self::P { self.0.get() }
}
fn by_param<T: Fac>(p: T::P, s: &str) -> i32 { p.parse(s) }
fn by_get<T: Fac>(t: &T, s: &str) -> i32 { t.get().parse(s) }
fn by_make<T: Fac>(s: &str) -> i32 { T::make().parse(s) }
fn by_let<T: Fac>(s: &str) -> i32 { let p: T::P = T::make(); p.parse(s) + p.parse("x") }
fn by_where<T: Fac>(s: &str) -> i32 where <T as Fac>::P: Parser { let p = T::make(); p.parse(s) }
fn main() {
    println!("{} {}", by_param::<LenFac>(Len, "abc"), by_get(&CountFac('b'), "abba"));
    println!("{} {}", by_make::<CountFac>("banana"), by_let::<LenFac>("ab"));
    println!("{} {}", by_where::<LenFac>("abcd"), by_get(&Wrap(CountFac('n')), "banana"));
    println!("{}", Wrap::<LenFac>::make().parse("hello"));
}
