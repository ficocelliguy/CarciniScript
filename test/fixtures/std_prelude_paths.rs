// Full std paths to prelude items, variant paths through the enum, a turbofish on a variant, and
// `size_of` as a value.
use std::mem::size_of;

fn main() {
    let a = std::option::Option::Some(3);
    let n = std::mem::size_of::<u32>();
    let b: std::result::Result<i32, String> = std::result::Result::Ok(1);
    let m = core::cmp::max(1, 2);
    println!("{:?} {} {:?} {}", a, n, b, m);
    let c = Option::Some(3);
    let d: Option<i32> = Option::None;
    let e = Result::<i32, String>::Ok(1);
    let f = size_of::<u32>() + size_of::<i64>() + size_of::<bool>() + size_of::<char>();
    let g = std::mem::size_of_val(&7u64) + std::mem::size_of_val(&1.5f32);
    let h = std::vec::Vec::<i32>::new();
    let i = std::string::String::from("a");
    let j: std::option::Option<i32> = Some(1);
    let k = std::collections::HashMap::<i32, i32>::new();
    println!("{:?} {:?} {:?} {} {} {:?} {} {:?} {}", c, d, e, f, g, h, i, j, k.len());
    let l = Ok::<i32, String>(1);
    let o = None::<i32>;
    let p = Some::<i32>(3);
    let q = Err::<i32, String>("e".into());
    println!("{:?} {:?} {:?} {:?}", l, o, p, q);
    match std::option::Option::Some(2) {
        Option::Some(x) => println!("some {x}"),
        Option::None => println!("none"),
    }
}
