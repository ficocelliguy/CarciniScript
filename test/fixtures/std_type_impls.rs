// Trait impls on std types: your traits on `Option`, `Result`, `HashMap`, `BTreeMap`,
// `HashSet` and `u128` (with default methods, generic impls, and through a generic `T: Tr`),
// and `From` impls into `String` and numbers (`String::from(x)`, `.into()`, `impl Into<String>`).
use std::collections::{BTreeMap, HashMap, HashSet};
trait Describe {
    fn describe(&self) -> String;
    fn shout(&self) -> String {
        self.describe().to_uppercase()
    }
}
impl<T: std::fmt::Debug> Describe for Option<T> {
    fn describe(&self) -> String {
        match self {
            Some(v) => format!("some {v:?}"),
            None => "none".to_string(),
        }
    }
}
impl<T: std::fmt::Debug, E: std::fmt::Display> Describe for Result<T, E> {
    fn describe(&self) -> String {
        match self {
            Ok(v) => format!("ok {v:?}"),
            Err(e) => format!("err {e}"),
        }
    }
}
trait Total {
    fn total(&self) -> i64;
}
impl Total for HashMap<String, i64> {
    fn total(&self) -> i64 {
        self.values().sum()
    }
}
impl Total for BTreeMap<u32, i64> {
    fn total(&self) -> i64 {
        self.values().sum::<i64>() * 10
    }
}
impl Total for HashSet<i64> {
    fn total(&self) -> i64 {
        self.iter().sum()
    }
}
impl Total for u128 {
    fn total(&self) -> i64 {
        (*self % 1000) as i64
    }
}
fn sum_all<T: Total>(xs: &[T]) -> i64 {
    xs.iter().map(|x| x.total()).sum()
}
#[derive(Debug, Clone, Copy)]
struct Id(u32);
struct Host {
    name: &'static str,
    ram: u32,
}
impl From<Id> for String {
    fn from(id: Id) -> String {
        format!("id-{}", id.0)
    }
}
impl From<&Host> for String {
    fn from(h: &Host) -> String {
        format!("{}({}GB)", h.name, h.ram)
    }
}
impl From<Id> for u64 {
    fn from(id: Id) -> u64 {
        id.0 as u64 * 1000
    }
}
fn show(s: impl Into<String>) -> String {
    s.into()
}
fn main() {
    let a: Option<i32> = Some(3);
    let b: Option<i32> = None;
    println!("{} {} {}", a.describe(), b.describe(), a.shout());
    let r: Result<u8, String> = Err("bad".into());
    println!("{}", r.describe());
    let mut m: HashMap<String, i64> = HashMap::new();
    m.insert("a".to_string(), 4);
    m.insert("b".to_string(), 5);
    let mut bt = BTreeMap::new();
    bt.insert(1u32, 2i64);
    let s: HashSet<i64> = [1, 2, 3].into_iter().collect();
    println!("{} {} {} {}", m.total(), bt.total(), s.total(), 123456u128.total());
    println!("{}", sum_all(&[m.clone(), m]));
    let a = String::from(Id(7));
    let b: String = Id(8).into();
    let h = Host { name: "n00dles", ram: 4 };
    let c = String::from(&h);
    let d: u64 = Id(3).into();
    let e = String::from("plain");
    println!("{a} {b} {c} {d} {e} {}", show(Id(9)));
}
