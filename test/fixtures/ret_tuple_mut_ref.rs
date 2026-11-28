// A `&mut` inside a tuple your own function returns is a `&mut` binding: written through
// without `mut`, and the writes land in the caller's value.
fn view<'a>(s: &'a [u32], d: &'a mut [u32]) -> (&'a [u32], &'a mut [u32]) {
    (s, d)
}

fn split(v: &mut Vec<u32>) -> (usize, &mut Vec<u32>) {
    (v.len(), v)
}

struct Acc {
    total: u32,
    log: Vec<String>,
}

fn pick<'a>(a: &'a mut Acc, b: &'a Acc) -> (&'a mut Acc, &'a Acc) {
    (a, b)
}

fn copy_into(src: &[u32], dst: &mut [u32]) {
    let (s, d) = view(src, dst);
    let n = s.len().min(d.len());
    let x = &mut d[..n];
    for i in 0..n {
        x[i] = s[i] * 10;
    }
    d[n - 1] += 1;
}

fn main() {
    let src = vec![1, 2, 3];
    let mut dst = vec![0; 5];
    copy_into(&src, &mut dst);
    println!("{:?}", dst);

    let mut v = vec![4, 5];
    let (n, w) = split(&mut v);
    w.push(n as u32);
    println!("{:?}", v);

    let mut a = Acc { total: 1, log: Vec::new() };
    let b = Acc { total: 7, log: vec!["b".to_string()] };
    let (x, y) = pick(&mut a, &b);
    x.total += y.total;
    x.log.push(format!("{}", y.log.len()));
    println!("{} {:?}", a.total, a.log);
}
