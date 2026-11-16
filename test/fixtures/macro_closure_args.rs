// A closure with several parameters inside a macro's arguments (`vec![|a, b| a + b]`): its
// parameter list's commas don't split the arguments.
fn main() {
    let ops: Vec<fn(i32, i32) -> i32> = vec![|a, b| a + b, |a, b| a * b, |_, b| b];
    let rs: Vec<i32> = ops.iter().map(|f| f(3, 4)).collect();
    println!("{rs:?}");
    let pick = |a: i32, b: i32| if a > b { a } else { b };
    println!("{} {}", pick(1, 2), (|x: i32, y: i32| x - y)(9, 4));
    assert_eq!((|a: u8, b: u8| a | b)(1, 2), 3);
    let flags = 1 | 2;
    println!("{}", matches!(flags, 1 | 3));
}
