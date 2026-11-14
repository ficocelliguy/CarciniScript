// `char::to_uppercase` / `to_lowercase` as iterators (`next`, `collect`, `flat_map`, `extend`,
// `count`, `to_string`, `{}`; `'ß'` uppercases to "SS"), and `Chars::as_str` for the rest of a string.
fn capitalize(s: &str) -> String {
    let mut cs = s.chars();
    match cs.next() {
        Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

fn main() {
    let c = 'q';
    let up = c.to_uppercase().next().unwrap();
    let s: String = "Straße".chars().flat_map(|c| c.to_uppercase()).collect();
    let mut out = String::new();
    for ch in "AbC".chars() {
        out.extend(ch.to_lowercase());
    }
    println!("{} {} {} {}", up, s, out, capitalize("hello"));
    println!("{} {}", 'ß'.to_uppercase().count(), 'x'.to_uppercase().to_string());
    println!("{}", 'é'.to_uppercase());
}
