#![deny(unused)]
#![warn(non_snake_case)]
fn main() {
    let a = 1;
    let b = 2;
    let BadName = 3;
    let _ = BadName;
    #[allow(unused_variables)]
    let c = 1;
    let X = 2;
    let _ = X;
}
#[warn(dead_code)]
fn f() {}
