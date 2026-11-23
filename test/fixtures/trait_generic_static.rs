// A trait's own generic associated function called through a type parameter:
// `O::select(x)` / `O::select::<u8>(x)` with `O: Order`.
trait Order { fn select<R: Copy>(a: R, b: R) -> R; }
struct First;
struct Second;
impl Order for First { fn select<R: Copy>(a: R, _b: R) -> R { a } }
impl Order for Second { fn select<R: Copy>(_a: R, b: R) -> R { b } }
fn go<O: Order>(x: u8, y: u8) -> u8 { O::select(x, y) + O::select::<u8>(y, x) }
fn pick<O: Order>(a: &str, b: &str) -> String { O::select(a, b).to_string() }
fn main() {
    println!("{} {}", go::<First>(3, 10), go::<Second>(3, 10));
    println!("{} {}", pick::<First>("a", "b"), pick::<Second>("a", "b"));
}
