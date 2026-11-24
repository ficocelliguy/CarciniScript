fn f1() -> Result<(), ()> { Ok(()) }
fn main() {
    let mut a = 1;
    let b = 2;
    let mut c = 0;
    c = 5;
    match b { 1 => {}, 1 => {}, _ => {} }
    f1();
    g();
    let BadV = 1;
    println!("{a}{BadV}");
}
fn g() {
    let mut x = 1;
    let y = 3;
    let mut z = vec![1];
    match y { 2 => {}, 2 => {}, _ => {} }
    let w = (y + 1);
    return;
    let k = 3;
}
struct S { a: i32, b: i32, c: i32 }
impl S { fn new() -> S { S { a: 1, b: 2, c: 3 } } fn get(&self) -> i32 { 0 } fn set(&mut self) {} }
enum E { A, B(i32), C }
fn h(e: E) -> i32 { match e { E::A => 1, _ => 2 } }
