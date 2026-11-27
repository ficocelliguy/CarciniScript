// One generic trait implemented several times for one type where the impls differ only by number
// type (`Scale<f64>` / `Scale<i32>`) or only by return type (`Conv<u8>` / `Conv<u16>`), called
// through a type parameter or a `where M: Scale<R>` clause: the call hands the trait argument's
// class, which picks the impl.
trait Scale<R> { fn scale(&self, r: R) -> String; }
struct M(i32);
impl Scale<f64> for M { fn scale(&self, r: f64) -> String { format!("f64 {}", self.0 as f64 * r) } }
impl Scale<i32> for M { fn scale(&self, r: i32) -> String { format!("i32 {}", self.0 * r) } }
fn apply<S: Scale<R>, R>(s: &S, r: R) -> String { s.scale(r) }
fn twice<R: Copy>(m: &M, r: R) -> String where M: Scale<R> { format!("{} {}", m.scale(r), m.scale(r)) }

trait Conv<T> { fn conv(&self) -> T; }
impl Conv<u8> for M { fn conv(&self) -> u8 { self.0 as u8 } }
impl Conv<u16> for M { fn conv(&self) -> u16 { (self.0 * 1000) as u16 } }
fn get<T>(m: &M) -> T where M: Conv<T> { m.conv() }
fn get2<C: Conv<T>, T>(c: &C) -> T { c.conv() }

trait Tag<T> { fn tag(&self, t: T) -> String; }
impl Tag<u8> for String { fn tag(&self, t: u8) -> String { format!("{self}:u8={t}") } }
impl Tag<i64> for String { fn tag(&self, t: i64) -> String { format!("{self}:i64={t}") } }
fn tagged<T>(s: &String, t: T) -> String where String: Tag<T> { s.tag(t) }

fn main() {
    let m = M(3);
    println!("{} | {}", apply(&m, 2.0), apply(&m, 2));
    println!("{} | {}", twice(&m, 1.5), twice(&m, 4i32));
    let a: u8 = get(&m);
    let b: u16 = get(&m);
    println!("{} {}", a, b);
    println!("{} {}", get2::<M, u8>(&m), get2::<_, u16>(&m));
    let s = "x".to_string();
    println!("{} {}", tagged(&s, 7u8), tagged(&s, 7i64));
}
