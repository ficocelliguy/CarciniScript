// `for x in &mut v[a..b]` (of an array or a Vec, open or closed ranges): the loop goes over a
// view of that part, so `*x = ..` writes `v`.
fn bump(n: &mut i32) {
    *n += 1;
}

fn main() {
    let mut buf = [0u8; 6];
    let n = 3;
    for x in &mut buf[..n] {
        *x = 1;
    }
    println!("{:?}", buf);

    let mut v = vec![1, 2, 3, 4, 5];
    for x in &mut v[2..] {
        *x *= 10;
    }
    for x in &mut v[1..3] {
        *x += 1;
    }
    for x in &mut v[..=1] {
        bump(x);
    }
    println!("{:?}", v);

    let mut a = [1, 2, 3, 4];
    for (i, x) in (&mut a[1..]).iter_mut().enumerate() {
        *x += i as i32;
    }
    for x in &mut a[..2] {
        bump(x);
    }
    println!("{:?}", a);
}
