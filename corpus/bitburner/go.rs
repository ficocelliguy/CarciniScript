//! IPvGO player: captures when it can, saves its own groups in atari, otherwise plays the
//! move that maximizes liberties and territory. Plays games back to back.
//!
//! run go.rs [opponent="Netburners"] [size=7]

use std::collections::{HashSet, VecDeque};
use std::ops::{Index, Not};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Cell {
    Empty,
    Dead,
    Stone(Color),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Color {
    Black,
    White,
}

impl Not for Color {
    type Output = Color;
    fn not(self) -> Color {
        match self {
            Color::Black => Color::White,
            Color::White => Color::Black,
        }
    }
}

impl TryFrom<char> for Cell {
    type Error = char;
    fn try_from(c: char) -> Result<Self, char> {
        Ok(match c {
            '.' => Cell::Empty,
            '#' => Cell::Dead,
            'X' => Cell::Stone(Color::Black),
            'O' => Cell::Stone(Color::White),
            other => return Err(other),
        })
    }
}

type Point = (usize, usize);

#[derive(Debug, Clone)]
struct Board {
    size: usize,
    cells: Vec<Cell>,
}

impl Index<Point> for Board {
    type Output = Cell;
    fn index(&self, (x, y): Point) -> &Cell {
        &self.cells[x * self.size + y]
    }
}

/// A connected group of one color and its liberties.
#[derive(Debug)]
struct Group {
    color: Color,
    stones: HashSet<Point>,
    liberties: HashSet<Point>,
}

impl Board {
    /// The game gives columns as strings: `board[x][y]`.
    fn parse(rows: &[String]) -> Result<Board, char> {
        let size = rows.len();
        let cells = rows
            .iter()
            .flat_map(|r| r.chars())
            .map(Cell::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Board { size, cells })
    }

    fn neighbors(&self, (x, y): Point) -> impl Iterator<Item = Point> + '_ {
        const DIRS: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        DIRS.iter().filter_map(move |&(dx, dy)| {
            let nx = x.checked_add_signed(dx)?;
            let ny = y.checked_add_signed(dy)?;
            (nx < self.size && ny < self.size).then_some((nx, ny))
        })
    }

    fn points(&self) -> impl Iterator<Item = Point> {
        let n = self.size;
        (0..n).flat_map(move |x| (0..n).map(move |y| (x, y)))
    }

    fn group_at(&self, start: Point) -> Option<Group> {
        let Cell::Stone(color) = self[start] else { return None };
        let mut group = Group { color, stones: HashSet::new(), liberties: HashSet::new() };
        let mut queue = VecDeque::from([start]);
        while let Some(p) = queue.pop_front() {
            if !group.stones.insert(p) {
                continue;
            }
            for n in self.neighbors(p) {
                match self[n] {
                    Cell::Empty => {
                        group.liberties.insert(n);
                    }
                    Cell::Stone(c) if c == color => queue.push_back(n),
                    _ => {}
                }
            }
        }
        Some(group)
    }

    fn groups(&self) -> Vec<Group> {
        let mut seen = HashSet::new();
        self.points()
            .filter_map(|p| {
                if seen.contains(&p) {
                    return None;
                }
                let g = self.group_at(p)?;
                seen.extend(g.stones.iter().copied());
                Some(g)
            })
            .collect()
    }

    /// Empty regions bordered only by `color`.
    fn territory(&self, color: Color) -> usize {
        let mut seen = HashSet::new();
        let mut total = 0;
        for p in self.points().filter(|&p| self[p] == Cell::Empty) {
            if seen.contains(&p) {
                continue;
            }
            let mut region = vec![];
            let mut borders = HashSet::new();
            let mut stack = vec![p];
            while let Some(q) = stack.pop() {
                if !seen.insert(q) {
                    continue;
                }
                region.push(q);
                for n in self.neighbors(q) {
                    match self[n] {
                        Cell::Empty => stack.push(n),
                        Cell::Stone(c) => {
                            borders.insert(c);
                        }
                        Cell::Dead => {}
                    }
                }
            }
            if borders.len() == 1 && borders.contains(&color) {
                total += region.len();
            }
        }
        total
    }

    fn with_move(&self, p: Point, color: Color) -> Board {
        let mut next = self.clone();
        next.cells[p.0 * self.size + p.1] = Cell::Stone(color);
        // remove captured enemy groups
        for g in next.groups().into_iter().filter(|g| g.color != color && g.liberties.is_empty()) {
            for s in g.stones {
                next.cells[s.0 * self.size + s.1] = Cell::Empty;
            }
        }
        next
    }
}

fn score_move(board: &Board, p: Point, me: Color) -> i64 {
    let after = board.with_move(p, me);
    let groups = after.groups();
    let mine_in_atari = groups.iter().filter(|g| g.color == me && g.liberties.len() == 1).count();
    let captured = board.cells.iter().filter(|c| **c == Cell::Stone(!me)).count()
        - after.cells.iter().filter(|c| **c == Cell::Stone(!me)).count();
    let my_liberties: usize = groups.iter().filter(|g| g.color == me).map(|g| g.liberties.len()).sum();
    let enemy_in_atari = groups.iter().filter(|g| g.color != me && g.liberties.len() == 1).count();

    1000 * captured as i64 + 30 * enemy_in_atari as i64 - 200 * mine_in_atari as i64
        + 5 * my_liberties as i64
        + 10 * after.territory(me) as i64
        - 10 * after.territory(!me) as i64
}

fn best_move(board: &Board, valid: &[Vec<bool>], me: Color) -> Option<Point> {
    board
        .points()
        .filter(|&(x, y)| valid[x][y])
        .map(|p| (score_move(board, p, me), p))
        .max_by_key(|&(s, (x, y))| (s, std::cmp::Reverse(x + y)))
        .filter(|&(s, _)| s > -100)
        .map(|(_, p)| p)
}

pub async fn main(ns: NS) {
    let opponent = ns.args.first().and_then(|a| a.as_string()).unwrap_or_else(|| "Netburners".into());
    let size = ns.args.get(1).and_then(|a| a.as_f64()).unwrap_or(7.0);
    let (mut wins, mut games) = (0u32, 0u32);

    loop {
        ns.go.reset_board_state(&opponent, size);
        games += 1;
        let result = 'game: loop {
            let rows = ns.go.get_board_state();
            let board = match Board::parse(&rows) {
                Ok(b) => b,
                Err(c) => {
                    ns.print(&format!("unexpected board symbol {c:?}"));
                    break 'game None;
                }
            };
            let valid = ns.go.analysis.get_valid_moves();
            let outcome = match best_move(&board, &valid, Color::Black) {
                Some((x, y)) => ns.go.make_move(x as f64, y as f64).await,
                None => ns.go.pass_turn().await,
            };
            if outcome.r#type == "gameOver" {
                break 'game Some(ns.go.get_game_state());
            }
        };
        if let Some(state) = result {
            let won = state.black_score > state.white_score;
            wins += u32::from(won);
            ns.print(&format!(
                "game {games}: {} {:.1} – {:.1} ({wins}/{games} won)",
                if won { "won" } else { "lost" },
                state.black_score,
                state.white_score,
            ));
        }
        ns.sleep(200).await;
    }
}
