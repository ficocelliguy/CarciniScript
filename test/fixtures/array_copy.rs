// Fixed-size arrays of `Copy` values are `Copy`: `let b = a`, passing, returning, struct fields
// and rows of a 2D array copy, so changing the copy leaves the original alone. A `Vec` (or an
// array of `String`s) still moves.
#[derive(Debug, Clone, Copy)]
struct Board {
    cells: [[u8; 3]; 3],
    turn: u8,
}

fn bump(mut a: [i32; 3]) -> [i32; 3] {
    a[0] += 100;
    a
}

fn first_row(b: Board) -> [u8; 3] {
    b.cells[0]
}

fn main() {
    let a = [1, 2, 3];
    let mut b = a;
    b[0] = 9;
    println!("{:?} {:?}", a, b);

    let c = bump(a);
    println!("{:?} {:?}", a, c);

    let names = ["x", "y"];
    let mut more = names;
    more[1] = "z";
    println!("{:?} {:?}", names, more);

    let grid = [[0u8; 3]; 3];
    let mut g2 = grid;
    g2[1][1] = 5;
    let mut row = grid[2];
    row[0] = 7;
    println!("{:?} {:?} {:?}", grid, g2, row);

    let mut board = Board { cells: grid, turn: 1 };
    let snapshot = board;
    board.cells[0][0] = 4;
    board.turn = 2;
    let mut cells = board.cells;
    cells[2][2] = 8;
    println!("{:?}", snapshot);
    println!("{:?} {:?}", board, cells);
    let mut r = first_row(board);
    r[1] = 6;
    println!("{:?} {:?}", r, board.cells[0]);

    let typed: [f64; 2] = [0.5, 1.5];
    let mut t2 = typed;
    t2[0] = 2.0;
    let mut slots = [None, Some(1)];
    let s2 = slots;
    slots[0] = Some(3);
    println!("{:?} {:?} {:?} {:?}", typed, t2, slots, s2);

    let mut history = Vec::new();
    let mut state = [0, 0];
    for i in 1..=3 {
        state[i % 2] += i;
        history.push(state);
    }
    println!("{:?}", history);
    let v = vec![1, 2];
    let w = v;
    println!("{:?}", w);
}
