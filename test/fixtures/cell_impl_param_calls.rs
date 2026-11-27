// `T::f()` / `T::CONST` inside `impl<T: Tr> Tr for Cell<T>` and `RefCell<T>` (and a `HashMap<K,
// V, S>` / blanket impl beside them), in associated functions and in methods with `self`.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::BuildHasher;

trait Schema {
    const SIZE: usize;
    fn declaration() -> String;
    fn describe(&self) -> String;
}

impl Schema for u8 {
    const SIZE: usize = 1;
    fn declaration() -> String {
        "u8".to_string()
    }
    fn describe(&self) -> String {
        format!("u8 {}", self)
    }
}

impl Schema for String {
    const SIZE: usize = 24;
    fn declaration() -> String {
        "String".to_string()
    }
    fn describe(&self) -> String {
        format!("str {}", self)
    }
}

impl<T: Schema + Copy> Schema for Cell<T> {
    const SIZE: usize = 8;
    fn declaration() -> String {
        format!("Cell<{}>", T::declaration())
    }
    fn describe(&self) -> String {
        format!("cell of {} ({})", T::declaration(), self.get().describe())
    }
}

trait Other {
    fn name() -> String;
    fn show(&self) -> String;
}

impl Other for u8 {
    fn name() -> String {
        "u8".into()
    }
    fn show(&self) -> String {
        format!("<{}>", self)
    }
}

impl<T: Other> Other for RefCell<T> {
    fn name() -> String {
        format!("RefCell<{}>", T::name())
    }
    fn show(&self) -> String {
        format!("{} holding {}", T::name(), self.borrow().show())
    }
}

impl<K: Schema, V: Schema, S: BuildHasher> Schema for HashMap<K, V, S> {
    const SIZE: usize = 48;
    fn declaration() -> String {
        format!("{}->{}", K::declaration(), V::declaration())
    }
    fn describe(&self) -> String {
        format!("{} entries", self.len())
    }
}

trait Twice {
    fn twice() -> String;
}

impl<Z: Schema> Twice for Z {
    fn twice() -> String {
        Z::declaration() + &Z::declaration()
    }
}

fn decl<T: Schema>(t: &T) -> String {
    format!("{} / {} / {}", T::declaration(), t.describe(), T::SIZE)
}

fn main() {
    println!("{} {}", <Cell<u8>>::declaration(), <Cell<u8> as Schema>::SIZE);
    let c = Cell::new(5u8);
    println!("{}", c.describe());
    println!("{}", decl(&c));
    let r = RefCell::new(9u8);
    println!("{} {}", <RefCell<u8> as Other>::name(), r.show());
    let mut m: HashMap<String, u8> = HashMap::new();
    m.insert("a".into(), 1);
    println!("{} {}", <HashMap<String, u8>>::declaration(), m.describe());
    println!("{}", decl(&m));
    println!("{}", u8::twice());
}
