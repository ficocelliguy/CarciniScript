fn main() {
    let mut s = String::from("a");
    s = String::from("b");
    println!("{s}");
    let mut v = vec![1];
    v = vec![2];
    println!("{v:?}");
    let mut t = (1, String::new());
    t = (2, String::new());
    println!("{t:?}");
    let mut o: Option<i32> = None;
    o = Some(1);
    println!("{o:?}");
}
