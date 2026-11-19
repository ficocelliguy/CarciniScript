// Loaded by `#[path]` from three files. Its `mod helpers;` is `lib/helpers.rs` (a `#[path]`
// file's children are its siblings, as in rustc) and its `include_str!` reads `lib/words.txt`.
pub mod helpers;

pub const WORDS: &str = include_str!("words.txt");

pub fn greet(who: &str) -> String {
    format!("hi {who}")
}

pub fn word_count() -> usize {
    WORDS.split_whitespace().count()
}
