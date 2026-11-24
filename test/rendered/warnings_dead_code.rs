#[derive(Debug)]
struct S { a: i32, b: i32, c: i32 }
impl S {
    fn new() -> S { S { a: 1, b: 2, c: 3 } }
    fn get(&self) -> i32 { 0 }
}
enum E { A, B(i32), C }
pub fn pubf() {}
struct U { x: i32 }
const K: i32 = 1;
static ST: i32 = 2;
type Alias = i32;
trait Tr { fn m(&self); }
mod inner { pub fn q() {} }
fn main() {
    let s = S::new();
    println!("{:?}", s.a);
    let e = E::A;
    if let E::B(_) = e {}
    let u = U { x: 1 };
}
