// Trait impls and type aliases of a `mod` file work in the file that loads it: `Deref` (and
// `DerefMut`) coercions, `&**h`, autoderef methods, an `impl Iterator`, `pub type` aliases.
mod net;

use net::{Countdown, Host, Hosts, Op, Stack};

fn shout(s: &str) -> String {
    s.to_uppercase()
}

fn apply(f: Op, a: i64, b: i64) -> i64 {
    f(a, b)
}

fn main() {
    let h = Host("n00dles".to_string());
    println!("{} {} {}", shout(&h), h.len(), &*h == "n00dles");
    let r = &h;
    println!("{} {}", &**r == "n00dles", h.starts_with("n0"));
    let hosts: Hosts = vec![h.clone(), Host("joes".into())];
    let names: Vec<&str> = hosts.iter().map(|h| &**h).collect();
    println!("{names:?} [{:>8}]", hosts[1]);
    let mut s = Stack(vec![1, 2]);
    s.push(3);
    println!("{} {:?}", s.len(), s.iter().sum::<i32>());
    let total: u32 = Countdown(4).sum();
    println!("{total} {:?}", Countdown(3).collect::<Vec<_>>());
    let ops: Vec<Op> = vec![net::add, |a, b| a * b];
    println!("{} {}", apply(ops[0], 2, 3), apply(ops[1], 2, 3));
}
