// A trait implemented for both a borrowed type and its owned twin (`str` / `String`, `[T]` /
// `Vec<T>`, `Path` / `PathBuf`), the owned one handing on to the borrowed one: one type in JS,
// served by the borrowed type's impl.
use std::path::{Path, PathBuf};

trait Loud {
    fn loud(&self) -> String;
}

impl Loud for str {
    fn loud(&self) -> String {
        self.to_uppercase()
    }
}

impl Loud for String {
    fn loud(&self) -> String {
        self.as_str().loud()
    }
}

trait Total {
    fn total(&self) -> i64;
}

impl Total for [i64] {
    fn total(&self) -> i64 {
        self.iter().sum()
    }
}

impl Total for Vec<i64> {
    fn total(&self) -> i64 {
        self.as_slice().total()
    }
}

trait Script {
    fn is_script(&self) -> bool;
}

impl Script for Path {
    fn is_script(&self) -> bool {
        self.extension().is_some_and(|e| e == "js" || e == "rs")
    }
}

impl Script for PathBuf {
    fn is_script(&self) -> bool {
        self.as_path().is_script()
    }
}

fn main() {
    println!("{} {}", "a".loud(), String::from("b").loud());
    let v: Vec<i64> = vec![1, 2, 3];
    println!("{} {}", v.total(), v[1..].total());
    let a = Path::new("hack.rs");
    let b = PathBuf::from("notes.txt");
    println!("{} {}", a.is_script(), b.is_script());
}
