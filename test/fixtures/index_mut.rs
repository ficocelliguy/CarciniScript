// `impl IndexMut`: `grid[(r, c)] = v`, `+=` / `*=` through it, an index held in a variable, and
// `names[i].push(..)` / `+=` on `String` elements; destructured parameters (`(r, c): (usize, usize)`).
use std::ops::{Index, IndexMut};

#[derive(Debug)]
struct Grid {
    w: usize,
    cells: Vec<u8>,
}

impl Index<(usize, usize)> for Grid {
    type Output = u8;
    fn index(&self, (r, c): (usize, usize)) -> &u8 {
        &self.cells[r * self.w + c]
    }
}

impl IndexMut<(usize, usize)> for Grid {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut u8 {
        let i = r * self.w + c;
        &mut self.cells[i]
    }
}

struct Names(Vec<String>);

impl Index<usize> for Names {
    type Output = String;
    fn index(&self, i: usize) -> &String {
        &self.0[i]
    }
}

impl IndexMut<usize> for Names {
    fn index_mut(&mut self, i: usize) -> &mut String {
        &mut self.0[i]
    }
}

fn main() {
    let mut g = Grid { w: 3, cells: vec![0; 6] };
    g[(1, 2)] = 7;
    g[(0, 0)] += 2;
    g[(0, 0)] *= 5;
    let p = (1, 1);
    g[p] = g[(1, 2)] + 1;
    println!("{:?} {}", g.cells, g[(0, 0)]);
    let mut n = Names(vec!["a".to_string(), "b".to_string()]);
    n[1].push('!');
    n[0] = "z".to_string();
    n[1] += "?";
    println!("{:?}", n.0);
}
