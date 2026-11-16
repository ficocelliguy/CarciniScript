fn a() -> i32 {
    fn imp() -> i32 {
        1
    }
    imp()
}
fn b() -> i32 {
    fn imp() -> i32 {
        2
    }
    println!("{}", imp());
    [1, 2].iter().map(|_| imp()).sum::<i32>()
}
fn imp() -> i32 {
    100
}
fn c() -> i32 {
    fn imp() -> i32 {
        fn imp() -> i32 {
            7
        }
        imp() * 10
    }
    let f = imp;
    f() + crate::imp()
}
fn main() {
    println!("{} {} {} {}", a(), b(), imp(), c());
}
