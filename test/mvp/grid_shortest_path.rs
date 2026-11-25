// Coding contract: "Shortest Path in a Grid"
// 0 = open, 1 = wall. Answer is a string of U/D/L/R from top-left to bottom-right, or "" if none.
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    const ALL: [Dir; 4] = [Dir::Up, Dir::Down, Dir::Left, Dir::Right];

    fn delta(self) -> (i32, i32) {
        match self {
            Dir::Up => (-1, 0),
            Dir::Down => (1, 0),
            Dir::Left => (0, -1),
            Dir::Right => (0, 1),
        }
    }

    fn letter(self) -> char {
        match self {
            Dir::Up => 'U',
            Dir::Down => 'D',
            Dir::Left => 'L',
            Dir::Right => 'R',
        }
    }
}

fn shortest_path(grid: &[Vec<u8>]) -> String {
    let rows = grid.len() as i32;
    let cols = grid[0].len() as i32;
    let goal = (rows - 1, cols - 1);
    if grid[0][0] == 1 {
        return String::new();
    }

    let mut came_from: HashMap<(i32, i32), ((i32, i32), Dir)> = HashMap::new();
    let mut queue = VecDeque::from([(0, 0)]);

    while let Some((r, c)) = queue.pop_front() {
        if (r, c) == goal {
            break;
        }
        for dir in Dir::ALL {
            let (dr, dc) = dir.delta();
            let (nr, nc) = (r + dr, c + dc);
            if nr < 0 || nc < 0 || nr >= rows || nc >= cols {
                continue;
            }
            if grid[nr as usize][nc as usize] == 1 || (nr, nc) == (0, 0) {
                continue;
            }
            if came_from.contains_key(&(nr, nc)) {
                continue;
            }
            came_from.insert((nr, nc), ((r, c), dir));
            queue.push_back((nr, nc));
        }
    }

    let mut path = Vec::new();
    let mut at = goal;
    while at != (0, 0) {
        match came_from.get(&at) {
            Some(&(prev, dir)) => {
                path.push(dir.letter());
                at = prev;
            }
            None => return String::new(),
        }
    }
    path.iter().rev().collect()
}

fn main() {
    let grids: Vec<Vec<Vec<u8>>> = vec![
        vec![vec![0, 1, 0, 0, 0], vec![0, 0, 0, 1, 0]],
        vec![vec![0, 1], vec![1, 0]],
        vec![
            vec![0, 0, 0, 0, 0, 0],
            vec![1, 1, 1, 1, 1, 0],
            vec![0, 0, 0, 0, 0, 0],
            vec![0, 1, 1, 1, 1, 1],
            vec![0, 0, 0, 0, 0, 0],
        ],
        vec![vec![0]],
    ];

    for (i, grid) in grids.iter().enumerate() {
        let answer = shortest_path(grid);
        println!(
            "grid {} ({}x{}): {:?} (len {})",
            i,
            grid.len(),
            grid[0].len(),
            answer,
            answer.len()
        );
    }
}
