// Indexing a `Vec<&mut T>` collected from `iter_mut()` over numbers, strings and bools: each item
// is a slot over the element, read and written through (`*rs[1] = 9`, `ss[0].push(..)`), and
// handed on to `&mut i32` parameters (`bump(rs[0])`, `bump(&mut rs[2])`).
fn bump(r: &mut i32) {
    *r += 100;
}

fn show(r: &i32) -> i32 {
    *r * 2
}

fn main() {
    let mut v = vec![1, 2, 3];
    let mut rs: Vec<&mut i32> = v.iter_mut().collect();
    *rs[1] = 9;
    *rs[0] += 5;
    println!("{}", rs[2]);
    let x = *rs[1] + 1;
    println!("{}", x);
    println!("{:?}", v);

    let mut f = vec![1.5, 2.5];
    let mut fs: Vec<&mut f64> = f.iter_mut().collect();
    *fs[0] *= 2.0;
    println!("{:?}", f);

    let mut s = vec![String::from("a"), String::from("b")];
    let mut ss: Vec<&mut String> = s.iter_mut().collect();
    *ss[1] = String::from("z");
    ss[0].push('x');
    println!("{} {}", ss[0], ss[1].len());
    println!("{:?}", s);

    let mut b = vec![true, false];
    let mut bs: Vec<&mut bool> = b.iter_mut().collect();
    *bs[1] = !*bs[0];
    println!("{:?}", b);

    let mut v = vec![4, 1, 3];
    let mut rs: Vec<&mut i32> = v.iter_mut().filter(|n| **n != 0).collect();
    bump(rs[0]);
    bump(&mut rs[2]);
    println!("{}", show(rs[1]));
    println!("{}", *rs[0] == 104);
    let i = rs.len() - 1;
    *rs[i] -= 1;
    rs.swap(0, 1);
    *rs[0] = 7;
    let total: i32 = rs.iter().map(|r| **r).sum();
    println!("{}", total);
    println!("{:?}", v);
}
