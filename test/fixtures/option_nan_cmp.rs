// Partial comparisons that reach a NaN follow `f64`'s rules: `Some(NaN) <= Some(1.0)` is false,
// `partial_cmp` gives `None`, and `None` is still below every `Some` (even `Some(NaN)`). The same
// holds inside tuples, Vecs, derived `PartialOrd` structs and iterator comparisons.
#[derive(Debug, PartialEq, PartialOrd)]
struct P {
    x: f64,
    y: f64,
}

fn main() {
    let a = Some(f64::NAN);
    let b = Some(1.0);
    let n: Option<f64> = None;
    println!("{} {} {} {}", a < b, a <= b, a > b, a >= b);
    println!("{} {} {} {}", b < a, b <= a, b > a, b >= a);
    println!("{} {} {}", a == a, a != a, a == b);
    println!("{:?} {:?} {:?}", a.partial_cmp(&b), b.partial_cmp(&a), a.partial_cmp(&a));
    println!("{:?} {:?}", n.partial_cmp(&a), a.partial_cmp(&n));
    println!("{} {} {} {}", n < a, a > n, n <= b, a >= n);
    println!("{:?} {:?}", b.partial_cmp(&Some(2.0)), b.partial_cmp(&b));
    let v = vec![Some(3.0), None, Some(f64::NAN)];
    println!("{} {} {}", v[0] > v[2], v[1] < v[2], v[2] <= v[2]);
    let t = (1.0, f64::NAN);
    println!("{} {} {:?}", t < (1.0, 2.0), t >= (1.0, 2.0), t.partial_cmp(&(1.0, 2.0)));
    println!("{:?}", t.partial_cmp(&(0.0, 2.0)));
    let w = vec![1.0, f64::NAN];
    println!("{} {} {:?}", w < vec![1.0, 2.0], w > vec![0.5], w.partial_cmp(&vec![1.0, 3.0]));
    println!("{:?}", w.iter().partial_cmp([1.0, 2.0].iter()));
    println!("{:?}", [1.0, 2.0].iter().partial_cmp([1.0, 3.0].iter()));
    println!("{}", w.iter().lt([1.0, 2.0].iter()));
    let p = P { x: 1.0, y: f64::NAN };
    let q = P { x: 1.0, y: 2.0 };
    println!("{} {} {:?} {:?}", p < q, p >= q, p.partial_cmp(&q), q.partial_cmp(&P { x: 2.0, y: 0.0 }));
    let o: Option<(f64, u8)> = Some((f64::NAN, 1));
    println!("{} {}", o < Some((0.0, 0)), o > None);
    println!("{} {}", [1.0, f64::NAN].is_sorted(), [1.0, 2.0].is_sorted());
}
