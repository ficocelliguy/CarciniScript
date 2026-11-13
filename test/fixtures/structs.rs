// Structs: named, tuple and unit structs, struct literals (shorthand, `..base`), field access and
// assignment, `impl` blocks (associated functions, `&self`, `&mut self`, `self`), `Self`,
// derives (Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default), Copy semantics,
// struct patterns, and structs in collections.
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn origin() -> Point {
        Point::new(0.0, 0.0)
    }

    fn dist(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }

    fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }

    fn swapped(self) -> Point {
        Point { x: self.y, y: self.x }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Version(u32, u32, u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Marker;

#[derive(Debug, Default)]
struct Inventory {
    owner: String,
    items: Vec<String>,
    gold: u32,
    origin: Point,
}

impl Inventory {
    fn add(&mut self, item: &str) -> usize {
        self.items.push(item.to_string());
        self.items.len()
    }

    fn total_len(&self) -> usize {
        self.items.iter().map(|s| s.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Server {
    name: String,
    ram: u32,
}

fn describe(p: Point) -> String {
    match p {
        Point { x, y: 0.0 } => format!("on the x axis at {}", x),
        Point { x: 0.0, .. } => "on the y axis".to_string(),
        Point { x, y } if x == y => "on the diagonal".to_string(),
        _ => "somewhere".to_string(),
    }
}

fn main() {
    let a = Point::new(3.0, 4.0);
    let o = Point::origin();
    println!("{:?} {:?} {}", a, o, a.dist(&o));
    let mut b = a;
    b.translate(1.0, -1.0);
    b.x *= 2.0;
    println!("{:?} {:?}", a, b);
    let c = Point { y: 9.5, ..a };
    println!("{:?} {:?} {}", c, c.swapped(), a == Point { x: 3.0, y: 4.0 });
    println!("{:?}", Point::default());

    let v1 = Version(1, 2, 3);
    let v2 = Version(1, 10, 0);
    println!("{:?} {} {} {:?}", v1, v1 < v2, v1.0 + v2.1, v1.cmp(&v2));
    let mut versions = vec![Version(2, 0, 0), v2.clone(), v1.clone(), Version(1, 2, 0)];
    versions.sort();
    println!("{:?}", versions);
    println!("{:?}", versions.iter().max());
    let Version(major, _, patch) = v2;
    println!("{} {}", major, patch);

    let m = Marker;
    println!("{:?} {}", m, m == Marker);

    let mut inv = Inventory { owner: String::from("ada"), ..Default::default() };
    inv.add("sword");
    let n = inv.add("shield");
    inv.gold += 25;
    println!("{} {} {} {:?}", n, inv.total_len(), inv.gold, inv.origin);
    println!("{:?}", inv);

    let servers = vec![
        Server { name: "n00dles".to_string(), ram: 4 },
        Server { name: "foodnstuff".to_string(), ram: 16 },
        Server { name: "sigma".to_string(), ram: 16 },
    ];
    let big: Vec<&Server> = servers.iter().filter(|s| s.ram >= 16).collect();
    println!("{:?}", big.iter().map(|s| &s.name).collect::<Vec<_>>());
    let mut set = HashSet::new();
    for s in &servers {
        set.insert(s.clone());
    }
    set.insert(servers[0].clone());
    println!("{}", set.len());
    let mut by_ram: HashMap<u32, Vec<String>> = HashMap::new();
    for s in &servers {
        by_ram.entry(s.ram).or_default().push(s.name.clone());
    }
    let mut keys: Vec<_> = by_ram.keys().copied().collect();
    keys.sort();
    println!("{:?} {:?}", keys, by_ram[&16]);
    let best = servers.iter().max_by_key(|s| (s.ram, s.name.len())).unwrap();
    println!("{}", best.name);

    for p in [Point::new(2.0, 0.0), Point::new(0.0, 5.0), Point::new(1.5, 1.5), Point::new(1.0, 2.0)] {
        println!("{}", describe(p));
    }
    let mut pts = vec![Point::new(1.0, 1.0), Point::new(2.0, 2.0)];
    for p in pts.iter_mut() {
        p.translate(10.0, 0.0);
    }
    pts[0].y = -1.0;
    println!("{:?}", pts);
}
