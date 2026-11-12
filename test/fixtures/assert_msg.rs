fn tri(n: u32) -> u32 {
    let mut t = 0;
    for i in 1..=n {
        t += i;
    }
    t
}

fn main() {
    assert_eq!(tri(3), 6);
    assert_ne!(tri(3), 7, "never printed {}", tri(100));
    println!("tri(10) = {}", tri(10));
    assert_eq!(tri(4), 11, "tri({}) should be {}", 4, "eleven");
}
