// `unsafe` blocks and `unsafe fn` (ordinary code in JS), `static mut` state changed through them,
// and `unsafe impl Send` for a marker trait.
static mut COUNTER: u32 = 0;
static NAMES: [&str; 2] = ["a", "b"];

unsafe fn bump(by: u32) -> u32 {
    unsafe {
        COUNTER += by;
        COUNTER
    }
}

fn get() -> u32 {
    unsafe { COUNTER }
}

trait Marker {}
struct S;
unsafe impl Send for S {}

fn main() {
    let a = unsafe { bump(2) };
    let b = unsafe { bump(3) };
    println!("{} {} {} {}", a, b, get(), NAMES[1]);
}
