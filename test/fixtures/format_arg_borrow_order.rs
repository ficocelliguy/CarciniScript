// Format arguments are borrowed and evaluated in order, as `format_args!` does: an argument may
// change a variable that only later arguments (or the string's `{name}` captures, which come
// after them) read, and `{1} {0}` still runs the arguments first to last.
use std::fmt::Write;

fn bump(n: &mut i32) -> i32 {
    *n += 1;
    *n
}

fn say(s: &str, v: i32) -> i32 {
    println!("eval {s}");
    v
}

fn main() {
    let mut n = 1;
    println!("{} {}", bump(&mut n), n);
    let s = format!("{n} {}", bump(&mut n));
    println!("{s}");
    let mut v = vec![1];
    println!("{:?} {:?}", { v.push(2); v.len() }, v);
    let mut out = String::new();
    write!(out, "{} {n}", bump(&mut n)).unwrap();
    writeln!(out, " {}", out.len()).unwrap_or(());
    print!("{out}");
    assert_eq!(bump(&mut n), n);
    println!("{n}");

    // Positional and named arguments out of template order.
    println!("{1} {0}", say("a", 1), say("b", 2));
    println!("{x} {}", bump(&mut n), x = say("x", 0));
    println!("{0:>1$} {n}", bump(&mut n), say("w", 4) as usize);
    let f = |k: i32| format!("{1}-{0}", say("p", k), say("q", k + 1));
    println!("{}", f(7));
    let ok = n > 0 && format!("{1}{0}", say("l", 1), say("r", 2)) == "21";
    println!("{ok}");
}
