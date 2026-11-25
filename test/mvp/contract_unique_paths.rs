// Coding contracts: "Unique Paths in a Grid I" and "II".

fn unique_paths(rows: usize, cols: usize) -> u64 {
    let mut row = vec![1u64; cols];
    for _ in 1..rows {
        for c in 1..cols {
            row[c] += row[c - 1];
        }
    }
    row[cols - 1]
}

fn unique_paths_obstacles(grid: &[Vec<u8>]) -> u64 {
    let cols = grid[0].len();
    let mut dp = vec![0u64; cols];
    dp[0] = 1;
    for row in grid {
        for c in 0..cols {
            if row[c] == 1 {
                dp[c] = 0;
            } else if c > 0 {
                dp[c] += dp[c - 1];
            }
        }
    }
    dp[cols - 1]
}

fn parse_grid(text: &str) -> Vec<Vec<u8>> {
    text.lines()
        .map(|line| line.split_whitespace().map(|n| n.parse().unwrap()).collect())
        .collect()
}

fn main() {
    println!("{}", unique_paths(3, 7));
    println!("{}", unique_paths(14, 10));
    println!("{}", unique_paths(17, 17));
    let grid = parse_grid(
        "0 0 0 0 0
         0 1 1 0 0
         0 0 0 1 0
         1 1 0 0 0",
    );
    println!("{} rows, {} paths", grid.len(), unique_paths_obstacles(&grid));
    let blocked = parse_grid("0 1\n1 0");
    println!("{}", unique_paths_obstacles(&blocked));
}
