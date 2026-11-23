// Fields of a `Vec<(usize, &mut T)>` collected from `iter_mut()` over numbers and strings: the
// `&mut` field of an item is a slot over the element, read and written through (`*rs[1].1 = 9`,
// `ss[0].1.push(..)`) and handed on to `&mut i32` parameters (`bump(rs[2].1)`).
fn bump(r: &mut i32) {
    *r += 100;
}
fn main() {
    let mut v = vec![1, 2, 3];
    let mut rs: Vec<(usize, &mut i32)> = v.iter_mut().enumerate().collect();
    *rs[1].1 = 9;
    *rs[0].1 += 5;
    println!("{} {}", rs[2].0, rs[2].1);
    let x = *rs[1].1 * 2;
    println!("{}", x);
    bump(rs[2].1);
    bump(&mut rs[0].1);
    println!("{:?}", v);
    let mut s = vec![String::from("a"), String::from("b")];
    let mut ss: Vec<(usize, &mut String)> = s.iter_mut().enumerate().filter(|(i, _)| *i > 0).collect();
    ss[0].1.push('!');
    println!("{} {}", ss[0].0, ss[0].1);
    println!("{:?}", s);
    let mut w = vec![5, 6];
    let mut ps: Vec<(&mut i32, i32)> = w.iter_mut().zip(vec![10, 20]).collect();
    *ps[1].0 += ps[1].1;
    println!("{:?}", w);
}
