fn main() {
    let v = vec![1, 2, 3];
    let n = v.len() + 2;
    println!("{:?}", &v[1..2]);
    println!("{:?}", &v[1..n]);
}
