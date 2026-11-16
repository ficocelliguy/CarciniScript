// `Default` of fixed-size arrays: N default elements, in `#[derive(Default)]` structs (also
// nested arrays, arrays of structs and a `const` length) and for a `let` annotation.
const SLOTS: usize = 3;

#[derive(Default, Debug, Clone)]
struct Threads([u32; 4]);

#[derive(Default, Debug, Clone, PartialEq)]
struct Tile {
    hits: u8,
    name: String,
}

#[derive(Default, Debug)]
struct Board {
    grid: [[u8; 2]; 3],
    cells: [Tile; 2],
    slots: [Option<i64>; SLOTS],
    flags: [bool; 0],
    total: f64,
}

fn main() {
    let mut t = Threads::default();
    t.0[2] = 7;
    println!("{t:?} {}", t.0.len());
    let mut b = Board::default();
    b.grid[1][0] = 9;
    b.cells[0].hits += 1;
    println!("{b:?}");
    // Each element is its own value.
    println!("{}", b.cells[1] == Tile::default());
    let x: [u8; 2] = Default::default();
    let y: [[i32; 2]; 2] = Default::default();
    let z: [String; 3] = Default::default();
    println!("{x:?} {y:?} {z:?}");
}
