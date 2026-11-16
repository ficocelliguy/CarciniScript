// `include!`: Rust source from a file next to the script, as items and as an expression.
include!("helpers.in");

fn main() {
    let primes: Vec<u32> = include!("primes.in");
    println!("{:?}", primes);
    println!("{}", double(21));
    println!("{}", LIMIT);
}
