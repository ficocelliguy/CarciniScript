fn checked_div(a: i32, b: i32) -> i32 {
    if b == 0 {
        panic!("cannot divide {} by zero", a);
    }
    a / b
}

fn main() {
    println!("{}", checked_div(10, 2));
    assert_eq!(checked_div(9, 3), 3);
    assert!(1 + 1 == 2);
    println!("{}", checked_div(1, 0));
    println!("unreachable");
}
