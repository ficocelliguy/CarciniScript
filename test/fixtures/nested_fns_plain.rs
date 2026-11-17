fn a() -> i32 {
    fn h() -> i32 {
        1
    }
    h()
}
fn b() -> i32 {
    fn h() -> i32 {
        2
    }
    h()
}
fn main() {
    println!("{}", a() + b());
}
