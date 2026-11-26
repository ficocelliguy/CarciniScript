// A `Copy` iterator consumed by value (`l.take(4)`, `for x in c`) works on a copy, where
// `by_ref()` / `&mut c` advance the original; reading a `const` / `static` array of `Copy`
// values copies it, so changing the copy leaves the item alone.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Light {
    Red,
    Green,
    Yellow,
}

impl Iterator for Light {
    type Item = Light;
    fn next(&mut self) -> Option<Light> {
        let cur = *self;
        *self = match self {
            Light::Red => Light::Green,
            Light::Green => Light::Yellow,
            Light::Yellow => Light::Red,
        };
        Some(cur)
    }
}

#[derive(Clone, Copy, Debug)]
struct Count {
    n: u32,
}

impl Iterator for Count {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.n >= 20 {
            None
        } else {
            self.n += 1;
            Some(self.n)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Dir {
    N,
    E,
}

#[derive(Clone, Copy, Debug)]
struct P {
    x: i32,
}

static ORDER: [Light; 3] = [Light::Red, Light::Green, Light::Yellow];
const PS: [P; 2] = [P { x: 1 }, P { x: 2 }];
static ONE: P = P { x: 7 };
const DIRS: [Dir; 2] = [Dir::N, Dir::E];
const GRID: [[u8; 2]; 2] = [[1, 2], [3, 4]];

fn main() {
    let mut l = Light::Red;
    l.next();
    let v: Vec<Light> = l.take(4).collect();
    println!("{:?} {:?}", l, v);

    let c = Count { n: 0 };
    let s: u32 = c.take(3).sum();
    for x in c.take(2) {
        print!("{} ", x);
    }
    for x in c {
        if x > 2 {
            break;
        }
        print!("{} ", x);
    }
    let all: Vec<u32> = c.skip(18).collect();
    println!("{:?} {} {:?}", c, s, all);

    let mut c2 = Count { n: 10 };
    let firsts: Vec<u32> = c2.by_ref().take(2).collect();
    let more: Vec<u32> = (&mut c2).take(1).collect();
    for x in &mut c2 {
        if x >= 15 {
            break;
        }
    }
    println!("{:?} {:?} {:?}", firsts, more, c2);

    let mut w = ORDER;
    w[0].next();
    let mut ps = PS;
    ps[0].x = 100;
    let mut one = ONE;
    one.x = 70;
    let mut ds = DIRS;
    ds[1] = Dir::N;
    let mut g = GRID;
    g[1][0] = 30;
    let mut row = GRID[0];
    row[1] = 20;
    println!("{:?} {:?} {:?} {:?}", w, ORDER, ps, PS);
    println!("{:?} {:?} {:?} {:?}", one, ONE, ds, DIRS);
    println!("{:?} {:?} {:?}", g, row, GRID);
    let mut q = PS[1];
    q.x = 5;
    println!("{:?} {:?}", q, PS);
}
