// Closure bounds in `where` clauses and inline, naming type parameters declared after them:
// `fn run<F, T>(f: F) -> T where F: FnOnce() -> T`.
use std::collections::HashMap;

fn run<F, T>(f: F) -> T
where
    F: FnOnce() -> T,
{
    f()
}

fn first<F: FnOnce() -> T, T>(f: F) -> T {
    f()
}

fn map_all<F, T, U>(v: Vec<T>, f: F) -> Vec<U>
where
    F: Fn(T) -> U,
{
    v.into_iter().map(f).collect()
}

fn retry<F, T, E>(mut attempt: F, tries: u32) -> Result<T, E>
where
    F: FnMut(u32) -> Result<T, E>,
{
    let mut last = attempt(0);
    for i in 1..tries {
        if last.is_ok() {
            break;
        }
        last = attempt(i);
    }
    last
}

fn group_by<K, V, F>(items: &[V], key: F) -> HashMap<K, Vec<V>>
where
    K: std::hash::Hash + Eq,
    V: Clone,
    F: Fn(&V) -> K,
{
    let mut out: HashMap<K, Vec<V>> = HashMap::new();
    for it in items {
        out.entry(key(it)).or_default().push(it.clone());
    }
    out
}

struct Memo<F, T>
where
    F: Fn(u64) -> T,
{
    f: F,
    hits: u32,
}

impl<F, T> Memo<F, T>
where
    F: Fn(u64) -> T,
{
    fn call(&mut self, x: u64) -> T {
        self.hits += 1;
        (self.f)(x)
    }
}

fn main() {
    println!("{} {}", run(|| 3), first(|| "s"));
    println!("{:?}", map_all(vec![1, 2, 3], |x| x * 10));
    println!("{:?}", map_all(vec!["a", "bb"], |s| s.len()));
    let r: Result<u32, String> = retry(|i| if i >= 2 { Ok(i * 100) } else { Err(format!("no {i}")) }, 5);
    let e: Result<u32, String> = retry(|i| Err(format!("fail {i}")), 3);
    println!("{r:?} {e:?}");
    let g = group_by(&[1, 2, 3, 4, 5, 6], |x| x % 3);
    let mut keys: Vec<_> = g.iter().collect();
    keys.sort();
    println!("{keys:?}");
    let mut m = Memo { f: |x: u64| x * x, hits: 0 };
    println!("{} {} {}", m.call(3), m.call(4), m.hits);
}
