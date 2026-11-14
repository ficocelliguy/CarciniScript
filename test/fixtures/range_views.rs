// Writes through a range of a Vec are views too: `v[2..].iter_mut()`, `v[0..2][1] = 9`, `sort_by` on a
// range, `split_at_mut`, and ranges of rows in a grid.
fn main() {
    let mut v = vec![1, 2, 3, 4, 5, 6];
    for x in v[2..].iter_mut() {
        *x *= 10;
    }
    v[0..2][1] = 9;
    v[1..5].sort_by(|a, b| b.cmp(a));
    let (left, right) = v.split_at_mut(3);
    left[0] += right[0];
    println!("{:?}", v);
    let mut grid = vec![vec![0; 4]; 2];
    for row in grid.iter_mut() {
        for c in row[1..3].iter_mut() {
            *c = 7;
        }
    }
    println!("{:?}", grid);
}
