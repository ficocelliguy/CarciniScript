// Typed `self` parameters: `self: &mut Self`, `self: &Self`, `self: &'a Self`, `mut self: Self`.
struct S { v: Vec<u8> }
impl S {
    fn init(self: &mut Self, x: u8) { self.v.push(x); }
    fn get(self: &Self) -> usize { self.v.len() }
    fn first<'a>(self: &'a Self) -> Option<&'a u8> { self.v.first() }
    fn into_v(mut self: Self) -> Vec<u8> { self.v.push(0); self.v }
}
fn main() {
    let mut s = S { v: vec![] };
    s.init(3);
    s.init(4);
    println!("{} {:?}", s.get(), s.first());
    println!("{:?}", s.into_v());
}
