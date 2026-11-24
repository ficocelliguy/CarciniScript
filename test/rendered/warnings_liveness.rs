#![allow(dead_code)]
struct myStruct;
enum Color { Red, green }
static counter: i32 = 0;
fn BadName(BadParam: i32) -> i32 { BadParam }
fn r() -> Result<i32, String> { Ok(1) }
fn main() {
    let mut a = 1;
    a = 2;
    let b;
    b = 3;
    let mut c = 0;
    c += 1;
    r();
    let v = vec![1];
    v.iter().map(|x| x + 1);
    match a { 1 => {}, 1 => {}, _ => {}, _ => {} }
    return;
    println!("{}", BadName(1));
}
fn other() -> i32 {
    panic!("x");
    let q = 1;
    q
}
