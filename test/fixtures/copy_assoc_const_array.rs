// Reading an associated `const` array of `Copy` values copies it, as for a plain `const`:
// changing the copy leaves the item alone.
#[derive(Clone, Copy, Debug)]
struct Q {
    y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Dir {
    N,
    E,
}

struct H;

impl H {
    const QS: [Q; 2] = [Q { y: 1 }, Q { y: 2 }];
    const DIRS: [Dir; 2] = [Dir::N, Dir::E];
    const ONE: Q = Q { y: 7 };

    fn bump() -> [Q; 2] {
        let mut a = Self::QS;
        a[0].y = 9;
        a
    }
}

trait Table {
    const ROWS: [[u8; 2]; 2];
    fn rows(&self) -> [[u8; 2]; 2] {
        let mut r = Self::ROWS;
        r[0][0] = 50;
        r
    }
}

struct T;

impl Table for T {
    const ROWS: [[u8; 2]; 2] = [[1, 2], [3, 4]];
}

trait Marks {
    const MARKS: [char; 2] = ['a', 'b'];
}

impl Marks for T {}

fn tweak<X: Table + Marks>() -> ([[u8; 2]; 2], [char; 2]) {
    let mut r = X::ROWS;
    r[1][0] = 9;
    let mut m = X::MARKS;
    m[0] = 'z';
    (r, m)
}

fn main() {
    let b = H::bump();
    println!("{:?} {:?}", b, H::QS);
    let mut a = H::QS;
    a[1].y = 20;
    let mut d = H::DIRS;
    d[0] = Dir::E;
    let mut one = H::ONE;
    one.y = 70;
    println!("{:?} {:?} {:?} {:?} {:?} {:?}", a, H::QS, d, H::DIRS, one, H::ONE);
    let mut q = H::QS[0];
    q.y = 5;
    println!("{:?} {:?}", q, H::QS);
    let r = T.rows();
    let mut s = T::ROWS;
    s[1][1] = 40;
    println!("{:?} {:?} {:?}", r, s, T::ROWS);
    let (r2, m) = tweak::<T>();
    let mut m2 = T::MARKS;
    m2[1] = 'y';
    println!("{:?} {:?} {:?} {:?} {:?}", r2, T::ROWS, m, m2, T::MARKS);
}
