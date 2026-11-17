// Debug-build checks around the inline 64-bit lowering: wrapping methods and `as` casts never
// panic, shifts by a literal within the width don't, and a plain `+` past `u64::MAX` does.
fn main() {
    let k: u64 = 1 << 63;
    println!("{} {}", k.wrapping_add(k), k.wrapping_mul(3));
    let neg: i32 = -1;
    println!("{} {}", neg as u64, (k >> 3) << 3);
    let h: u64 = 0xcbf29ce484222325;
    println!("{}", h.wrapping_mul(0x100000001b3) ^ (h << 5));
    let s = k + (k - 1);
    println!("{}", s);
    let t = s + 1;
    println!("{}", t);
}
