fn main() {
    let found: Option<u8> = vec![1u8, 2].get(5).copied();
    println!("looking");
    let x = found.unwrap();
    println!("{x}");
}
