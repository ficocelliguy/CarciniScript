// A fieldless `Copy` enum is one shared, immutable value per variant: reads don't copy it, and
// every `&mut` to one is a cell over its place, as for a number.
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord, Default)]
enum Dir {
    #[default]
    N,
    E,
    S,
    W,
}

impl Dir {
    fn turn(&mut self) {
        *self = match self {
            Dir::N => Dir::E,
            Dir::E => Dir::S,
            Dir::S => Dir::W,
            Dir::W => Dir::N,
        };
    }
    fn turn_twice(&mut self) {
        self.turn();
        self.turn();
    }
    fn back(&mut self) -> Dir {
        let old = *self;
        self.turn_twice();
        old
    }
    fn dx(self) -> i32 {
        match self {
            Dir::E => 1,
            Dir::W => -1,
            _ => 0,
        }
    }
    fn is_vertical(&self) -> bool {
        matches!(self, Dir::N | Dir::S)
    }
}

trait Spin {
    fn spin(&mut self);
}

impl Spin for Dir {
    fn spin(&mut self) {
        self.turn();
    }
}

#[derive(Debug, Clone, Copy)]
enum Op {
    Add = 1,
    Mul = 10,
}

#[derive(Debug, Clone)]
struct Bot {
    d: Dir,
    x: i32,
    trail: Vec<Dir>,
}

impl Bot {
    fn step(&mut self) {
        self.d.turn();
        self.x += self.d.dx();
        self.trail.push(self.d);
    }
    fn dir_mut(&mut self) -> &mut Dir {
        &mut self.d
    }
}

fn set(r: &mut Dir, to: Dir) {
    *r = to;
}

fn set_any<T>(r: &mut T, to: T) {
    *r = to;
}

fn spin_all<T: Spin>(xs: &mut [T]) {
    for x in xs.iter_mut() {
        x.spin();
    }
}

fn first_mut(v: &mut Vec<Dir>) -> Option<&mut Dir> {
    v.first_mut()
}

fn main() {
    // Reads share the value; changing one place leaves the others alone.
    let a = Dir::N;
    let mut b = a;
    b.turn();
    let mut c = b;
    set(&mut c, Dir::W);
    println!("{:?} {:?} {:?}", a, b, c);

    // `&mut self` methods on a local, a field, an element, through `self`.
    let mut d = Dir::S;
    d.turn_twice();
    let old = d.back();
    d.spin();
    println!("{:?} {:?} {} {}", d, old, d.dx(), d.is_vertical());
    let mut bot = Bot {
        d: Dir::N,
        x: 0,
        trail: vec![],
    };
    for _ in 0..5 {
        bot.step();
    }
    bot.trail[0].turn();
    *bot.dir_mut() = Dir::S;
    bot.dir_mut().turn();
    let saved = bot.clone();
    bot.d.turn();
    println!("{:?} {:?}", bot, saved);

    // Elements: indexing, `&mut v[i]`, `iter_mut`, `for in &mut`, `get_mut`, `last_mut`.
    let mut v = vec![Dir::N; 4];
    let w = v.clone();
    set(&mut v[1], Dir::E);
    v[2].turn();
    v[3] = v[1];
    v[3].turn();
    for x in v.iter_mut() {
        x.turn();
    }
    for x in &mut v {
        if *x == Dir::S {
            *x = Dir::N;
        }
    }
    if let Some(x) = v.get_mut(0) {
        *x = Dir::W;
    }
    *v.last_mut().unwrap() = Dir::E;
    if let Some(x) = first_mut(&mut v) {
        x.turn();
    }
    spin_all(&mut v);
    println!("{:?} {:?}", v, w);

    // Arrays and grids.
    let mut grid = [[Dir::N; 3]; 2];
    grid[1][2] = Dir::W;
    grid[0][1].turn();
    set(&mut grid[1][0], Dir::S);
    let row = grid[0];
    grid[0][0].turn();
    println!("{:?} {:?}", grid, row);
    let mut vg = vec![vec![Dir::E; 2]; 2];
    vg[1][1].turn();
    set_any(&mut vg[0][0], Dir::W);
    println!("{:?}", vg);

    // Maps.
    let mut m: HashMap<&str, Dir> = HashMap::new();
    m.insert("a", Dir::N);
    m.get_mut("a").unwrap().turn();
    *m.entry("b").or_insert(Dir::S) = Dir::W;
    m.entry("c").or_default().turn();
    for d in m.values_mut() {
        d.turn();
    }
    let mut keys: Vec<_> = m.iter().map(|(k, d)| (*k, *d)).collect();
    keys.sort();
    println!("{:?}", keys);
    let mut bm: BTreeMap<Dir, Dir> = BTreeMap::new();
    bm.insert(Dir::E, Dir::E);
    for (_, d) in bm.iter_mut() {
        d.turn();
    }
    if let Some(d) = bm.get_mut(&Dir::E) {
        set(d, Dir::N);
    }
    let set_of: HashSet<Dir> = [Dir::N, Dir::N, Dir::E].into_iter().collect();
    println!("{:?} {}", bm, set_of.len());

    // `Option<Dir>`, `ref mut`, `match &mut`.
    let mut o = Some(Dir::N);
    let o2 = o;
    if let Some(x) = o.as_mut() {
        x.turn();
    }
    if let Some(ref mut x) = o {
        x.turn();
    }
    match &mut o {
        Some(x) => *x = Dir::W,
        None => {}
    }
    println!("{:?} {:?}", o, o2);

    // `mem` functions and generics.
    let mut p = Dir::N;
    let mut q = Dir::E;
    std::mem::swap(&mut p, &mut q);
    let r = std::mem::replace(&mut p, Dir::S);
    let t = std::mem::take(&mut q);
    set_any(&mut q, Dir::W);
    println!("{:?} {:?} {:?} {:?}", p, q, r, t);
    let mut pair = (Dir::N, Dir::S);
    pair.0.turn();
    std::mem::swap(&mut pair.0, &mut pair.1);
    println!("{:?}", pair);

    // Closures that change a captured one.
    let mut e = Dir::N;
    let mut go = || e.turn();
    go();
    go();
    println!("{:?}", e);

    // Equality, ordering, hashing, casts, `Default`, `fill`, `sort`.
    let mut ds = vec![Dir::W, Dir::N, Dir::S, Dir::E];
    ds.sort();
    let dd: Dir = Default::default();
    let mut f = [Dir::E; 3];
    f.fill(Dir::S);
    f[0].turn();
    println!(
        "{:?} {:?} {:?} {} {} {}",
        ds,
        dd,
        f,
        Dir::W as i32,
        Op::Mul as i32 + Op::Add as i32,
        ds.contains(&Dir::S)
    );
}
