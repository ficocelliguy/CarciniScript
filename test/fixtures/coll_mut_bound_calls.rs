// A `&mut self` method of an impl for `HashMap<K, V>` / `HashSet<T>` / `Option<T>` reached
// through a `T: Tr` bound (`t.bump()` in `fn go<T: Tr>(t: &mut T)`), using `K::f()` / `T::f()`
// and `Self::f()` and assigning `*self`; and `if let Some(v) = o` / `match o` over a `&mut
// Option<T>` handing `v` to a number's `&mut self` method, which writes the option back.
use std::collections::{HashMap, HashSet};

trait Sch {
    fn decl() -> String;
    fn bump(&mut self) -> String;
}

impl Sch for u8 {
    fn decl() -> String {
        "u8".into()
    }
    fn bump(&mut self) -> String {
        *self += 1;
        format!("u8={}", self)
    }
}

impl Sch for String {
    fn decl() -> String {
        "String".into()
    }
    fn bump(&mut self) -> String {
        self.push('!');
        self.clone()
    }
}

impl<K: Sch, V: Sch> Sch for HashMap<K, V> {
    fn decl() -> String {
        format!("Map<{}, {}>", K::decl(), V::decl())
    }
    fn bump(&mut self) -> String {
        let n = self.len();
        for v in self.values_mut() {
            v.bump();
        }
        format!("{} {} {}", K::decl(), Self::decl(), n)
    }
}

impl<T: Sch> Sch for HashSet<T> {
    fn decl() -> String {
        format!("Set<{}>", T::decl())
    }
    fn bump(&mut self) -> String {
        format!("{} {}", T::decl(), self.len())
    }
}

impl<T: Sch + Default> Sch for Option<T> {
    fn decl() -> String {
        format!("Option<{}>", T::decl())
    }
    fn bump(&mut self) -> String {
        let d = T::decl();
        match self {
            Some(v) => {
                v.bump();
            }
            None => {
                *self = Some(T::default());
            }
        }
        d
    }
}

fn go<T: Sch>(t: &mut T) -> String {
    t.bump()
}

fn twice<T: Sch>(mut t: T) -> (String, T) {
    let s = t.bump();
    (s, t)
}

fn bump_inner<T: Sch>(o: &mut Option<T>) {
    if let Some(v) = o {
        v.bump();
    }
}

fn bump_match<T: Sch>(o: &mut Option<T>) {
    match o {
        Some(v) => {
            v.bump();
        }
        None => {}
    }
}

fn main() {
    let mut m: HashMap<String, u8> = HashMap::new();
    m.insert("a".into(), 1);
    println!("{}", go(&mut m));
    println!("{}", m["a"]);
    let mut s: HashSet<u8> = HashSet::new();
    s.insert(3);
    println!("{}", go(&mut s));
    let (r, m2) = twice(m);
    println!("{} {}", r, m2["a"]);

    let mut o: Option<u8> = None;
    println!("{} {:?}", go(&mut o), o);
    println!("{} {:?}", go(&mut o), o);
    bump_inner(&mut o);
    bump_match(&mut o);
    println!("{:?}", o);
    let mut so: Option<String> = Some("hi".into());
    bump_inner(&mut so);
    println!("{} {:?}", go(&mut so), so);
}
