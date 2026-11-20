// Your traits on std types: `Cell`, iterator adapters, `Instant`, `Wrapping`, `LinkedList`,
// `ControlFlow`, `CStr` / `CString`; and `PartialOrd<Bytes> for str` under the operators.
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::LinkedList;
use std::collections::hash_map::Keys;
use std::ffi::{CStr, CString};
use std::iter::Peekable;
use std::num::Wrapping;
use std::ops::ControlFlow;
use std::time::Instant;

trait Describe {
    fn describe(&self) -> String;
}
impl Describe for Cell<i32> {
    fn describe(&self) -> String {
        format!("cell {}", self.get())
    }
}
impl<I: Iterator<Item = i32>> Describe for Peekable<I> {
    fn describe(&self) -> String {
        "peekable".into()
    }
}
impl Describe for Instant {
    fn describe(&self) -> String {
        "now".into()
    }
}
impl Describe for Wrapping<u8> {
    fn describe(&self) -> String {
        format!("wrap {}", self.0)
    }
}
impl Describe for LinkedList<i32> {
    fn describe(&self) -> String {
        format!("list {} {:?}", self.len(), self)
    }
}
impl Describe for ControlFlow<i32, ()> {
    fn describe(&self) -> String {
        match self {
            ControlFlow::Break(b) => format!("break {}", b),
            ControlFlow::Continue(()) => "cont".into(),
        }
    }
}
impl Describe for CStr {
    fn describe(&self) -> String {
        format!("cstr {}", self.to_bytes().len())
    }
}
impl Describe for CString {
    fn describe(&self) -> String {
        format!("cstring {:?}", self)
    }
}
fn show<T: Describe>(t: &T) -> String {
    t.describe()
}
fn keys_len(k: Keys<'_, String, i32>) -> usize {
    k.len()
}

#[derive(Debug, PartialEq, PartialOrd)]
struct Bytes(Vec<u8>);
impl PartialEq<Bytes> for str {
    fn eq(&self, o: &Bytes) -> bool {
        self.as_bytes() == &o.0[..]
    }
}
impl PartialOrd<Bytes> for str {
    fn partial_cmp(&self, o: &Bytes) -> Option<Ordering> {
        self.as_bytes().partial_cmp(&o.0[..])
    }
}
impl PartialEq<str> for Bytes {
    fn eq(&self, o: &str) -> bool {
        &self.0[..] == o.as_bytes()
    }
}
impl PartialOrd<str> for Bytes {
    fn partial_cmp(&self, o: &str) -> Option<Ordering> {
        self.0[..].partial_cmp(o.as_bytes())
    }
}

fn main() {
    println!("{} {}", Cell::new(7).describe(), show(&Cell::new(8)));
    let p = vec![1, 2].into_iter().peekable();
    println!("{} {}", p.describe(), show(&vec![3].into_iter().peekable()));
    let mut m = std::collections::HashMap::new();
    m.insert("a".to_string(), 1);
    println!("{}", keys_len(m.keys()));
    println!("{}", Instant::now().describe());
    println!("{}", (Wrapping(250u8) + Wrapping(10)).describe());
    let mut l = LinkedList::new();
    l.push_back(1);
    l.push_back(2);
    l.push_front(0);
    println!("{} {}", l.describe(), show(&l));
    println!("{:?} {:?} {} {}", l.pop_front(), l.back(), l.contains(&2), l.is_empty());
    let mut other: LinkedList<i32> = (5..7).collect();
    l.append(&mut other);
    println!("{:?} {} {:?}", l, other.len(), l.iter().map(|x| x * 10).collect::<Vec<_>>());
    let c: ControlFlow<i32, ()> = ControlFlow::Break(3);
    println!("{} {}", c.describe(), ControlFlow::<i32, ()>::Continue(()).describe());
    println!(
        "{} {:?} {:?}",
        c.is_break(),
        c.break_value(),
        ControlFlow::<i32>::Continue(()).continue_value()
    );
    let s = CString::new("abc").unwrap();
    println!("{} {}", s.describe(), s.as_c_str().describe());
    println!(
        "{:?} {:?}",
        s.as_bytes(),
        CString::new(vec![1u8, 0, 2]).map(|_| ()).unwrap_err().nul_position()
    );
    let b = Bytes(vec![98]);
    println!("{} {} {}", *"a" < b, *"b" == b, *"c" > b);
    println!("{} {} {} {}", b > *"a", b == *"b", b <= *"b", b >= *"c");
}
