// Cross-folder modules: `#[path = ".."]` loads a module file from another directory, relative
// to this file (`..` included). A file loaded that way reads its own `mod`s and `include_str!`s
// next to itself, and one `lib/` file serves scripts in several folders.
#[path = "lib/util.rs"]
mod util;
mod hack;
mod corp;

fn main() {
    println!("{}", util::greet("main"));
    println!("{}", hack::run());
    println!("{}", corp::run());
    println!("{} words", util::word_count());
}
