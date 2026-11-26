// One generic trait implemented several times for one type (`Scale<f64>`, `Scale<Feet>` and
// `Scale<(i32, i32)>` for `Meters`; `From<Feet>` and `From<Inches>`), called through a type
// parameter, a `where Meters: Scale<R>` clause or `dyn`: the impl is picked by the arguments.
use std::collections::HashMap;
#[derive(Debug, Clone, Copy, PartialEq)]
struct Meters(f64);
struct Feet(f64);
struct Inches(i32);
impl From<Feet> for Meters { fn from(f: Feet) -> Self { Meters(f.0 * 0.3048) } }
impl From<Inches> for Meters { fn from(i: Inches) -> Self { Meters(i.0 as f64 * 0.0254) } }

fn to_m<T: Into<Meters>>(x: T) -> Meters { x.into() }
fn total<T>(xs: Vec<T>) -> f64 where Meters: From<T> { xs.into_iter().map(|x| Meters::from(x).0).sum() }

trait Scale<R> { fn scale(&self, r: R) -> Self; }
impl Scale<f64> for Meters { fn scale(&self, r: f64) -> Self { Meters(self.0 * r) } }
impl Scale<Feet> for Meters { fn scale(&self, r: Feet) -> Self { Meters(self.0 * r.0 * 0.3048) } }
impl Scale<(i32, i32)> for Meters { fn scale(&self, r: (i32, i32)) -> Self { Meters(self.0 * r.0 as f64 / r.1 as f64) } }

fn apply<S: Scale<R>, R>(s: &S, r: R) -> S { s.scale(r) }
fn twice<R: Copy>(m: Meters, r: R) -> Meters where Meters: Scale<R> { m.scale(r).scale(r) }

trait Mix<T> { fn mix(self, t: T) -> String; }
impl Mix<i32> for String { fn mix(self, t: i32) -> String { format!("{}#{}", self, t) } }
impl Mix<char> for String { fn mix(self, t: char) -> String { format!("{}@{}", self, t) } }
fn mixed<T>(s: String, t: T) -> String where String: Mix<T> { s.mix(t) }

fn main_a() {
    println!("{:?}", to_m(Feet(10.0)));
    println!("{:?}", to_m(Inches(100)));
    println!("{:.3}", total(vec![Feet(1.0), Feet(2.0)]));
    println!("{:.3}", total(vec![Inches(1), Inches(2)]));
    let m = Meters(2.0);
    println!("{:?} {:?} {:?}", apply(&m, 3.0), apply(&m, Feet(1.0)), apply(&m, (1, 4)));
    println!("{:?} {:?}", twice(m, 2.0), twice(m, (3, 1)));
    println!("{} {}", mixed("a".to_string(), 1), mixed("b".to_string(), 'z'));
    let v: Vec<Meters> = vec![Feet(1.0).into(), Inches(10).into()];
    println!("{:?}", v);
}
trait Add2<T> { fn add2(&self, t: T) -> i32; }
impl Add2<i32> for i32 { fn add2(&self, t: i32) -> i32 { self + t } }
impl Add2<bool> for i32 { fn add2(&self, t: bool) -> i32 { if t { self + 2 } else { *self } } }
fn a2<T>(x: i32, t: T) -> i32 where i32: Add2<T> { x.add2(t) }
#[derive(Debug, Default)]
struct Inv { items: HashMap<String, u32>, gold: u32 }
struct Gold(u32);
struct Item(&'static str, u32);
trait Add<T> { fn add(&mut self, t: T); }
impl Add<Gold> for Inv { fn add(&mut self, g: Gold) { self.gold += g.0; } }
impl Add<Item> for Inv { fn add(&mut self, i: Item) { *self.items.entry(i.0.to_string()).or_insert(0) += i.1; } }
impl Add<(&'static str, u32, u32)> for Inv { fn add(&mut self, t: (&'static str, u32, u32)) { self.add(Item(t.0, t.1 * t.2)); } }
fn give<A: Add<T>, T>(a: &mut A, t: T) { a.add(t); }
fn give_all<T>(inv: &mut Inv, ts: Vec<T>) where Inv: Add<T> { for t in ts { inv.add(t); } }
fn via_dyn(a: &mut dyn Add<Gold>) { a.add(Gold(1)); }
fn main_b() {
    let mut inv = Inv::default();
    give(&mut inv, Gold(5));
    give(&mut inv, Item("ore", 2));
    give(&mut inv, ("gem", 2, 3));
    give_all(&mut inv, vec![Gold(1), Gold(2)]);
    give_all(&mut inv, vec![Item("ore", 1)]);
    via_dyn(&mut inv);
    let mut v: Vec<_> = inv.items.iter().collect();
    v.sort();
    println!("{} {:?}", inv.gold, v);
}
fn main() {
    main_a();
    println!("{} {}", a2(1, 5), a2(1, true));
    main_b();
}
