// A contract solver mid-edit: an import and a variable left over, a value assigned and never
// read, and an ignored `Result` from writing into a String.
use std::collections::HashSet;
use std::fmt::Write;

fn count_islands(grid: &[&str]) -> usize {
    let rows = grid.len();
    let cols = grid[0].len();
    let cells: Vec<Vec<u8>> = grid.iter().map(|r| r.bytes().collect()).collect();
    let mut seen = vec![vec![false; cols]; rows];
    let mut islands = 0;
    for r in 0..rows {
        for c in 0..cols {
            if cells[r][c] != b'#' || seen[r][c] {
                continue;
            }
            islands += 1;
            let mut stack = vec![(r, c)];
            let mut size = 0;
            while let Some((y, x)) = stack.pop() {
                if seen[y][x] {
                    continue;
                }
                seen[y][x] = true;
                size = 1;
                if y > 0 && cells[y - 1][x] == b'#' { stack.push((y - 1, x)); }
                if y + 1 < rows && cells[y + 1][x] == b'#' { stack.push((y + 1, x)); }
                if x > 0 && cells[y][x - 1] == b'#' { stack.push((y, x - 1)); }
                if x + 1 < cols && cells[y][x + 1] == b'#' { stack.push((y, x + 1)); }
            }
        }
    }
    islands
}

fn main() {
    let grid = ["##..#", "#...#", "..#..", "....#"];
    let width = grid[0].len();
    let mut out = String::new();
    write!(out, "islands: {}", count_islands(&grid));
    println!("{out}");
}
