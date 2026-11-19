#[path = "../lib/util.rs"]
mod util;

use util::helpers::double;

pub fn run() -> String {
    format!("hack: {} {}", util::greet("hack"), double(21))
}
