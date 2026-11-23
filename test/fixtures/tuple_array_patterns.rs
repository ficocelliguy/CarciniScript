// Array patterns inside tuple patterns are irrefutable where the array lengths fit: a tuple
// literal's, and the items of `enumerate()` / `zip(..)` over arrays.
fn main() {
    let a = [[1, 2, 3], [4, 5, 6]];
    for (j, [first, rest @ ..]) in a.iter().enumerate() {
        println!("{} {} {:?}", j, first, rest);
    }
    let ([x, ..], y) = ([7, 8], 9);
    println!("{} {}", x, y);
    let pairs: [[u8; 2]; 2] = [[1, 2], [3, 4]];
    let names = ["a", "b"];
    for ([k, v], n) in pairs.iter().zip(names.iter()) {
        println!("{} {} {}", k, v, n);
    }
}
