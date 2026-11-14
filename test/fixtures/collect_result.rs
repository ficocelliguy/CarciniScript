// Collecting into `Result` and `Option`: stops at the first `Err` / `None` (later items are never
// pulled), else wraps the collected `Vec`, `HashSet`, `HashMap` or `String`. Also via `?`.
use std::collections::{HashMap, HashSet};

fn parse_all(items: &[&str]) -> Result<Vec<i32>, String> {
    items
        .iter()
        .map(|s| s.parse::<i32>().map_err(|_| format!("bad number: {s}")))
        .collect()
}

fn total(items: &[&str]) -> Result<i32, String> {
    let nums = parse_all(items)?;
    Ok(nums.iter().sum())
}

fn halves(v: &[i32]) -> Option<Vec<i32>> {
    v.iter().map(|x| if x % 2 == 0 { Some(x / 2) } else { None }).collect()
}

fn main() {
    println!("{:?}", parse_all(&["1", "2", "3"]));
    println!("{:?}", parse_all(&["1", "x", "y"]));
    println!("{:?} {:?}", total(&["4", "5"]), total(&["4", "five"]));
    println!("{:?} {:?}", halves(&[2, 4, 6]), halves(&[2, 3, 4]));

    // Short-circuit: the closure stops running at the first error.
    let mut seen = 0;
    let r: Result<Vec<i32>, String> = (1..10)
        .map(|i| {
            seen += 1;
            if i == 3 { Err(format!("stop at {i}")) } else { Ok(i) }
        })
        .collect();
    println!("{:?} after {seen}", r);

    let set: Option<HashSet<char>> = "abca".chars().map(Some).collect();
    println!("{}", set.unwrap().len());
    let map = vec![("a", 1), ("b", 2)]
        .into_iter()
        .map(|(k, v)| if v > 0 { Ok((k, v)) } else { Err(k) })
        .collect::<Result<HashMap<&str, i32>, &str>>();
    println!("{}", map.unwrap()["b"]);
    let word: Option<String> = vec!['h', 'i'].into_iter().map(Some).collect();
    println!("{:?}", word);
    let empty: Result<Vec<i32>, String> = Vec::<i32>::new().into_iter().map(Ok).collect();
    println!("{:?}", empty);
}
