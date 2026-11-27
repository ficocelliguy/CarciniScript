#[must_use]
fn unit() {}
#[deprecated]
struct Old { a: u8 }
#[deprecated]
struct Tup(u8);
#[deprecated]
fn dep() -> u8 { 0 }
#[deprecated]
fn dep_user() -> u8 { dep() }
impl Old { fn get(&self) -> u8 { self.a } }
fn takes(o: &Old) -> u8 { o.get() }
#[deprecated]
enum DE { A }
#[deprecated]
const C: u8 = 1;
mod m { #[deprecated] pub fn inner() {} }
fn main() {
    unit();
    let o = Old { a: 1 };
    let t = Tup(2);
    let _ = (takes(&o), t.0, DE::A as u8, C);
    let f = dep;
    let _ = f;
    m::inner();
}
