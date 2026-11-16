// `debug_assert!` and friends: off in release builds, like rustc -O.
fn half(n: i32) -> i32 {
    debug_assert!(n % 2 == 0, "odd: {}", n);
    debug_assert_eq!(n % 2, 0);
    debug_assert_ne!(n, 7, "seven");
    n / 2
}

fn main() {
    println!("{}", half(10));
    println!(
        "{} {}",
        cfg!(debug_assertions),
        cfg!(all(not(test), any(debug_assertions, feature = "x")))
    );
    if cfg!(debug_assertions) {
        println!("debug build");
    }
    println!("{}", half(7));
}
