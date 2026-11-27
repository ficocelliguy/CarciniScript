// `K::f()` / `T::f()` in methods with `self` of `impl<K: Tr, V: Tr> Tr for HashMap<K, V>` and
// `impl<T: Tr> Tr for HashSet<T>`, called directly and through a `T: Tr`; `Self::f()` in a method
// with `self` of an impl for `Vec<T>` / `HashMap<K, V>`; a blanket impl reached through
// `<Cell<u8>>::f()`; and `Trait::f(..)` whose `Self` is settled after the call (an impl's `T`, or
// `let v: Vec<u32> = ..`).
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

trait Schema {
    fn declaration() -> String;
    fn d(&self) -> String;
}

impl Schema for u8 {
    fn declaration() -> String {
        "u8".to_string()
    }
    fn d(&self) -> String {
        format!("u8={}", self)
    }
}

impl Schema for String {
    fn declaration() -> String {
        "String".to_string()
    }
    fn d(&self) -> String {
        format!("s={}", self)
    }
}

impl<T: Schema> Schema for Vec<T> {
    fn declaration() -> String {
        format!("Vec<{}>", T::declaration())
    }
    fn d(&self) -> String {
        format!("{} of {}", Self::declaration(), self.len())
    }
}

impl<K: Schema, V: Schema> Schema for HashMap<K, V> {
    fn declaration() -> String {
        format!("Map<{}, {}>", K::declaration(), V::declaration())
    }
    fn d(&self) -> String {
        format!("{} / {} / {}", K::declaration(), Self::declaration(), self.len())
    }
}

impl<T: Schema> Schema for HashSet<T> {
    fn declaration() -> String {
        format!("Set<{}>", T::declaration())
    }
    fn d(&self) -> String {
        format!("{}: {}", T::declaration(), self.len())
    }
}

impl<T: Schema + Copy> Schema for Cell<T> {
    fn declaration() -> String {
        format!("Cell<{}>", T::declaration())
    }
    fn d(&self) -> String {
        self.get().d()
    }
}

fn show<T: Schema>(t: &T) -> String {
    format!("[{}]", t.d())
}

trait Twice {
    fn twice() -> String;
}

impl<Z: Schema> Twice for Z {
    fn twice() -> String {
        Z::declaration() + "!"
    }
}

trait Decode: Sized {
    fn decode(data: &mut &[u8]) -> Self;
}

fn get(b: &mut &[u8]) -> u8 {
    let r = b[0];
    *b = &b[1..];
    r
}

impl Decode for u32 {
    fn decode(data: &mut &[u8]) -> Self {
        get(data) as u32
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode(data: &mut &[u8]) -> Self {
        let n = u32::decode(data);
        let mut v = Vec::new();
        for _ in 0..n {
            v.push(Decode::decode(data));
        }
        v
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode(data: &mut &[u8]) -> Self {
        match get(data) {
            0 => None,
            _ => Some(Decode::decode(data)),
        }
    }
}

fn dec<T: Decode>(data: &mut &[u8]) -> T {
    Decode::decode(data)
}

fn main() {
    let v: Vec<u8> = vec![1, 2];
    let mut m: HashMap<String, u8> = HashMap::new();
    m.insert("a".into(), 1);
    let s: HashSet<u8> = [3u8].into_iter().collect();
    println!("{}", v.d());
    println!("{}", m.d());
    println!("{}", s.d());
    println!("{} {} {}", show(&v), show(&m), show(&s));
    println!("{}", show(&Cell::new(4u8)));
    println!("{} {} {}", <Vec<u8>>::twice(), <Cell<u8>>::twice(), u8::twice());
    println!("{}", <HashMap<u8, String>>::twice());
    let mut d: &[u8] = &[2, 7, 8, 1, 9, 3, 0, 1, 5, 1, 6];
    let x: Vec<u32> = Decode::decode(&mut d);
    let o: Option<u32> = dec(&mut d);
    let w: Vec<Option<u32>> = Decode::decode(&mut d);
    println!("{:?} {:?} {:?}", x, o, w);
}
