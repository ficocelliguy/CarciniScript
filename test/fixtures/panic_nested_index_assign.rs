// `grid[i][j] += x` with `j` out of range: the panic reports the inner index.
fn main() {
    let mut grid = vec![vec![1, 2, 3]; 2];
    let j = grid[0].len() + 2;
    println!("before");
    grid[1][j] += 1;
    println!("{:?}", grid);
}
