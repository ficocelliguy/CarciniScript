// Const generics with trait impls (Default, Display, dyn), T::default() through a class view,
// derived Default/Clone/Hash, and struct update syntax.
use std::collections::HashSet;
#[derive(Debug, Clone)]
struct Stack<const N: usize> {
    items: Vec<i32>,
}

impl<const N: usize> Default for Stack<N> {
    fn default() -> Self {
        Stack {
            items: Vec::with_capacity(N),
        }
    }
}

impl<const N: usize> Stack<N> {
    fn cap(&self) -> usize {
        N
    }
    fn full(&self) -> bool {
        self.items.len() == N
    }
}

fn make<T: Default>() -> T {
    T::default()
}

trait Sized2 {
    fn size(&self) -> usize;
}

impl<const N: usize> Sized2 for Stack<N> {
    fn size(&self) -> usize {
        N * 10
    }
}

fn show(s: &dyn Sized2) -> usize {
    s.size()
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
struct Slots<const N: usize> {
    used: Vec<usize>,
    name: String,
}

impl<const N: usize> Slots<N> {
    fn free(&self) -> usize {
        N - self.used.len()
    }
    fn indices(&self) -> Vec<usize> {
        (0..N).filter(|i| !self.used.contains(i)).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vector<const D: usize> {
    c: [f64; D],
}

impl<const D: usize> Vector<D> {
    fn dot(&self, o: &Self) -> f64 {
        (0..D).map(|i| self.c[i] * o.c[i]).sum()
    }
    fn scaled(&self, k: f64) -> Self {
        let mut c = self.c;
        for x in c.iter_mut() {
            *x *= k;
        }
        Vector { c }
    }
}

fn largest<const N: usize>(xs: [u32; N]) -> Option<u32> {
    xs.iter().copied().max()
}

use std::fmt;
struct Grid<const W: usize, const H: usize> {
    cells: [[char; W]; H],
}
impl<const W: usize, const H: usize> Default for Grid<W, H> {
    fn default() -> Self {
        Grid {
            cells: [[(b'.') as char; W]; H],
        }
    }
}
impl<const W: usize, const H: usize> fmt::Display for Grid<W, H> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for row in &self.cells {
            writeln!(f, "{}", row.iter().collect::<String>())?;
        }
        write!(f, "{}x{}", W, H)
    }
}
fn first<T: Copy + Default, const N: usize>(a: &[T; N]) -> T {
    if N > 0 { a[0] } else { T::default() }
}
fn main() {
    let a: Stack<3> = Default::default();
    println!("{}", a.cap());
    let b: Stack<5> = make();
    println!("{} {}", b.cap(), b.full());
    let c = b.clone();
    println!("{} {}", c.cap(), show(&c));
    let boxed: Vec<Box<dyn Sized2>> = vec![Box::new(a), Box::new(c)];
    for x in &boxed {
        println!("{}", x.size());
    }
    let s: Slots<4> = Default::default();
    println!("{} {:?}", s.free(), s);
    let t = Slots::<4> {
        used: vec![1, 3],
        ..s.clone()
    };
    println!("{} {:?}", t.free(), t.indices());
    let mut set = HashSet::new();
    set.insert(s.clone());
    set.insert(t.clone());
    set.insert(s.clone());
    println!("{}", set.len());
    let v = Vector { c: [1.0, 2.0, 3.0] };
    let w = v.scaled(2.0);
    println!("{} {:?} {:?}", v.dot(&w), w, v == w);
    println!("{:?} {:?}", largest([3, 9, 2]), largest::<0>([]));
    let adders: Vec<Box<dyn Fn(usize) -> usize>> = (0..2usize)
        .map(|k| Box::new(move |x| x + k) as Box<dyn Fn(usize) -> usize>)
        .collect();
    println!("{}", adders[1](s.free()));
    let mut g: Grid<4, 2> = Grid::default();
    g.cells[1][2] = (b'#') as char;
    println!("{}", g);
    let g2 = Grid::<2, 3>::default();
    println!("{}", g2);
    println!("{} {}", first(&[5, 6]), first::<i32, 0>(&[]));
    let arr = [[1u8; 3]; 2];
    println!("{:?}", arr);
}
