// Inference from a later arm: `'l: loop { .. break 'l Default::default(); .. break 'l x; }` (also
// `parse()`, `collect()`, `into()`, `None` in an earlier `break`), and `Default::default()` in an
// `if` / `match` arm typed by another arm. Only the later arm fixes the type, as in rustc.
use std::collections::HashMap;

fn parse_first(xs: &[&str]) -> u64 {
    let r = 'l: loop {
        for x in xs {
            if x.len() == 2 {
                break 'l x.parse().unwrap();
            }
        }
        break 'l 0u64;
    };
    r * 2
}

fn collect_it(n: u32) -> usize {
    let r = 'l: {
        if n > 3 {
            break 'l (0..n).collect();
        }
        let v: Vec<u32> = vec![1];
        v
    };
    r.len()
}

fn into_it(n: u8) -> u64 {
    let r = 'l: loop {
        if n > 3 {
            break 'l n.into();
        }
        break 'l 9u64;
    };
    r
}

fn default_it(n: u8) -> String {
    let r = 'l: loop {
        if n > 3 {
            break 'l Default::default();
        }
        break 'l String::from("x");
    };
    r
}

fn first_vec(n: i32) -> usize {
    let r = 'outer: loop {
        for i in 0..n {
            if i == 100 {
                break 'outer Vec::new();
            }
            if i == 3 {
                let mut v = Vec::new();
                v.push(i as u64);
                break 'outer v;
            }
        }
        break 'outer Vec::new();
    };
    r.len()
}

fn first_map() -> String {
    let m = 'x: {
        if false {
            break 'x HashMap::new();
        }
        let mut h = HashMap::new();
        h.insert("k".to_string(), 2.5f64);
        h
    };
    format!("{:?}", m.get("k"))
}

fn first_big(xs: &[f64]) -> Option<f64> {
    let mut i = 0;
    let r = 'outer: loop {
        loop {
            if i >= xs.len() {
                break 'outer None;
            }
            if xs[i] > 1.0 {
                break 'outer Some(xs[i]);
            }
            i += 1;
        }
    };
    let total = r.map(|x| x * 2.0);
    total
}

fn first_int() -> i64 {
    let mut k = 0;
    let r = 'l: loop {
        k += 1;
        if k > 5 {
            break 'l Default::default();
        }
        if k == 10 {
            break 'l 7i64;
        }
    };
    r + 1
}

fn arms(c: bool) -> String {
    let a = if c { Default::default() } else { String::from("s") };
    let b = match c {
        true => Default::default(),
        false => 3u8,
    };
    let d = if c { None } else { Some(Default::default()) };
    let e: Option<u8> = d;
    format!("{a:?} {b} {e:?}")
}

fn main() {
    println!("{}", parse_first(&["5", "42"]));
    println!("{}", collect_it(5));
    println!("{}", into_it(5));
    println!("{:?} {:?}", default_it(5), default_it(1));
    println!("{}", first_vec(5));
    println!("{}", first_map());
    println!("{:?} {:?}", first_big(&[0.5, 3.0]), first_big(&[]));
    println!("{}", first_int());
    println!("{}", arms(true));
    println!("{}", arms(false));
}
