// Taking apart fixed-size arrays std returns (`to_le_bytes`, `arr.map(..)`) and the rows of a
// `Vec<[T; N]>`: irrefutable, as in rustc.
fn main() {
    let x: u16 = 0x1234;
    let [a, b] = x.to_le_bytes();
    println!("{} {}", a, b);
    let [c, d, e, f] = 0x01020304u32.to_be_bytes();
    println!("{} {} {} {}", c, d, e, f);
    let rows: Vec<[i32; 2]> = vec![[1, 2], [3, 4]];
    for [p, q] in rows.iter() {
        println!("{} {}", p, q);
    }
    for [p, q] in rows {
        println!("{}", p + q);
    }
    let grid = [[1u8, 2, 3], [4, 5, 6]];
    for &[r, s, t] in &grid {
        println!("{} {} {}", r, s, t);
    }
    let [m, n] = [7i32, 8].map(|v| v * 2);
    println!("{} {}", m, n);
}
