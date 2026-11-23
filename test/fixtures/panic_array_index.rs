// An out-of-bounds index into an array panics at the start of the indexed expression (a `Vec`'s
// at the `[`), inside a macro argument or a nested row too.
fn get_index(n: usize) -> usize {
    n + 3
}

fn total(s: &[i32], n: usize) -> i32 {
    if n < 3 { s[n] } else { 0 }
}

fn main() {
    let grid = [[1, 2], [3, 4]];
    let v = vec![5, 6];
    println!("{} {} {}", grid[1][get_index(0) - 2], v[get_index(0) - 2], total(&[7, 8, 9], 2));
    let mut a = [1, 2, 3];
    a[get_index(0)] = 0;
    println!("{:?}", a);
}
