// Format strings capture `const` and `static` items like locals (`"{CYAN}text{RESET}"`), also
// as a width or precision.
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";
const WIDTH: usize = 8;
const LIMIT: f64 = 2.5;
static GREETING: &str = "hello";
const PAIR: (i32, char) = (7, 'x');

fn main() {
    println!("{CYAN}text{RESET}");
    println!("[{GREETING:>WIDTH$}] [{LIMIT:.3}] [{LIMIT:>WIDTH$.1}] {PAIR:?}");
    let s = format!("{CYAN}{}{RESET}", 42);
    println!("{:?} {}", s, s.len());
    let width = 3;
    println!("[{GREETING:>width$}]");
    println!("{WIDTH:04} {:>1$}", "x", WIDTH);
}
