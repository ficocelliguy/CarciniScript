// `&mut v[a..b]` views: the Array methods on them run on the base array, nested views flatten,
// and `v[a..b].sort()` on a large range writes back without spreading.
fn fiddle(s: &mut [i32]) {
    s.sort();
    s.reverse();
    s.swap(0, s.len() - 1);
    s[1] += 1000;
    let last = s.len() - 1;
    s[last] -= 1;
}

fn stats(s: &mut [i32]) -> (i32, usize, bool, Option<i32>, Option<i32>) {
    let total: i32 = s.iter().sum();
    let n = s.iter().filter(|x| **x > 0).count();
    (total, n, s.contains(&-5), s.first().copied(), s.last().copied())
}

fn inner(s: &mut [i32]) {
    let n = s.len();
    if n > 2 {
        inner(&mut s[1..n - 1]);
    }
    for x in s.iter_mut() {
        *x += 1;
    }
}

fn halves(s: &mut [i32]) {
    let n = s.len() / 2;
    let (a, b) = s.split_at_mut(n);
    a.fill(7);
    b.reverse();
    for c in s.chunks_mut(3) {
        c[0] = -c[0];
    }
}

fn describe(s: &[i32]) -> String {
    format!("{:?} {} {:?} {:?}", s, s.len(), s.iter().rev().collect::<Vec<_>>(), s.iter().step_by(2).copied().collect::<Vec<_>>())
}

fn main() {
    let mut v: Vec<i32> = vec![5, -5, 3, 9, 1, 0, -2, 8, 4, 6];
    fiddle(&mut v[2..8]);
    println!("{:?}", v);
    let r = stats(&mut v[1..5]);
    println!("{:?}", r);
    inner(&mut v[..]);
    println!("{:?}", v);
    inner(&mut v[3..]);
    println!("{:?}", v);
    halves(&mut v[1..9]);
    println!("{:?}", v);
    {
        let s = &mut v[2..7];
        println!("{}", describe(s));
        println!("{:?} {:?} {:?}", s.to_vec(), s.iter().position(|x| *x == 7), s.binary_search(&7).is_ok());
        println!("{:?} {:?}", s.windows(2).map(|w| w[0] + w[1]).collect::<Vec<_>>(), s.chunks(2).map(|c| c.len()).collect::<Vec<_>>());
        s.rotate_left(2);
        println!("{:?}", s);
        s.copy_from_slice(&[1, 2, 3, 4, 5]);
        let t = &mut s[1..4];
        t.reverse();
        println!("{:?} {}", t, t == [4, 3, 2]);
        println!("{:?} {:?}", &s[..2], s.iter().map(|x| x * 2).collect::<Vec<_>>());
    }
    println!("{:?}", v);
    v[1..5].sort_unstable();
    v[..3].reverse();
    v[4..].fill(2);
    println!("{:?}", v);
    let mut big: Vec<u32> = (0..200_000).map(|i| (i * 7919) % 100_003).collect();
    big[10..199_990].sort();
    big[100..100_100].reverse();
    big[5..15].rotate_right(3);
    println!("{} {} {} {}", big[10], big[199_989], big[100], big.iter().map(|x| *x as u64).sum::<u64>());
    let mut view_of_view = vec![1, 2, 3, 4, 5, 6, 7, 8];
    {
        let a = &mut view_of_view[1..7];
        let b = &mut a[1..5];
        let c = &mut b[1..3];
        c.swap(0, 1);
        c[0] += 10;
        b.sort();
        println!("{:?} {:?} {}", b, b.len(), b.iter().any(|x| *x == 14));
    }
    println!("{:?}", view_of_view);
    let mut strs = vec!["d".to_string(), "b".to_string(), "c".to_string(), "a".to_string()];
    strs[1..].sort();
    let s = &mut strs[..2];
    s[0].push('!');
    println!("{:?}", s);
    println!("{:?}", strs);
    let mut empty: Vec<i32> = vec![1, 2];
    let e = &mut empty[1..1];
    println!("{} {:?} {:?} {:?}", e.len(), e, e.iter().sum::<i32>(), e.first());
    e.sort();
    println!("{:?}", empty);
}
