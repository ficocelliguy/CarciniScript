// Read a config the quick way: `unwrap` and `expect` everywhere.
use std::collections::HashMap;

fn parse_config(text: &str) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').expect("line without '='");
        out.insert(key.trim().to_string(), value.trim().parse().unwrap());
    }
    out
}

fn main() {
    let good = parse_config("threads = 8\n# comment\nreserve=32\n");
    let mut keys: Vec<_> = good.keys().collect();
    keys.sort();
    println!("{:?} threads={}", keys, good["threads"]);

    let missing = good.get("cycles").copied();
    println!("cycles: {:?}", missing);

    let bad = parse_config("threads = 8\nreserve = 3.5\n");
    println!("unreachable: {}", bad.len());
}
