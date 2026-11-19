#[path = "../lib/util.rs"]
mod util;

pub fn run() -> String {
    let first = util::WORDS.lines().next().unwrap_or("");
    format!("corp: {} first line {first:?}", util::greet("corp"))
}
