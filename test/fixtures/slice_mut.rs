// `&mut v[a..b]`: a view whose writes reach `v` (indexing, `iter_mut`, `swap`, `sort`,
// `copy_from_slice`), passed to functions, re-sliced (a recursive merge sort) and bound with `let`.
fn bump(s: &mut [i32]) {
    for x in s.iter_mut() {
        *x += 100;
    }
    s[0] = -1;
}

fn merge_sort(v: &mut [i32]) {
    let n = v.len();
    if n <= 1 {
        return;
    }
    let mid = n / 2;
    merge_sort(&mut v[..mid]);
    merge_sort(&mut v[mid..]);
    let merged: Vec<i32> = {
        let (a, b) = (v[..mid].to_vec(), v[mid..].to_vec());
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::with_capacity(n);
        while i < a.len() && j < b.len() {
            if a[i] <= b[j] {
                out.push(a[i]);
                i += 1;
            } else {
                out.push(b[j]);
                j += 1;
            }
        }
        out.extend_from_slice(&a[i..]);
        out.extend_from_slice(&b[j..]);
        out
    };
    v.copy_from_slice(&merged);
}

fn reverse_part(s: &mut [char]) {
    let n = s.len();
    for i in 0..n / 2 {
        s.swap(i, n - 1 - i);
    }
}

fn main() {
    let mut v = vec![5, 3, 8, 1, 9, 2, 7];
    bump(&mut v[2..5]);
    println!("{:?}", v);
    merge_sort(&mut v);
    println!("{:?}", v);
    let mut w = vec![9, 8, 7, 6, 5, 4];
    merge_sort(&mut w[1..=4]);
    let part = &mut w[3..];
    part.sort();
    println!("{:?} {}", w, w[1..3].len());
    let mut cs: Vec<char> = "abcdef".chars().collect();
    reverse_part(&mut cs[1..5]);
    println!("{}", cs.iter().collect::<String>());
}
