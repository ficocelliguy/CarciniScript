// Inherent impls per type argument, each with its own associated consts of one name (as macros
// in bitvec, crossbeam and jiff generate them).
struct Cell<T>(T);

impl Cell<u8> {
    const MIN: u8 = 1;
    const BITS: u32 = 8;
    fn lo() -> u8 {
        Self::MIN
    }
    fn show(&self) {
        println!("u8 {} {} {}", Self::MIN, Self::BITS, self.0);
    }
}

impl Cell<i32> {
    const MIN: i32 = -5;
    const BITS: u32 = 32;
    fn lo() -> i32 {
        Self::MIN * 2
    }
    fn show(&self) {
        println!("i32 {} {} {}", Self::MIN, Self::BITS, self.0);
    }
}

struct Lanes<const N: usize>;
impl Lanes<2> {
    const WIDTH: usize = 2;
}
impl Lanes<4> {
    const WIDTH: usize = 4;
}

fn main() {
    println!("{} {}", Cell::<u8>::MIN, Cell::<i32>::MIN);
    let m = Cell::<i32>::MIN + 1;
    println!("{} {} {}", Cell::<u8>::lo(), Cell::<i32>::lo(), m);
    Cell(3u8).show();
    Cell(7i32).show();
    let a = [0u8; Cell::<i32>::BITS as usize];
    println!("{} {} {}", a.len(), Lanes::<2>::WIDTH, Lanes::<4>::WIDTH);
}
