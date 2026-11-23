// `Some(&mut x)` of a number, string or `Option` matched by `let`-`else`, `if let` or a tuple
// pattern: the binding is a slot, written through without `mut`.
fn main() {
    let mut x = 1i32;
    let (Some(a), b) = (Some(&mut x), 2) else { return };
    *a += b;
    println!("{}", x);
    let mut y = String::from("s");
    let Some(p) = Some(&mut y) else { return };
    p.push('t');
    println!("{}", y);
    if let Some(q) = Some(&mut x) { *q *= 10; }
    if let (Some(r), Some(s)) = (Some(&mut x), Some(&mut y)) { *r += 1; s.push('!'); }
    let mut o = Some(5u8);
    let Some(w) = Some(&mut o) else { return };
    *w = None;
    println!("{} {} {:?}", x, y, o);
}
