// Hand-written `Hash`, `Copy`, `TryFrom`, `FromIterator` and `Extend` impls, integer
// `try_from` / `try_into`, and `IntoIterator` parameters.
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::hash::{Hash, Hasher};

// Equal by `id` only: `Hash` must agree.
#[derive(Debug, Clone)]
struct User {
    id: u32,
    name: String,
}

impl PartialEq for User {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for User {}

impl Hash for User {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct Tag(String);

#[derive(Debug, PartialEq, Eq)]
struct Key {
    tag: Tag,
    n: i64,
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tag.hash(state);
        self.n.hash(state);
    }
}

#[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

impl Clone for Point {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Point {}

#[derive(Debug)]
struct Even(u32);

impl TryFrom<i64> for Even {
    type Error = String;

    fn try_from(v: i64) -> Result<Self, Self::Error> {
        if v >= 0 && v % 2 == 0 {
            Ok(Even(v as u32))
        } else {
            Err(format!("{} is not a non-negative even number", v))
        }
    }
}

#[derive(Debug, Default)]
struct Bag {
    items: Vec<i32>,
    total: i32,
}

impl FromIterator<i32> for Bag {
    fn from_iter<I: IntoIterator<Item = i32>>(iter: I) -> Self {
        let mut b = Bag::default();
        for x in iter {
            b.items.push(x);
            b.total += x;
        }
        b
    }
}

impl Extend<i32> for Bag {
    fn extend<T: IntoIterator<Item = i32>>(&mut self, iter: T) {
        for x in iter.into_iter() {
            self.items.push(x);
            self.total += x;
        }
    }
}

fn total(xs: impl IntoIterator<Item = u32>) -> u32 {
    let mut t = 0;
    for x in xs {
        t += x;
    }
    t
}

fn count_keys<I: IntoIterator<Item = (String, i32)>>(pairs: I) -> usize {
    pairs.into_iter().filter(|(_, v)| *v > 1).count()
}

fn main() {
    // Hash + Eq by id: the second "Ann" replaces nothing, it's the same key.
    let mut seen = HashSet::new();
    seen.insert(User {
        id: 1,
        name: "Ann".into(),
    });
    seen.insert(User {
        id: 2,
        name: "Bob".into(),
    });
    let fresh = seen.insert(User {
        id: 1,
        name: "Ann (again)".into(),
    });
    println!("{} {}", seen.len(), fresh);
    let mut by_user: HashMap<User, u32> = HashMap::new();
    *by_user
        .entry(User {
            id: 7,
            name: "x".into(),
        })
        .or_insert(0) += 1;
    *by_user
        .entry(User {
            id: 7,
            name: "y".into(),
        })
        .or_insert(0) += 1;
    println!("{:?}", by_user.values().collect::<Vec<_>>());
    let mut keys = HashSet::new();
    keys.insert(Key {
        tag: Tag("a".into()),
        n: 1,
    });
    keys.insert(Key {
        tag: Tag("a".into()),
        n: 1,
    });
    keys.insert(Key {
        tag: Tag("b".into()),
        n: 1,
    });
    println!("{}", keys.len());
    let u = seen.iter().find(|u| u.id == 2).unwrap();
    println!("{}", u.name);

    // Copy
    let p = Point { x: 1, y: 2 };
    let mut q = p;
    q.x = 10;
    println!("{:?} {:?}", p, q);

    // TryFrom / try_into
    println!("{:?} {:?}", Even::try_from(8), Even::try_from(7));
    let e: Result<Even, String> = 12i64.try_into();
    println!("{:?}", e);
    println!("{:?} {:?}", u8::try_from(255), u8::try_from(256));
    println!("{:?} {:?}", i8::try_from(-129i32), u32::try_from(-1i64));
    let small: u16 = 300u64.try_into().unwrap();
    let big: i64 = 5usize.try_into().unwrap();
    println!("{} {}", small, big);
    match u8::try_from(1000) {
        Ok(v) => println!("ok {}", v),
        Err(e) => println!("error: {}", e),
    }
    let r: Result<i8, _> = 200i32.try_into();
    println!("{:?}", r);
    let c: Result<char, _> = 0x1F600u32.try_into();
    println!("{:?} {:?} {:?}", char::try_from(65u32), c, char::try_from(0xD800u32));
    if let Err(e) = char::try_from(0x110000u32) {
        println!("{}", e);
    }

    // FromIterator / Extend / IntoIterator parameters
    let bag: Bag = (1..=4).collect();
    println!("{:?}", bag);
    let mut b2 = vec![5, 6].into_iter().map(|x| x * 10).collect::<Bag>();
    b2.extend(vec![1, 2]);
    b2.extend((0..3).map(|x| x + 100));
    println!("{:?} {}", b2.items, b2.total);
    println!(
        "{} {}",
        total(vec![1, 2, 3]),
        total((1..=10).filter(|x| x % 2 == 0))
    );
    let mut set = HashSet::new();
    set.insert(4u32);
    println!("{}", total(set));
    let mut m = HashMap::new();
    m.insert("a".to_string(), 1);
    m.insert("b".to_string(), 2);
    m.insert("c".to_string(), 3);
    println!("{}", count_keys(m));
}
