// A trait implemented for both `Stdout` and `StdoutLock` (`Stderr`, `Stdin` too): the locks
// are their own types.
use std::io::{BufRead, Write};

trait Name {
    fn name(&self) -> &'static str;
}
impl Name for std::io::Stdout {
    fn name(&self) -> &'static str {
        "stdout"
    }
}
impl Name for std::io::StdoutLock<'_> {
    fn name(&self) -> &'static str {
        "stdout lock"
    }
}
impl Name for std::io::Stderr {
    fn name(&self) -> &'static str {
        "stderr"
    }
}
impl Name for std::io::StderrLock<'_> {
    fn name(&self) -> &'static str {
        "stderr lock"
    }
}

fn main() {
    let out = std::io::stdout();
    println!("{}", out.name());
    let mut lock = out.lock();
    println!("{}", lock.name());
    writeln!(lock, "through the lock").unwrap();
    lock.flush().unwrap();
    drop(lock);
    let err = std::io::stderr();
    println!("{} / {}", err.name(), err.lock().name());
    // A script has no input: the lock is a `BufRead` at its end.
    let n = std::io::stdin().lock().lines().count();
    println!("{n} lines");
}
