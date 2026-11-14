// Path-call syntax for methods of built-in types (`u64::pow(2, 40)`, `str::len(s)`) and such
// methods as values (`.fold(i32::MIN, i32::max)`, `.map(str::trim)`).
fn main() {
    let v = vec![3, -7, 5];
    println!("{}", v.iter().copied().fold(i32::MIN, i32::max));
    let a: Vec<i32> = v.iter().copied().map(i32::abs).collect();
    let words: Vec<&str> = vec![" a ", "b "].into_iter().map(str::trim).collect();
    let owned: Vec<String> = words.iter().copied().map(String::from).collect();
    let ok = "abc1".chars().all(char::is_alphanumeric);
    let up: String = "abc".chars().map(|c| c.to_ascii_uppercase()).collect();
    println!("{:?} {:?} {:?} {} {}", a, words, owned, ok, up);
    println!("{} {} {}", u64::pow(2, 40), f64::max(1.5, 2.5), i32::abs(-4));
    println!("{} {}", f64::sqrt(16.0), char::is_alphabetic('x'));
    println!("{} {}", str::len("hey"), i64::min(3, i64::from(2u8)));
}
