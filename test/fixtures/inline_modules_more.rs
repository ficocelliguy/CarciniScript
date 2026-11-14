// More inline modules: a trait declared in a module and implemented outside it (also as
// `dyn`), `use shapes::Kind::*` through a module into an enum, a generic struct, nested consts
// and statics, and a type alias in a module.
mod shapes {
    pub trait Area {
        fn area(&self) -> f64;
        fn describe(&self) -> String {
            format!("area {:.1}", self.area())
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum Kind {
        Round,
        Boxy(u32),
    }

    pub struct Stack<T> {
        pub items: Vec<T>,
    }

    impl<T: Clone> Stack<T> {
        pub fn new() -> Self {
            Stack { items: Vec::new() }
        }
        pub fn push(&mut self, t: T) {
            self.items.push(t);
        }
    }

    pub mod consts {
        pub const PI: f64 = 3.0;
        pub static NAMES: [&str; 2] = ["a", "b"];
    }

    pub type Pair = (i32, i32);
}

use shapes::Kind::*;
use shapes::{Area, Kind};

struct Circle(f64);

impl shapes::Area for Circle {
    fn area(&self) -> f64 {
        shapes::consts::PI * self.0 * self.0
    }
}

fn kind_of(k: &Kind) -> u32 {
    match k {
        Round => 0,
        Boxy(n) => *n,
        #[allow(unreachable_patterns)]
        _ => 9,
    }
}

fn main() {
    let c = Circle(2.0);
    println!("{} {}", c.area(), c.describe());
    let ks = vec![Kind::Round, Boxy(3), shapes::Kind::Boxy(4)];
    let total: u32 = ks.iter().map(kind_of).sum();
    println!("{:?} {}", ks, total);
    let mut s: shapes::Stack<i32> = shapes::Stack::new();
    s.push(1);
    s.push(2);
    let p: shapes::Pair = (s.items[0], s.items[1]);
    println!("{:?} {:?} {}", p, shapes::consts::NAMES, Round == Kind::Round);
    let d: Box<dyn Area> = Box::new(Circle(1.0));
    println!("{}", d.describe());
}
