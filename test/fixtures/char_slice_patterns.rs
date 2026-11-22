// A char slice or array as a pattern (Rust by Example's `std/str`), and `.eq()` called as a
// method on strings (`std/rc`).
use std::rc::Rc;

fn main() {
    let s = " ,hello, world,, ";
    println!("[{}]", s.trim_matches(&[' ', ','] as &[char]));
    println!("[{}]", s.trim_matches(&[' ', ',']));
    println!("[{}]", s.trim_matches([' ', ',']));
    let chars: &[char] = &[' ', ',', 'd'];
    println!("[{}]", s.trim_start_matches(chars));
    println!("[{}]", s.trim_end_matches(chars));
    let v: Vec<&str> = "a-b_c d".split(&['-', '_', ' '][..]).collect();
    println!("{:?}", v);
    let v: Vec<&str> = "a-b_c".split(['-', '_']).collect();
    println!("{:?}", v);
    println!("{:?} {:?}", "x=1;y".find(&['=', ';'][..]), "abc".find(['z']));
    println!("{}", "a,b;c".replace(&[',', ';'][..], " "));
    println!("{}", "a,b;c".replace([',', ';'], "+"));
    println!("{} {}", "hello".starts_with(['h', 'j']), "hello".ends_with(chars));
    println!("{}", "1,2".contains([',', ';']));
    println!("{:?}", "k:v=w".split_once([':', '=']));
    println!("{:?}", "a b,c".rsplit([' ', ',']).collect::<Vec<_>>());

    let a = String::from("hi");
    let b = String::from("hi");
    let r1 = Rc::new(String::from("rc"));
    let r2 = Rc::clone(&r1);
    let r3 = Rc::new(String::from("other"));
    println!("{} {} {}", a.eq(&b), a.eq("hi"), a.ne(&b));
    println!("{} {}", r1.eq(&r2), r1.eq(&r3));
    println!("{} {}", "x".eq("x"), 3.eq(&4));
    println!("{} {} {}", a.lt(&String::from("hj")), 2.5.ge(&2.5), 'a'.gt(&'b'));
    println!("{} {}", vec![1, 2].eq(&vec![1, 2]), Some(3).ne(&None));
    println!("{:?}", "a1b22c".split(|c: char| c.is_ascii_digit()).collect::<Vec<_>>());
    println!("{}", count_any("banana", &['a', 'n']));
}

fn count_any(s: &str, set: &[char]) -> usize {
    s.matches(set).count()
}
