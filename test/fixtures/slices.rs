// Range indexing: `&v[a..b]` (all range forms) on Vecs and arrays, slices passed to functions and
// iterated, and `&s[a..b]` on strings with UTF-8 byte offsets.
fn sum(xs: &[i32]) -> i32 {
    xs.iter().sum()
}

fn main() {
    let v = vec![10, 20, 30, 40, 50];
    println!("{:?} {:?} {:?} {:?} {:?}", &v[1..3], &v[..2], &v[3..], &v[..], &v[1..=3]);
    println!("{} {}", sum(&v[2..]), sum(&v[..0]));
    let arr = [1, 2, 3, 4];
    let (left, right) = (&arr[..2], &arr[2..]);
    println!("{:?} {:?} {}", left, right, left.len() + right.len());
    let mid = v[1..4].to_vec();
    println!("{:?} {:?}", mid, v[1..4].iter().rev().collect::<Vec<_>>());
    for w in v[..3].windows(2) {
        print!("{:?}", w);
    }
    println!();
    let s = "héllo wörld";
    println!("{} {} {}", &s[0..1], &s[1..3], &s[7..]);
    let word = &s[..6];
    println!("{:?} {}", word, word.len());
    let host = "home.server";
    let dot = host.len() - 7;
    println!("{}", &host[..dot - 1]);
}
