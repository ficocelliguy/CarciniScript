/// A fieldless `Copy` enum in a `mod` file: shared values, `&mut` through cells.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Dir {
    N,
    E,
    S,
    W,
}

pub trait Spin {
    fn spin(&mut self);
}

impl Dir {
    pub fn turn(&mut self) {
        *self = match self {
            Dir::N => Dir::E,
            Dir::E => Dir::S,
            Dir::S => Dir::W,
            Dir::W => Dir::N,
        };
    }
}

impl Spin for Dir {
    fn spin(&mut self) {
        self.turn();
        self.turn();
    }
}

pub fn set(r: &mut Dir, to: Dir) {
    *r = to;
}

pub fn spin_all<T: Spin>(xs: &mut [T]) {
    for x in xs.iter_mut() {
        x.spin();
    }
}
