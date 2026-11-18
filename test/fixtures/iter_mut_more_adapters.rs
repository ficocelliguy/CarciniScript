// More `iter_mut()` outside a `for` loop: elements found in (or taken from) a `&mut` iterator of
// structs (`if let Some(g) = v.iter_mut().find(..)`, `let mut it = v.iter_mut(); it.next()`),
// `try_for_each` / `try_fold` / `find_map` over numbers, `&mut` iterators returned from functions
// and methods, passed as parameters and kept in fields, a map's `values_mut()` / `iter_mut()`
// with adapters, `VecDeque::iter_mut().for_each(..)`, and `let (src, dst) = (a, &mut *b);`.
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Debug)]
struct Group {
    id: u32,
    args: Vec<u32>,
}

#[derive(Debug, Default)]
struct H {
    name: u8,
}

struct Bag {
    nums: Vec<i32>,
    names: Vec<String>,
    groups: Vec<Group>,
}

impl Bag {
    fn nums_mut(&mut self) -> std::slice::IterMut<'_, i32> {
        self.nums.iter_mut()
    }
    fn names_mut(&mut self) -> impl Iterator<Item = &mut String> {
        self.names.iter_mut().filter(|s| !s.is_empty())
    }
    fn groups_mut(&mut self) -> impl Iterator<Item = &mut Group> + '_ {
        self.groups.iter_mut()
    }
    fn boxed(&mut self) -> Box<dyn Iterator<Item = &mut i32> + '_> {
        Box::new(self.nums.iter_mut().rev())
    }
}

struct Cursor<'a> {
    it: std::slice::IterMut<'a, i32>,
}

impl Cursor<'_> {
    fn bump(&mut self) {
        if let Some(x) = self.it.next() {
            *x += 100;
        }
    }
}

fn first_to_zero<'a>(mut it: impl Iterator<Item = &'a mut i32>) {
    if let Some(x) = it.next() {
        *x = 0;
    }
    it.for_each(|x| *x += 1);
}

fn bump_all(v: &mut [i32]) -> Result<(), String> {
    v.iter_mut().try_for_each(|item| {
        if *item > 100 {
            return Err(format!("too big: {item}"));
        }
        *item += 1;
        Ok(())
    })
}

fn copy(src: &[u32], dst_words: &mut Vec<u32>) {
    let (src, dst) = (src, &mut *dst_words);
    for (from, to) in src.iter().zip(dst.iter_mut()) {
        *to = *from;
    }
}

fn vals(m: &mut BTreeMap<u8, i64>) -> impl Iterator<Item = &mut i64> {
    m.values_mut()
}

fn structs() {
    let mut groups = vec![Group { id: 1, args: vec![] }, Group { id: 2, args: vec![] }];
    for g in [2u32, 3] {
        if let Some(ag) = groups.iter_mut().find(|grp| grp.id == g) {
            ag.args.push(g * 10);
        } else {
            groups.push(Group { id: g, args: vec![g] });
        }
    }
    let mut mapping: Vec<(String, Vec<u32>)> = vec![("a".into(), vec![]), ("b".into(), vec![])];
    if let Some((_, known)) = mapping.iter_mut().rev().find(|(id, _)| id == "b") {
        known.push(7);
    }
    println!("{groups:?} {mapping:?}");
    let mut hs = vec![H::default(), H::default(), H::default()];
    let mut iter = hs.iter_mut();
    let mut n = 1;
    while let Some(h) = iter.next() {
        h.name = n;
        n += 1;
        if n > 2 {
            break;
        }
    }
    let mut nums = vec![1, 2, 3];
    let mut it = nums.iter_mut();
    if let Some(x) = it.next() {
        *x = 10;
    }
    for x in it {
        *x += 100;
    }
    println!("{hs:?} {nums:?}");
}

