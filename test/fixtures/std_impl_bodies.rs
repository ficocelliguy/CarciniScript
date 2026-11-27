// The bodies of `Deref`, `AsRef`, `Borrow`, `Drop`, `DoubleEndedIterator`, `ExactSizeIterator`,
// `IntoIterator` and `Hasher` impls are checked like any other: tuples inside them keep their
// element types (`opt.unwrap().0`, `.map(|p| p.0)`).
use std::borrow::Borrow;
use std::hash::Hasher;
use std::ops::Deref;

struct Pairs {
    items: Vec<(u8, u16)>,
    front: usize,
    back: usize,
}

impl Iterator for Pairs {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        if self.front == self.back {
            return None;
        }
        self.front += 1;
        self.items.get(self.front - 1).map(|p| p.0)
    }
}

impl DoubleEndedIterator for Pairs {
    fn next_back(&mut self) -> Option<u8> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.items.get(self.back).map(|p: &(u8, u16)| p.0)
    }
}

impl ExactSizeIterator for Pairs {
    fn len(&self) -> usize {
        self.items[self.front..self.back].iter().map(|p| p.1 as usize).count()
    }
}

struct Wrap(Option<(String, u8)>);

impl Deref for Wrap {
    type Target = String;
    fn deref(&self) -> &String {
        &self.0.as_ref().unwrap().0
    }
}

impl AsRef<str> for Wrap {
    fn as_ref(&self) -> &str {
        self.0.as_ref().map(|p| p.0.as_str()).unwrap_or("")
    }
}

impl Borrow<str> for Wrap {
    fn borrow(&self) -> &str {
        &self.0.as_ref().unwrap().0
    }
}

impl Drop for Wrap {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            println!("drop {} {}", p.0, p.1);
        }
    }
}

struct Sum(u64);

impl Hasher for Sum {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let pair = (bytes.len(), bytes.iter().map(|&b| b as u64).sum::<u64>());
        self.0 += pair.0 as u64 + pair.1;
    }
}

fn main() {
    let items = vec![(1, 10), (2, 20), (3, 30)];
    let p = Pairs { back: items.len(), items, front: 0 };
    println!("{}", p.len());
    let v: Vec<u8> = p.rev().collect();
    println!("{v:?}");
    let w = Wrap(Some(("hi".to_string(), 7)));
    let b: &str = w.borrow();
    println!("{} {} {}", w.len(), w.as_ref(), b);
    let mut h = Sum(0);
    h.write(&[1, 2, 3]);
    println!("{}", h.finish());
}
