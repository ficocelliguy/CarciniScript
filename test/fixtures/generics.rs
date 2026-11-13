// Generic functions: type parameters with bounds (inline and `where`), instantiation at each call
// (including turbofish), generic values compared, added, cloned, formatted and collected, and
// generics over user structs.
use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct Score(u32);

fn largest<T: PartialOrd + Copy>(items: &[T]) -> T {
    let mut best = items[0];
    for &x in items {
        if x > best {
            best = x;
        }
    }
    best
}

fn count_eq<T>(items: &[T], target: &T) -> usize
where
    T: PartialEq,
{
    items.iter().filter(|x| *x == target).count()
}

fn sum_all<T: Copy + std::ops::Add<Output = T>>(items: &[T], zero: T) -> T {
    let mut total = zero;
    for &x in items {
        total = total + x;
    }
    total
}

fn label<T: Display>(name: &str, value: T) -> String {
    format!("{}={}", name, value)
}

fn pair_up<A: Clone, B: Clone>(a: &A, b: &B) -> (A, B) {
    (a.clone(), b.clone())
}

fn first_or<T: Clone>(items: &[T], fallback: T) -> T {
    match items.first() {
        Some(x) => x.clone(),
        None => fallback,
    }
}

fn repeat_all<T: Clone>(items: &[T], n: usize) -> Vec<T> {
    let mut out = Vec::new();
    for _ in 0..n {
        for x in items {
            out.push(x.clone());
        }
    }
    out
}

fn main() {
    println!("{} {} {:?}", largest(&[3, 9, 2]), largest(&[1.5, -2.0]), largest(&['x', 'b', 'z']));
    println!("{:?}", largest(&[Score(4), Score(11), Score(7)]));
    println!("{} {}", count_eq(&[1, 2, 1, 1], &1), count_eq(&["a", "b"], &"c"));
    println!("{} {}", sum_all(&[1, 2, 3], 0), sum_all(&[0.5, 0.25], 0.0));
    println!("{} {} {}", label("hp", 42), label("name", "ada"), label("ratio", 0.5));
    let (a, b) = pair_up(&String::from("x"), &vec![1, 2]);
    println!("{} {:?}", a, b);
    let empty: Vec<i32> = Vec::new();
    println!("{} {}", first_or(&empty, -1), first_or(&[7, 8], -1));
    println!("{:?}", repeat_all(&["ab", "c"], 2));
    let picked = largest::<u8>(&[200, 3]);
    println!("{}", picked);
    let f = largest::<i64>;
    println!("{}", f(&[5, 50, 25]));
}
