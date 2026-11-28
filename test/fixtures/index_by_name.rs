// `Index::index` / `IndexMut::index_mut` called by name, as methods and by path; and a chain
// of operator methods sharing one receiver (`a.add(b).add(5).neg()`).
use std::ops::{Add, Index, IndexMut, Neg, Range};

struct Sv(Vec<u8>);
impl Index<Range<usize>> for Sv {
    type Output = [u8];
    fn index(&self, r: Range<usize>) -> &[u8] {
        &self.0[r]
    }
}
impl Index<usize> for Sv {
    type Output = u8;
    fn index(&self, i: usize) -> &u8 {
        &self.0[i]
    }
}
impl IndexMut<usize> for Sv {
    fn index_mut(&mut self, i: usize) -> &mut u8 {
        &mut self.0[i]
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Cell {
    name: String,
    hits: u32,
}
struct Grid {
    cells: Vec<Cell>,
}
impl Index<(usize, usize)> for Grid {
    type Output = Cell;
    fn index(&self, (r, c): (usize, usize)) -> &Cell {
        &self.cells[r * 2 + c]
    }
}
impl IndexMut<(usize, usize)> for Grid {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut Cell {
        &mut self.cells[r * 2 + c]
    }
}

#[derive(Clone, Copy, Debug)]
struct V(i32);
impl Add for V {
    type Output = V;
    fn add(self, o: V) -> V {
        V(self.0 + o.0)
    }
}
impl Add<i32> for V {
    type Output = V;
    fn add(self, o: i32) -> V {
        V(self.0 + o)
    }
}
impl Neg for V {
    type Output = V;
    fn neg(self) -> V {
        V(-self.0)
    }
}

fn first<T: Index<usize, Output = u8>>(t: &T) -> u8 {
    *t.index(0)
}

fn main() {
    let mut sv = Sv(vec![1, 2, 3, 4]);
    println!("{:?}", sv.index(1..3));
    println!("{}", sv.index(1..3).len());
    *sv.index_mut(0) = 9;
    *sv.index_mut(3) += 10;
    println!("{} {} {}", sv.index(0), *sv.index(3), Index::index(&sv, 1));
    *IndexMut::index_mut(&mut sv, 2) = 7;
    println!("{:?} {}", sv.0, std::ops::Index::index(&sv, 2));
    println!("{}", first(&sv));

    let mut g = Grid {
        cells: (0..4)
            .map(|i| Cell { name: format!("c{i}"), hits: 0 })
            .collect(),
    };
    g.index_mut((1, 0)).hits += 3;
    g.index_mut((0, 1)).name.push('!');
    let c = g.index((1, 0)).clone();
    println!("{:?} {:?} {}", c, g.index((0, 1)), g[(1, 0)].hits);

    let v = vec![10, 20, 30];
    println!("{} {:?}", v.index(1), v.index(1..));

    println!("{}", V(1).add(V(2)).add(5).neg().0);
    println!("{}", V(1).neg().add(V(2)).0);
    println!("{}", (-V(4)).add(1).add(V(1).add(1)).0);
}
