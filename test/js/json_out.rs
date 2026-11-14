// `JSON::stringify` / `stringify_pretty`: serde_json's output for numbers, strings, options,
// collections, tuples, structs (named, tuple, newtype, unit), enums of every shape, Result and
// nested generic types.
use js::JSON;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Serialize)]
struct Point {
    x: f64,
    y: i32,
}

#[derive(Serialize)]
struct Pair(u8, String);

#[derive(Serialize)]
struct Meters(f64);

#[derive(Serialize)]
struct Marker;

#[derive(Serialize)]
enum Shape {
    Dot,
    Circle(f64),
    Pair(i32, String),
    Rect { w: u8, h: u8 },
}

#[derive(Serialize)]
struct Labeled<T> {
    label: String,
    value: T,
    tags: Option<Vec<T>>,
}

#[derive(Serialize)]
struct Tree {
    name: String,
    kids: Vec<Tree>,
}

fn main() {
    // Numbers as serde_json writes them.
    let floats = vec![0.0, -0.0, 1.0, 1.5, -2.25, 0.1 + 0.2, 1e15, 1e16, 1e17, 1.5e300, 1e-5, 1e-6, 1.25e-7, 123456.789];
    println!("{}", JSON::stringify(&floats));
    println!("{}", JSON::stringify(&vec![0.1f32, 1.0f32, 16777216.0f32, 3.4e38f32, 1e-7f32]));
    println!("{}", JSON::stringify(&(i32::MIN, u8::MAX, -7i64, 123456789u32)));
    println!("{}", JSON::stringify(&f64::NAN));
    // Strings, chars, bools, unit.
    println!("{}", JSON::stringify(&"quote \" backslash \\ newline \n tab \t bell \u{7} é ✓"));
    println!("{} {} {}", JSON::stringify(&'x'), JSON::stringify(&true), JSON::stringify(&()));
    // Options and collections.
    let some: Option<i32> = Some(3);
    let none: Option<i32> = None;
    println!("{} {}", JSON::stringify(&some), JSON::stringify(&none));
    let mut m = BTreeMap::new();
    m.insert(2, "two".to_string());
    m.insert(1, "one".to_string());
    println!("{}", JSON::stringify(&m));
    let set: BTreeSet<char> = ['c', 'a', 'b'].into_iter().collect();
    let dq: VecDeque<u8> = VecDeque::from(vec![3, 4]);
    println!("{} {}", JSON::stringify(&set), JSON::stringify(&dq));
    let empty: Vec<i32> = Vec::new();
    println!("{} {}", JSON::stringify(&empty), JSON::stringify(&BTreeMap::<String, i32>::new()));
    // Structs and enums.
    println!("{}", JSON::stringify(&Point { x: 1.0, y: -2 }));
    println!("{} {} {}", JSON::stringify(&Pair(1, "a".into())), JSON::stringify(&Meters(2.5)), JSON::stringify(&Marker));
    let shapes = vec![Shape::Dot, Shape::Circle(2.0), Shape::Pair(1, "b".into()), Shape::Rect { w: 3, h: 4 }];
    println!("{}", JSON::stringify(&shapes));
    let results: Vec<Result<i32, String>> = vec![Ok(1), Err("bad".into())];
    println!("{}", JSON::stringify(&results));
    let l = Labeled { label: "pts".into(), value: Point { x: 0.5, y: 1 }, tags: Some(vec![Point { x: 1.0, y: 2 }]) };
    println!("{}", JSON::stringify(&l));
    let t = Tree { name: "root".into(), kids: vec![Tree { name: "leaf".into(), kids: vec![] }] };
    println!("{}", JSON::stringify(&t));
    // Pretty.
    println!("{}", JSON::stringify_pretty(&l));
    println!("{}", JSON::stringify_pretty(&shapes));
    println!("{}", JSON::stringify_pretty(&t));
    println!("{}", JSON::stringify_pretty(&(empty, BTreeMap::<String, i32>::new(), none)));
}
