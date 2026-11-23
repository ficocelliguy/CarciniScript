// Your trait's method on an unsuffixed literal: the literal takes the type of the one impl
// (`5.seconds()` with `impl Secs for i64`), or the fallback `i32` / `f64` among several.
trait Secs { fn seconds(self) -> i64; }
impl Secs for i64 { fn seconds(self) -> i64 { self * 1000 } }
trait Wide { fn wide(self) -> String; }
impl Wide for i32 { fn wide(self) -> String { format!("i32 {}", self) } }
impl Wide for u64 { fn wide(self) -> String { format!("u64 {}", self) } }
trait Half { fn half(self) -> f32; }
impl Half for f32 { fn half(self) -> f32 { self / 2.0 } }
fn main() {
    println!("{} {}", 5.seconds(), 3_000_000_000.seconds());
    println!("{} {}", 7.wide(), (-2).wide());
    println!("{}", 3.0.half());
}