fn closures() {
    let mut v = vec![1, 2, 3];
    println!("{:?} {v:?}", bump_all(&mut v));
    v.push(500);
    println!("{:?} {v:?}", bump_all(&mut v));
    let mut w = vec![1, 2, 3, 4];
    let r: Result<(), i32> = w.iter_mut().try_for_each(|x| {
        if *x == 3 {
            return Err(*x);
        }
        *x *= 10;
        Ok(())
    });
    let total = w.iter_mut().try_fold(0, |acc, x| {
        *x += 1;
        if acc > 100 { None } else { Some(acc + *x) }
    });
    let found = w.iter_mut().find_map(|x| {
        if *x > 20 {
            *x = -1;
            Some(*x)
        } else {
            None
        }
    });
    println!("{r:?} {total:?} {found:?} {w:?}");
    let mut s = vec![String::from("a"), String::from("b")];
    let _ = s.iter_mut().rev().try_for_each(|x| -> Result<(), ()> {
        x.push('!');
        Ok(())
    });
    let mut slices = [0usize; 5];
    for (i, s) in slices.iter_mut().enumerate().take(3) {
        *s = i * 10 + 1;
    }
    for (i, s) in slices.iter_mut().enumerate().skip(3).rev() {
        *s += i;
    }
    println!("{s:?} {slices:?}");
}

fn returned() {
    let mut b = Bag {
        nums: vec![1, 2, 3],
        names: vec!["a".into(), String::new(), "c".into()],
        groups: vec![Group { id: 1, args: vec![] }, Group { id: 2, args: vec![] }],
    };
    let mut it = b.nums_mut();
    if let Some(x) = it.next() {
        *x = 100;
    }
    it.for_each(|x| *x += 1);
    for s in b.names_mut() {
        s.push('?');
    }
    let n = b.names_mut().count();
    for g in b.groups_mut() {
        g.id *= 10;
    }
    if let Some(g) = b.groups_mut().find(|g| g.id == 20) {
        g.args.push(42);
    }
    for x in b.boxed().take(2) {
        *x *= 2;
    }
    println!("{:?} {:?} {:?} {n}", b.nums, b.names, b.groups);
    let mut v = vec![5, 6, 7];
    first_to_zero(v.iter_mut());
    first_to_zero(v.iter_mut().rev());
    {
        let mut c = Cursor { it: v.iter_mut() };
        c.bump();
        c.bump();
    }
    let mut d = vec![0u32; 3];
    copy(&[4, 5, 6], &mut d);
    println!("{v:?} {d:?}");
}

fn maps() {
    let mut m: BTreeMap<String, String> = BTreeMap::new();
    m.insert("x".into(), "1".into());
    m.insert("y".into(), "2".into());
    for (k, v) in m.iter_mut().filter(|(k, _)| k.as_str() != "x") {
        v.push_str(k);
    }
    if let Some((_, v)) = m.iter_mut().find(|(_, v)| v.as_str() == "1") {
        *v = String::from("one");
    }
    let mut it = m.values_mut();
    if let Some(v) = it.next() {
        v.push('!');
    }
    m.iter_mut().for_each(|(_, v)| v.push('.'));
    println!("{m:?}");
    let mut n: BTreeMap<u8, i64> = BTreeMap::new();
    n.insert(3, 30);
    n.insert(1, 10);
    n.insert(2, 50);
    if let Some(top) = n.values_mut().max() {
        *top += 1;
    }
    for v in vals(&mut n).skip(1) {
        *v *= 2;
    }
    n.values_mut().for_each(|v| *v += 1000);
    let s: i64 = n.values_mut().map(|v| { *v -= 1; *v }).sum();
    let ks: Vec<u8> = n.iter_mut().map(|(k, _)| *k).collect();
    println!("{n:?} {s} {ks:?}");
    let mut objs: HashMap<&str, Group> = HashMap::new();
    objs.insert("a", Group { id: 1, args: vec![] });
    if let Some((_, g)) = objs.iter_mut().find(|(k, _)| **k == "a") {
        g.id = 7;
    }
    objs.iter_mut().for_each(|(_, g)| g.id *= 2);
    objs.values_mut().for_each(|g| g.args.push(1));
    if let Some(g) = objs.values_mut().find(|g| g.id > 5) {
        g.args.push(2);
    }
    println!("{:?}", objs["a"]);
}

fn deques() {
    let mut d: VecDeque<i32> = VecDeque::from(vec![1, 2, 3]);
    d.push_front(0);
    d.iter_mut().for_each(|x| *x *= 2);
    d.iter_mut().enumerate().for_each(|(i, x)| *x += i as i32);
    if let Some(x) = d.iter_mut().find(|x| **x == 5) {
        *x = 50;
    }
    let mut it = d.iter_mut();
    if let Some(x) = it.next() {
        *x = -1;
    }
    println!("{d:?}");
}

fn main() {
    structs();
    closures();
    returned();
    maps();
    deques();
}
