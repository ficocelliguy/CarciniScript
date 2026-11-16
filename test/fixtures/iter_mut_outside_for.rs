// `iter_mut()` over numbers and strings outside a `for` loop: `next()` in `if let` / `while let`
// (with `enumerate`, `zip`, `rev`, `skip`, `step_by`), `.unwrap()` into a `let`, collecting a
// `Vec<&mut T>` and writing through it. Every write must reach the Vec.
fn bump_all(v: &mut Vec<i32>) {
    let mut it = v.iter_mut().skip(1).step_by(2);
    while let Some(x) = it.next() {
        *x += 100;
    }
}

fn main() {
    let mut v = vec![1, 2, 3, 4, 5];
    let mut it = v.iter_mut();
    if let Some(x) = it.next() {
        *x = 9;
    }
    if let Some(y) = it.next() {
        *y *= 10;
    }
    println!("{:?}", v);

    let mut e = v.iter_mut().enumerate();
    while let Some((i, x)) = e.next() {
        *x += i as i32;
    }
    println!("{:?}", v);

    let w = vec![10, 20, 30];
    for (a, b) in v.iter_mut().zip(w.iter()) {
        *a -= *b;
    }
    println!("{:?}", v);

    let mut z = v.iter_mut().zip(w.iter());
    while let Some((a, b)) = z.next() {
        *a += *b * 2;
    }
    println!("{:?}", v);

    let refs: Vec<&mut i32> = v.iter_mut().collect();
    for r in refs {
        *r *= 2;
    }
    println!("{:?}", v);

    let mut evens: Vec<&mut i32> = v.iter_mut().rev().collect();
    for r in evens.iter_mut() {
        **r += 1;
    }
    println!("{} {:?}", evens.len(), v);

    let last = v.iter_mut().last().unwrap();
    *last = 0;
    let first = v.iter_mut().next().unwrap();
    *first = -1;
    println!("{:?}", v);

    if let Some(third) = v.iter_mut().nth(2) {
        *third = 333;
    }
    let n = v.iter_mut().count();
    println!("{:?} {}", v, n);

    bump_all(&mut v);
    println!("{:?}", v);

    let mut names = vec![String::from("a"), String::from("b")];
    let mut ni = names.iter_mut();
    while let Some(n) = ni.next() {
        n.push('!');
        *n = n.to_uppercase();
    }
    println!("{:?}", names);

    let mut flags = [false; 3];
    let mut fi = flags.iter_mut().enumerate();
    while let Some((i, f)) = fi.next() {
        *f = i % 2 == 0;
    }
    println!("{:?}", flags);
}
