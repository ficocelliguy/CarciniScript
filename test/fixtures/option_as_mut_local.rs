// A binding through `&mut local` / `local.as_mut()` / `match &mut x` handed to a `&mut i32` or
// `&mut String` parameter: the local lives in a cell, and the callee's writes reach it.
fn bump(n: &mut i32) {
    *n += 1;
}

fn grow(s: &mut String) {
    s.push('!');
}

fn main() {
    let mut o: Option<i32> = Some(5);
    if let Some(n) = o.as_mut() {
        bump(n)
    }
    match o.as_mut() {
        Some(n) => bump(n),
        None => {}
    }
    if let Some(n) = &mut o {
        bump(n);
        *n += 10;
    }
    println!("{:?}", o);

    let mut s: Option<String> = Some("hi".to_string());
    if let Some(t) = s.as_mut() {
        grow(t);
        t.push('?');
    }
    match &mut s {
        Some(t) => grow(t),
        None => {}
    }
    println!("{:?}", s);

    let mut v: Option<Vec<i32>> = Some(vec![1]);
    if let Some(w) = v.as_mut() {
        w.push(2);
    }
    println!("{:?}", v);

    let mut c = Some(1);
    let mut f = || {
        if let Some(n) = c.as_mut() {
            bump(n);
            *n *= 3;
        }
    };
    f();
    f();
    println!("{:?}", c);

    let mut x = 3;
    match &mut x {
        n => bump(n),
    }
    let mut b = true;
    match &mut b {
        y => *y = !*y,
    }
    println!("{} {}", x, b);
}
