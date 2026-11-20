// Debug-build checks around the inline 64-bit lowering (JS numbers, exact to 2^53): wrapping
// methods and `as` casts never panic, shifts by a literal within the width don't, and a plain
// `+` past `u64::MAX` does.
fn main() {
    let k: u64 = 1 << 50;
    println!("{} {}", k.wrapping_add(k), k.wrapping_mul(3));
    let neg: i32 = -1;
    println!("{} {}", neg as i64, (k >> 3) << 3);
    let h: u64 = 0xcbf29ce4;
    println!("{}", h.wrapping_mul(0x1b3) ^ (h << 5));
    let s = k + (k - 1);
    println!("{}", s);
    let top: u64 = 1 << 63;
    let t = top + std::hint::black_box(top);
    println!("{}", t);
}
