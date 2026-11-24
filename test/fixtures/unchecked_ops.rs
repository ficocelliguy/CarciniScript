// `unchecked_*` integer arithmetic (undefined behavior on overflow, so only in-range values
// here), and `env::args_os`.
use std::env;

fn main() {
    let a: i32 = 4;
    let b = unsafe { a.unchecked_add(3) };
    let c = unsafe { b.unchecked_mul(-6) };
    let d: u8 = unsafe { 200u8.unchecked_sub(55) };
    let e: u64 = unsafe { 1u64.unchecked_mul(1 << 40) };
    let f: i128 = unsafe { (1i128 << 100).unchecked_add(7) };
    println!("{} {} {} {} {}", b, c, d, e, f);
    let extra = env::args_os().skip(1).count();
    println!("{}", extra);
    for a in env::args_os().skip(1) {
        println!("{:?}", a);
    }
}
