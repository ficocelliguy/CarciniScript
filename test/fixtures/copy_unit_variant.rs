// A copy of a unit variant, or of an `Option` / `Result` holding a `Copy` value, is its own value:
// writing through a `&mut` to the copy (a cell over the place for a fieldless `Copy` enum, the
// object rewritten in place otherwise) leaves the original alone; so do changes to `mut self` in a
// method on a `Copy` type.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Dir {
    N,
    E,
    S,
    W,
}

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    Circle(f64),
    Empty,
}

#[derive(Clone, Copy, Debug)]
struct P {
    d: Dir,
    k: i32,
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
}

#[derive(Clone, Copy, Debug)]
struct Q {
    k: i32,
}

impl Q {
    fn bump(mut self) -> Q {
        self.k += 1;
        self
    }
}

impl Dir {
    fn flip(mut self) -> Dir {
        self = match self {
            Dir::N => Dir::S,
            _ => Dir::N,
        };
        self
    }
}

fn set(r: &mut Dir) {
    *r = Dir::W;
}

fn set_shape(r: &mut Shape) {
    *r = Shape::Circle(1.0);
}

fn main() {
    let a = Dir::N;
    let mut b = a;
    set(&mut b);
    let c = Dir::E;
    let mut e = c.clone();
    set(&mut e);
    println!("{:?} {:?} {:?} {:?}", a, b, c, e);

    let s = Shape::Empty;
    let mut t = s.clone();
    set_shape(&mut t);
    println!("{:?} {:?}", s, t);

    let p = P { d: Dir::S, k: 1 };
    let mut q = p;
    set(&mut q.d);
    q.k = 2;
    println!("{:?} {:?}", p, q);

    let mut arr = [Dir::N; 2];
    set(&mut arr[0]);
    let mut v = vec![Dir::N; 3];
    set(&mut v[1]);
    println!("{:?} {:?}", arr, v);

    let mut x = Dir::N;
    let mut y = Dir::E;
    std::mem::swap(&mut x, &mut y);
    let z = x;
    set(&mut x);
    println!("{:?} {:?} {:?}", x, y, z);

    let mut v = vec![Dir::N, Dir::E];
    let w = v.clone();
    for d in v.iter_mut() {
        *d = Dir::S;
    }
    println!("{:?} {:?}", v, w);

    let mut f = Dir::W;
    let g = f;
    f.turn();
    f.turn();
    println!("{:?} {:?}", f, g);

    let o = Some(Dir::N);
    let mut o2 = o;
    if let Some(d) = o2.as_mut() {
        *d = Dir::W;
    }
    println!("{:?} {:?}", o, o2);

    let o = Some(P { d: Dir::N, k: 1 });
    let mut o2 = o;
    if let Some(p) = o2.as_mut() {
        p.k = 5;
    }
    println!("{:?} {:?}", o, o2);

    let r: Result<P, ()> = Ok(P { d: Dir::E, k: 1 });
    let mut r2 = r;
    if let Ok(p) = r2.as_mut() {
        p.k = 7;
    }
    println!("{:?} {:?}", r, r2);

    let r: Result<i32, ()> = Ok(1);
    let mut r2 = r;
    if let Ok(ref mut x) = r2 {
        *x += 10;
    }
    println!("{:?} {:?}", r, r2);

    // A `mut self` method on a `Copy` value works on a copy.
    let q = Q { k: 1 };
    let q2 = q.bump();
    let h = Dir::N;
    let h2 = h.flip();
    println!("{:?} {:?} {:?} {:?}", q, q2, h, h2);
}
