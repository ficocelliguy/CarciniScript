fn f() -> &'static str {
    let s = String::from("a");
    &s
}
fn main() { f(); }
