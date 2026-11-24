// std iterator structs named as field and return types (`btree_map::Keys`, `btree_set::Difference`,
// `vec::Drain`, `slice::Iter`) inside wrappers of the same name, and B-tree keys bounded by a
// generic parameter's `Ord` (in the impl, a `where` clause, or a trait with `Ord` as a supertrait).
use std::collections::btree_map::{self, BTreeMap};
use std::collections::btree_set::{self, BTreeSet};
use std::slice;

pub struct Map<K, V> {
    inner: BTreeMap<K, V>,
}

pub struct Keys<'a, K, V> {
    inner: btree_map::Keys<'a, K, V>,
}

pub struct Values<'a, K, V> {
    inner: btree_map::Values<'a, K, V>,
}

pub struct ValuesMut<'a, K, V> {
    inner: btree_map::ValuesMut<'a, K, V>,
}

pub struct IntoKeys<K, V> {
    inner: btree_map::IntoKeys<K, V>,
}

impl<K: Ord, V> Map<K, V> {
    pub fn new() -> Self {
        Map { inner: BTreeMap::new() }
    }
    pub fn insert(&mut self, k: K, v: V) {
        self.inner.insert(k, v);
    }
    pub fn keys(&self) -> Keys<'_, K, V> {
        Keys {
            inner: self.inner.keys(),
        }
    }
    pub fn values(&self) -> Values<'_, K, V> {
        Values {
            inner: self.inner.values(),
        }
    }
    pub fn values_mut(&mut self) -> ValuesMut<'_, K, V> {
        ValuesMut {
            inner: self.inner.values_mut(),
        }
    }
    pub fn into_keys(self) -> IntoKeys<K, V> {
        IntoKeys {
            inner: self.inner.into_keys(),
        }
    }
    pub fn first(&self) -> Option<&K> {
        let Keys { inner } = self.keys();
        let mut it = inner;
        it.next()
    }
}

impl<'a, K, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;
    fn next(&mut self) -> Option<&'a K> {
        self.inner.next()
    }
}

impl<'a, K, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;
    fn next(&mut self) -> Option<&'a V> {
        self.inner.next()
    }
}

impl<K, V> Iterator for IntoKeys<K, V> {
    type Item = K;
    fn next(&mut self) -> Option<K> {
        self.inner.next()
    }
}

pub struct Set<T> {
    inner: BTreeSet<T>,
}

pub struct Difference<'a, T> {
    inner: btree_set::Difference<'a, T>,
}

impl<T> Set<T>
where
    T: Ord,
{
    pub fn from_vec(v: Vec<T>) -> Self {
        Set {
            inner: v.into_iter().collect(),
        }
    }
    pub fn difference<'a>(&'a self, other: &'a Set<T>) -> Difference<'a, T> {
        Difference {
            inner: self.inner.difference(&other.inner),
        }
    }
    pub fn count(&self, other: &Set<T>) -> usize {
        self.difference(other).inner.count()
    }
}

pub struct Slab<T> {
    entries: Vec<T>,
    len: usize,
}

pub struct Drain<'a, T> {
    inner: std::vec::Drain<'a, T>,
    len: usize,
}

impl<T> Slab<T> {
    pub fn drain(&mut self) -> Drain<'_, T> {
        let old_len = self.len;
        self.len = 0;
        Drain {
            inner: self.entries.drain(..),
            len: old_len,
        }
    }
}

impl<'a, T> Iterator for Drain<'a, T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        self.len = self.len.saturating_sub(1);
        self.inner.next()
    }
}

pub struct Iter<'a, T> {
    inner: slice::Iter<'a, T>,
}

pub fn evens<T>(v: &[T]) -> Iter<'_, T> {
    Iter { inner: v.iter() }
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        let x = self.inner.next();
        self.inner.next();
        x
    }
}

pub trait Key: Ord + Copy {}
impl Key for u8 {}

pub fn largest<K: Key>(m: &BTreeMap<K, i32>) -> Option<K> {
    m.keys().next_back().copied()
}

fn main() {
    let mut m = Map::new();
    m.insert("b", 2);
    m.insert("a", 1);
    m.insert("c", 3);
    println!("{:?}", m.first());
    println!("{:?}", m.keys().collect::<Vec<_>>());
    println!("{:?}", m.values().map(|v| v * 10).collect::<Vec<_>>());
    println!("{}", m.values_mut().inner.count());
    println!("{:?}", m.into_keys().collect::<Vec<_>>());

    let a = Set::from_vec(vec![1, 2, 3, 4, 5]);
    let b = Set::from_vec(vec![2, 4]);
    println!("{} {:?}", a.count(&b), a.difference(&b).inner.collect::<Vec<_>>());

    let mut s = Slab { entries: vec![10, 20, 30], len: 3 };
    let d = s.drain();
    println!("{:?} {}", d.collect::<Vec<_>>(), s.entries.len());

    println!("{:?}", evens(&[1, 2, 3, 4, 5]).collect::<Vec<_>>());

    let mut k: BTreeMap<u8, i32> = BTreeMap::new();
    k.insert(3, 0);
    k.insert(9, 0);
    println!("{:?}", largest(&k));
}
