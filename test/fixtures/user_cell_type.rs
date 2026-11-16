// A type of the program's own named like a std type (`Cell`) is that type: in `Vec<Cell>`,
// `type Output = Cell`, `Result<Self, char>`, and `Cell::new(..)` of its own. std's `RefCell`
// still works next to it.
use std::cell::RefCell;
use std::ops::Index;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Cell {
    Empty,
    Black,
    White,
}

impl Cell {
    fn new(c: char) -> Self {
        Cell::try_from(c).unwrap_or(Cell::Empty)
    }
}

impl TryFrom<char> for Cell {
    type Error = char;
    fn try_from(c: char) -> Result<Self, char> {
        match c {
            '.' => Ok(Cell::Empty),
            'X' => Ok(Cell::Black),
            'O' => Ok(Cell::White),
            other => Err(other),
        }
    }
}

struct Board {
    cells: Vec<Cell>,
    width: usize,
}

impl Index<(usize, usize)> for Board {
    type Output = Cell;
    fn index(&self, (x, y): (usize, usize)) -> &Cell {
        &self.cells[y * self.width + x]
    }
}

fn main() {
    let rows = ["X.O", "OX?"];
    let mut cells: Vec<Cell> = Vec::new();
    for r in rows {
        for c in r.chars() {
            match Cell::try_from(c) {
                Ok(cell) => cells.push(cell),
                Err(bad) => {
                    println!("bad symbol {bad:?}");
                    cells.push(Cell::new(bad));
                }
            }
        }
    }
    let b = Board { cells, width: 3 };
    println!("{:?} {:?} {}", b[(0, 0)], b[(2, 1)], b[(1, 1)] == Cell::Black);
    let count = RefCell::new(0);
    for c in &b.cells {
        if *c != Cell::Empty {
            *count.borrow_mut() += 1;
        }
    }
    println!("{} stones", count.borrow());
}
