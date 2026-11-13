fn main() {
    let mut v: Vec<i32> = vec![1, 2, 3];
    let i = v.len() + 1;
    println!("before");
    v.swap(0, i);
    println!("{:?}", v);
}
