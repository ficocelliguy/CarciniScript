// The crate root's `#![recursion_limit]` holds in its module files too: `tally.rs` expands a
// macro 140 levels deep, past rustc's default 128.
#![recursion_limit = "200"]

mod tally;

fn main() {
    println!("{}", tally::total());
    println!("{}", tally::nested::words());
}
