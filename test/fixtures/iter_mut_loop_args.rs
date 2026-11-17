fn bump(v: &mut i32) {
    *v += 1;
}
fn shout(s: &mut String) {
    s.push('!');
}
fn main() {
    let mut a = vec![1, 2, 3];
    for x in a.iter_mut() {
        bump(x);
    }
    let mut names = vec![String::from("a"), String::from("b")];
    for (i, s) in names.iter_mut().enumerate() {
        if i == 1 {
            shout(s);
        }
    }
    for x in &mut a {
        bump(x);
        bump(x);
    }
    println!("{a:?} {names:?}");
}
