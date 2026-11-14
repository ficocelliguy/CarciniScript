// Associated constants: in inherent impls (`Self::W`, `Grid::W`, used by other constants and
// methods), declared in a trait (`const SCALE: f64;`) and given by each impl, and on enums.
trait Unit {
    const SCALE: f64;
    const NAME: &'static str;
    fn to_base(&self, x: f64) -> f64 {
        x * 1.0
    }
}

struct Grid {
    cells: Vec<u8>,
}

impl Grid {
    const W: usize = 4;
    const H: usize = 3;
    const AREA: usize = Self::W * Self::H;
    const EMPTY: u8 = b'.';

    fn new() -> Self {
        Grid { cells: vec![Self::EMPTY; Grid::AREA] }
    }

    fn row(&self, r: usize) -> String {
        self.cells[r * Self::W..(r + 1) * Self::W].iter().map(|&c| c as char).collect()
    }
}

struct Meters;
struct Feet;

impl Unit for Meters {
    const SCALE: f64 = 1.0;
    const NAME: &'static str = "m";
}

impl Unit for Feet {
    const SCALE: f64 = 0.3048;
    const NAME: &'static str = "ft";
}

#[derive(Debug, Clone, Copy)]
enum Level {
    Low,
    High,
}

impl Level {
    const ALL: [Level; 2] = [Level::Low, Level::High];
    const LIMIT: i32 = 10;
}

fn main() {
    let mut g = Grid::new();
    g.cells[Grid::W + 1] = b'#';
    println!("{} {} {}", Grid::W, Grid::H, Grid::AREA);
    for r in 0..Grid::H {
        println!("{}", g.row(r));
    }
    println!("{} {} {} {}", Meters::SCALE, Meters::NAME, Feet::SCALE * 10.0, Feet::NAME);
    println!("{:?} {}", Level::ALL, Level::LIMIT * 2);
}
