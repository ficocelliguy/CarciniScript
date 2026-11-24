#![allow(dead_code)]
const maxValue: i32 = 1;
mod BadMod {}
struct S { BadField: i32 }
impl S { fn BadMethod(&self) {} const lowConst: i32 = 1; }
trait T { fn TraitM(&self); }
trait lower_trait {}
fn f<t>(_x: t) {}
fn main() {
    let o: Option<i32> = None;
    match o { x => {}, _ => {} }
    match o { Some(_) => {}, None => {}, _ => {} }
    let w = 5;
    match w { 1 | 2 => {}, 2 => {}, _ => {} }
    let s = S { BadField: 1 };
    let S { BadField } = s;
    let v = vec![1];
    let it = v.iter();
    it.clone().map(|x| x);
    let mut q = 1;
    q += 1;
    println!("{}", q);
}
