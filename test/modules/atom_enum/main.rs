// An immutable enum declared in a `mod` file: its `&mut self` methods, `&mut` parameters and
// generic `&mut T` calls from the file that loads it go through cells.
mod dir;

use dir::{Dir, Spin, set, spin_all};

struct Bot {
    d: Dir,
}

fn main() {
    let a = Dir::N;
    let mut b = a;
    b.turn();
    let mut bot = Bot { d: b };
    bot.d.spin();
    set(&mut b, Dir::W);
    let mut v = vec![a, b, bot.d];
    v[0].turn();
    spin_all(&mut v);
    for d in v.iter_mut() {
        set(d, Dir::S);
        d.turn();
    }
    println!("{:?} {:?} {:?} {:?}", a, b, bot.d, v);
}
