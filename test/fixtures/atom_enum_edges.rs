// More `&mut` shapes over an immutable enum (see atom_enum.rs): closures over `iter_mut`, operator
// impls, `dyn` calls, pattern bindings through `&mut self`, `&mut` fields and `get_or_insert`.
use std::ops::AddAssign;
#[derive(Clone, Copy, PartialEq, Debug)]
enum Dir {
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
}
impl AddAssign<i32> for Dir {
    fn add_assign(&mut self, n: i32) {
        for _ in 0..n {
            self.turn();
        }
    }
}
trait Spin {
    fn spin(&mut self);
    fn name(&self) -> String;
}
impl Spin for Dir {
    fn spin(&mut self) {
        self.turn();
    }
    fn name(&self) -> String {
        format!("{:?}", self)
    }
}
struct Bot {
    d: Dir,
    ds: Vec<Dir>,
}
impl Bot {
    fn go(&mut self) {
        match self {
            Bot { d, .. } => d.turn(),
        }
        let Bot { d, ds } = self;
        *d = Dir::W;
        ds.push(*d);
        if let Some(x) = self.ds.get_mut(0) {
            x.turn();
        }
        let i = 0;
        self.at(i).turn();
        *self.at(i) = Dir::S;
    }
    fn at(&mut self, i: usize) -> &mut Dir {
        &mut self.ds[i]
    }
}
struct R<'a> {
    d: &'a mut Dir,
}
fn main() {
    let mut v = vec![Dir::N, Dir::E, Dir::S];
    v.iter_mut().for_each(|d| d.turn());
    v.iter_mut()
        .filter(|d| **d == Dir::E)
        .for_each(|d| *d = Dir::N);
    for (i, d) in v.iter_mut().enumerate() {
        if i == 2 {
            *d += 2;
        }
    }
    println!("{:?}", v);
    let mut d = Dir::N;
    d += 3;
    let mut b: Box<dyn Spin> = Box::new(d);
    b.spin();
    println!("{:?} {}", d, b.name());
    let mut spinners: Vec<Box<dyn Spin>> = vec![Box::new(Dir::N), Box::new(Dir::S)];
    for s in spinners.iter_mut() {
        s.spin();
    }
    println!(
        "{}",
        spinners
            .iter()
            .map(|s| s.name())
            .collect::<Vec<_>>()
            .join(",")
    );
    let mut bot = Bot {
        d: Dir::N,
        ds: vec![Dir::E],
    };
    bot.go();
    println!("{:?} {:?}", bot.d, bot.ds);
    let mut x = Dir::N;
    let mut y = Dir::E;
    {
        let (a, b2) = (&mut x, &mut y);
        *a = Dir::S;
        b2.turn();
    }
    let r = R { d: &mut x };
    r.d.turn();
    *r.d = Dir::E;
    println!("{:?} {:?}", x, y);
    let mut it = v.iter_mut();
    while let Some(d) = it.next() {
        d.turn();
    }
    let mut o: Option<Dir> = None;
    o.get_or_insert(Dir::N).turn();
    let z = o.take();
    println!("{:?} {:?} {:?}", v, o, z);
    let mut taken = Bot {
        d: Dir::W,
        ds: vec![],
    };
    let old = std::mem::replace(&mut taken.d, Dir::N);
    println!("{:?} {:?}", old, taken.d);
}
