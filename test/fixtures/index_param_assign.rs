// `c[ix] = e` / `c[ix] += 1` through a type parameter's `IndexMut<Ix>` bound, given a Vec, a
// VecDeque, an array or your own type with `impl IndexMut` (numbered after other operator impls).
use std::collections::VecDeque;
use std::ops::{Add, Index, IndexMut};

#[derive(Debug, Clone, Copy, PartialEq)]
struct V2(i32, i32);

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        V2(self.0 + o.0, self.1 + o.1)
    }
}

struct Grid {
    w: usize,
    cells: Vec<i64>,
}

// (Another operator impl first: `Grid`'s `index` / `index_mut` aren't its first.)
impl std::ops::Neg for Grid {
    type Output = Grid;
    fn neg(self) -> Grid {
        Grid { w: self.w, cells: self.cells.iter().map(|c| -c).collect() }
    }
}

fn at<C: Index<(usize, usize), Output = i64>>(c: &C, ix: (usize, usize)) -> i64 {
    c[ix]
}

impl Index<(usize, usize)> for Grid {
    type Output = i64;
    fn index(&self, (x, y): (usize, usize)) -> &i64 {
        &self.cells[y * self.w + x]
    }
}

impl IndexMut<(usize, usize)> for Grid {
    fn index_mut(&mut self, (x, y): (usize, usize)) -> &mut i64 {
        &mut self.cells[y * self.w + x]
    }
}

struct Names {
    v: Vec<String>,
}

impl Index<usize> for Names {
    type Output = String;
    fn index(&self, i: usize) -> &String {
        &self.v[i]
    }
}

impl IndexMut<usize> for Names {
    fn index_mut(&mut self, i: usize) -> &mut Self::Output {
        &mut self.v[i]
    }
}

fn bump<C: IndexMut<usize, Output = i32>>(c: &mut C, i: usize) {
    c[i] += 1;
    c[i] *= 10;
}

fn put<C, Ix, T>(c: &mut C, ix: Ix, v: T)
where
    C: IndexMut<Ix, Output = T>,
{
    c[ix] = v;
}

fn swap_in<C>(c: &mut C, a: usize, b: usize)
where
    C: IndexMut<usize, Output = V2>,
{
    let t = c[a];
    c[a] = c[b];
    c[b] = t + V2(1, 1);
}

fn main() {
    let mut v = vec![1, 2, 3];
    bump(&mut v, 1);
    let mut d: VecDeque<i32> = VecDeque::new();
    d.push_back(5);
    d.push_front(4);
    bump(&mut d, 0);
    let mut a = [7, 8];
    bump(&mut a, 0);
    println!("{:?} {:?} {:?}", v, d, a);

    let mut g = Grid { w: 2, cells: vec![0; 4] };
    put(&mut g, (1, 1), 9);
    put(&mut g, (0, 1), -3);
    println!("{:?} {} {}", g.cells, g[(1, 1)], at(&g, (0, 1)));
    let g = -g;
    println!("{}", at(&g, (1, 1)));

    let mut n = Names { v: vec!["a".to_string(), "b".to_string()] };
    put(&mut n, 0, "z".to_string());
    let mut ws = vec!["x".to_string()];
    put(&mut ws, 0, "y".to_string());
    println!("{:?} {:?}", n.v, ws);

    let mut ps = vec![V2(1, 2), V2(3, 4)];
    swap_in(&mut ps, 0, 1);
    println!("{:?}", ps);
}
