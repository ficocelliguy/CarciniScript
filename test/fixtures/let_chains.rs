// Let-chains (Rust 2024): `if let .. && ..` with several lets and plain conditions, later parts
// using earlier bindings, chains as values, in `else if`, and in `while`.
fn classify(a: Option<i32>, b: Result<i32, String>) -> &'static str {
    if let Some(x) = a && x > 0 && let Ok(y) = b && y > x {
        "both, increasing"
    } else if let Some(x) = a && x < 0 {
        "negative"
    } else {
        "other"
    }
}

fn main() {
    println!("{}", classify(Some(1), Ok(5)));
    println!("{}", classify(Some(5), Ok(1)));
    println!("{}", classify(Some(-3), Ok(1)));
    println!("{}", classify(None, Err("no".to_string())));

    let words = vec!["apple", "kiwi", "banana"];
    let found = if let Some(w) = words.iter().find(|w| w.len() > 4) && w.starts_with('b') { *w } else { "none" };
    println!("{}", found);

    let mut stack = vec![1, 2, 30, 4];
    while let Some(top) = stack.pop() && top < 10 {
        println!("popped {}", top);
    }
    println!("{:?}", stack);

    let pairs = vec![(1, Some('a')), (2, None), (3, Some('c'))];
    for (n, c) in &pairs {
        if *n != 2 && let Some(ch) = c {
            println!("{} {}", n, ch);
        }
    }
}
