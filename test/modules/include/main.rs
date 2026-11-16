// `include_str!` / `include_bytes!`: files next to the script, read through the resolver that
// loads `mod` files (also from a module).
mod data;

const WORDS: &str = include_str!("words.txt");

fn main() {
    let words: Vec<&str> = WORDS.lines().collect();
    println!("{:?}", words);
    println!("{}", include_bytes!("words.txt").len());
    println!("{}", data::count());
}
