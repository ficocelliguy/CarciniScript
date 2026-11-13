// HashMap: insert/get/remove with Option results, the entry API (counting and grouping),
// iteration (read and write-through), indexing, tuple keys, cloning and equality.
// Iteration order is unspecified in Rust, so printed collections are sorted first.
use std::collections::HashMap;

fn word_counts(text: &str) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    let mut word = String::new();
    for c in ['t', 'o', ' ', 'b', 'e', ' ', 'o', 'r', ' ', 'n', 'o', 't', ' ', 't', 'o', ' ', 'b', 'e', ' '] {
        if c == ' ' {
            if !word.is_empty() {
                *counts.entry(word.clone()).or_insert(0) += 1;
                word = String::new();
            }
        } else {
            word.push(c);
        }
    }
    let _ = text;
    counts
}

fn main() {
    let mut ages: HashMap<String, u32> = HashMap::new();
    println!("{:?}", ages.insert(String::from("ada"), 36));
    println!("{:?}", ages.insert(String::from("ada"), 37));
    ages.insert(String::from("bob"), 25);
    println!("{:?} {:?} {}", ages.get("ada"), ages.get("eve"), ages.contains_key("bob"));
    println!("{} {}", ages["bob"], ages.len());
    println!("{:?} {:?} {}", ages.remove("bob"), ages.remove("bob"), ages.len());
    println!("{:?}", ages);

    let counts = word_counts("to be or not to be");
    let mut words = Vec::new();
    for (w, n) in &counts {
        words.push((w.clone(), *n));
    }
    words.sort();
    println!("{:?}", words);
    println!("to: {}, missing: {}", counts["to"], counts.get("xyz").copied().unwrap_or(0));

    // Grouping: values are Vecs, mutated in place through the entry.
    let mut by_len: HashMap<usize, Vec<String>> = HashMap::new();
    for w in ["fig", "kiwi", "pear", "plum", "date", "lime", "apple"] {
        by_len.entry(w.len()).or_default().push(w.to_string());
    }
    by_len.entry(10).or_insert_with(Vec::new);
    let mut lens = Vec::new();
    for k in by_len.keys() {
        lens.push(*k);
    }
    lens.sort();
    for k in &lens {
        print!("{}={:?} ", k, by_len[k]);
    }
    println!();
    if let Some(fours) = by_len.get_mut(&4) {
        fours.sort();
        fours.push(String::from("zzz"));
    }
    println!("{:?}", by_len[&4]);

    // Write-through iteration on number values.
    let mut stock: HashMap<(i32, i32), i64> = HashMap::from([((0, 0), 5), ((1, 2), -3), ((2, 2), 10)]);
    for (_, v) in stock.iter_mut() {
        *v *= 2;
    }
    for v in stock.values_mut() {
        *v += 1;
    }
    let mut total = 0;
    for v in stock.values() {
        total += *v;
    }
    let mut cells = Vec::new();
    for ((r, c), v) in &stock {
        cells.push((*r, *c, *v));
    }
    cells.sort();
    println!("total {total} {:?}", cells);
    *stock.entry((9, 9)).or_insert(100) -= 1;
    *stock.entry((0, 0)).or_default() += 1000;
    println!("{} {} {}", stock[&(9, 9)], stock[&(0, 0)], stock.len());

    let copy = stock.clone();
    stock.insert((5, 5), 0);
    println!("{} {} {}", copy.len(), stock.len(), copy == stock);
    stock.remove(&(5, 5));
    println!("{} {}", copy == stock, stock.is_empty());
    stock.clear();
    println!("{} {:?}", stock.is_empty(), HashMap::from([(1, 'x')]));

    let missing = ages["nobody"];
    println!("unreachable {missing}");
}
