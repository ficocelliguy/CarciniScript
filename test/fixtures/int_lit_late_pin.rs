// Unsuffixed integer locals stay open through methods every integer type has (`to_string`,
// `clone`, comparisons), so a later use pins them, as in rustc.
fn main() {
    // A later `usize` use pins `c` after `to_string` on it and on its range's items.
    let c = 10;
    let s = c.to_string();
    println!("{}", s.len() + c);
    let k = 4;
    let n = (0..k).map(|x| x.to_string()).count();
    println!("{}", n + k);

    // Pinned to a big integer type after the fact: `to_string` still prints it right.
    let big = 9_000_000_000;
    let t = big.to_string();
    let wide: u64 = big;
    println!("{} {}", t, wide * 2);

    // Comparisons and `clone` leave it open too.
    let a = 3;
    let b = a.clone();
    println!("{} {} {:?}", a.eq(&b), a.lt(&5), a.partial_cmp(&2));
    let w: Vec<u8> = Vec::with_capacity(b);
    println!("{}", w.len() + a);

    // With no later pin, the literal falls back to `i32`.
    let f = 7;
    let g = f.to_string();
    println!("{} {}", g, f / 2);

    // Floats the same way.
    let x = 1.5;
    let y = x.to_string();
    let z: f32 = x;
    println!("{} {}", y, z * 2.0);
}
