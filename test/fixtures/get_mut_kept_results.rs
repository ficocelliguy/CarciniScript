// `get_mut` / `peek_mut` of a number, string or bool unwrapped with `unwrap_or_else(|| &mut d)`
// (a slot over the default local), and `ok_or(e)` / `ok_or_else(..)` kept in a local as a
// `Result` that is then matched, `?`-ed or unwrapped: writes reach the collection.
use std::collections::{BTreeMap, HashMap};

fn bump(m: &mut HashMap<&str, i32>, k: &str) -> Result<i32, String> {
    let r = m.get_mut(k).ok_or(format!("no {k}"));
    *r? += 1;
    let r2 = m.get_mut(k).ok_or_else(|| "gone".to_string());
    let v = r2?;
    *v *= 10;
    Ok(*v)
}

fn main() {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("a", 1);
    let mut spare = 7;
    *m.get_mut("a").unwrap_or_else(|| &mut spare) += 100;
    *m.get_mut("b").unwrap_or_else(|| &mut spare) += 1;
    let x = *m.get_mut("a").unwrap_or_else(|| &mut spare);
    println!("{:?} {} {}", m, spare, x);
    let mut d = 0;
    let r = m.get_mut("a").unwrap_or_else(|| &mut d);
    *r -= 1;
    let r = m.get_mut("zz").unwrap_or_else(|| {
        println!("default");
        &mut d
    });
    *r += 5;
    println!("{:?} {}", m, d);

    let kept = m.get_mut("a").ok_or("missing");
    match kept {
        Ok(v) => *v += 1000,
        Err(e) => println!("{}", e),
    }
    let kept = m.get_mut("q").ok_or("missing");
    match kept {
        Ok(v) => *v += 1000,
        Err(e) => println!("{}", e),
    }
    let r = m.get_mut("a").ok_or(0);
    if let Ok(v) = r {
        *v *= 2;
    }
    let r = m.get_mut("a").ok_or(0);
    *r.unwrap() += 1;
    println!("{:?}", m);
    println!("{:?}", bump(&mut m, "a"));
    println!("{:?}", bump(&mut m, "nope"));
    println!("{:?}", m);

    let mut names: BTreeMap<u8, String> = BTreeMap::new();
    names.insert(1, "n00dles".into());
    let mut fallback = String::new();
    names.get_mut(&1).unwrap_or_else(|| &mut fallback).push('!');
    names.get_mut(&2).unwrap_or_else(|| &mut fallback).push('?');
    let r = names.get_mut(&1).ok_or(());
    r.expect("there").push_str("-home");
    println!("{:?} {:?}", names, fallback);

    let mut v = vec![1.5, 2.5];
    let mut z = 0.0;
    *v.get_mut(1).unwrap_or_else(|| &mut z) *= 2.0;
    *v.get_mut(9).unwrap_or_else(|| &mut z) += 1.0;
    let r = v.get_mut(0).ok_or("none");
    if let Ok(x) = r {
        *x = -1.0;
    }
    println!("{:?} {}", v, z);
    let mut it = vec![1, 2].into_iter().peekable();
    let mut p0 = 0;
    *it.peek_mut().unwrap_or_else(|| &mut p0) += 10;
    println!("{:?} {}", it.collect::<Vec<_>>(), p0);
}
