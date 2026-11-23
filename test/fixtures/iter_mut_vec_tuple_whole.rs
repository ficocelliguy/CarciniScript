// Whole items of a `Vec<(usize, &mut T)>` collected from `iter_mut()` over numbers, strings and
// floats: bound by `let t = &mut rs[1]` / `let (i, r) = &mut rs[2]` / `for t in &rs` (`t.1` is the
// element's slot), and printed with `{:?}` (a copy with the slots read), as is a `Vec<&mut T>`.
fn bump(r: &mut i32) {
    *r += 100;
}
fn main() {
    let mut v = vec![1, 2, 3];
    let mut rs: Vec<(usize, &mut i32)> = v.iter_mut().enumerate().collect();
    println!("{:?}", rs);
    for (i, r) in rs.iter_mut() {
        **r += *i as i32;
    }
    for t in &rs {
        println!("{} {} {:?}", t.0, t.1, t);
    }
    for t in rs.iter_mut() {
        *t.1 += 1;
        bump(t.1);
    }
    println!("{:?} {:?}", rs[1], &rs[2]);
    let t = &mut rs[1];
    *t.1 += 40;
    println!("{} {}", t.0, t.1);
    let (i, r) = &mut rs[2];
    **r += 7;
    println!("{} {}", i, r);
    let u = &rs[0];
    let n = *u.1 + 1;
    println!("{} {:?}", n, u);
    let pairs = vec![(1, 2)];
    for t in &pairs {
        println!("{}", t.1);
    }
    println!("{:?}", v);
    let mut w = vec![String::from("a"), String::from("b")];
    let refs: Vec<&mut String> = w.iter_mut().collect();
    println!("{:?}", refs);
    let mut fs = vec![1.5, 2.0];
    let tf: Vec<(usize, &mut f64)> = fs.iter_mut().enumerate().collect();
    println!("{:?}", tf);
    println!("{tf:?}");
    for t in &tf {
        println!("{t:?}");
    }
}
