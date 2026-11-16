// `LazyLock` / `OnceLock` statics, `once_cell::sync::Lazy`, `cfg_if!` and `either::Either`.
use either::Either;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock};

static PRIMES: LazyLock<Vec<u32>> = LazyLock::new(|| {
    (2..30u32)
        .filter(|n| (2..*n).all(|d| n % d != 0))
        .collect()
});

static NAMES: Lazy<HashMap<&'static str, u8>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("one", 1);
    m.insert("two", 2);
    m
});

static CONFIG: OnceLock<String> = OnceLock::new();

cfg_if::cfg_if! {
    if #[cfg(feature = "nope")] {
        fn platform() -> &'static str { "windows" }
    } else if #[cfg(all(not(feature = "x"), debug_assertions, not(debug_assertions)))] {
        fn platform() -> &'static str { "wasm" }
    } else {
        fn platform() -> &'static str { "other" }
    }
}

fn classify(s: &str) -> Either<i64, String> {
    match s.parse::<i64>() {
        Ok(n) => Either::Left(n),
        Err(_) => Either::Right(s.to_uppercase()),
    }
}

fn main() {
    println!("{:?} {}", *PRIMES, PRIMES.len());
    println!("{} {:?}", NAMES["two"], NAMES.get("three"));
    println!("{:?}", CONFIG.get());
    println!("{:?} {:?}", CONFIG.set("fast".to_string()), CONFIG.set("slow".to_string()));
    println!("{:?} {}", CONFIG.get(), CONFIG.get_or_init(|| "never".to_string()));
    let cache: OnceLock<u32> = OnceLock::new();
    println!("{} {}", cache.get_or_init(|| 7), cache.get_or_init(|| 8));
    println!("{}", platform());
    for s in ["42", "hi"] {
        let e = classify(s);
        println!("{} {} {:?} {:?}", e, e.is_left(), e.clone().left(), e.clone().right());
        println!("{:?}", e.clone().map_left(|n| n * 2).map_right(|s| s.len()));
        println!("{}", e.either(|n| n.to_string(), |s| s + "!"));
    }
    let f: Either<u8, char> = Either::Right('x');
    println!("{:?} {}", f.flip(), f.right_or('z'));
}
