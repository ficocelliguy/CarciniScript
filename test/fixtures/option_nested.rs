// `Option<Option<T>>` (and deeper) kept, passed, stored and read: `Some(None)` is its own value,
// from `Some(..)`, collections, iterators, maps and `map`, through patterns, `?`, `unwrap*`,
// `flatten`, `==`, ordering, `{:?}` and JSON.
use std::collections::{HashMap, VecDeque};

fn describe(o: Option<Option<i32>>) -> String {
    match o {
        None => "none".to_string(),
        Some(None) => "some(none)".to_string(),
        Some(Some(n)) => format!("some(some({}))", n),
    }
}

fn first_inner(v: &[Option<i32>]) -> Option<i32> {
    let x = v.first()?;
    x.map(|n| n * 10)
}

fn deep(n: u32) -> Option<Option<Option<u32>>> {
    match n {
        0 => None,
        1 => Some(None),
        2 => Some(Some(None)),
        _ => Some(Some(Some(n))),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Cell {
    seen: Option<Option<String>>,
}

fn main() {
    let vals: Vec<Option<i32>> = vec![None, Some(2), None];
    let a = vals.first().copied();
    let b = vals.get(1).copied();
    let c = vals.get(9).copied();
    println!("{:?} {:?} {:?}", a, b, c);
    println!("{} {} {}", describe(a), describe(b), describe(c));
    println!("{} {}", a.is_some(), c.is_none());
    println!("{:?} {:?}", a.flatten(), b.flatten());
    println!("{:?} {:?}", a.unwrap(), b.unwrap_or(Some(7)));
    println!("{:?}", c.unwrap_or(Some(7)));
    println!("{:?} {:?}", first_inner(&vals), first_inner(&[Some(4)]));
    println!("{}", a == Some(None));
    println!("{} {}", a < b, None < a);
    let mut sorted = vec![b, c, a];
    sorted.sort();
    println!("{:?}", sorted);

    let mut it = vals.iter().copied();
    while let Some(x) = it.next() {
        print!("[{:?}]", x);
    }
    println!();
    println!("{:?} {:?}", vals.iter().copied().last(), vals.iter().copied().max());
    println!("{:?}", vals.iter().copied().find(|x| x.is_none()));

    let mut m: HashMap<&str, Option<u8>> = HashMap::new();
    println!("{:?}", m.insert("a", None));
    println!("{:?}", m.insert("a", Some(1)));
    println!("{:?} {:?}", m.get("a"), m.get("z"));
    m.insert("b", None);
    println!("{:?} {:?}", m.remove("b"), m.remove("b"));

    let mut q: VecDeque<Option<char>> = VecDeque::new();
    q.push_back(None);
    q.push_back(Some('x'));
    println!("{:?} {:?}", q.front(), q.back());
    println!("{:?} {:?} {:?}", q.pop_front(), q.pop_front(), q.pop_front());

    let mapped: Option<Option<i32>> = Some(3).map(|n| if n > 5 { Some(n) } else { None });
    println!("{:?}", mapped);
    let lifted: Option<Option<i32>> = Some(5).map(Some);
    println!("{:?}", lifted);
    let mut slot: Option<Option<i32>> = None;
    println!("{:?} {:?}", slot.replace(None), slot);

    for n in 0..4 {
        print!("{:?} ", deep(n));
    }
    println!();
    println!("{:?} {:?}", deep(2).flatten(), deep(2).flatten().flatten());

    let cells = vec![
        Cell { seen: None },
        Cell { seen: Some(None) },
        Cell { seen: Some(Some("x".into())) },
    ];
    println!("{:?}", cells);
    println!("{}", cells[1] == Cell { seen: Some(None) });
    println!("{}", cells.iter().filter(|c| c.seen.is_some()).count());
    if let Some(inner) = &cells[1].seen {
        println!("inner is_none: {}", inner.is_none());
    }
}
