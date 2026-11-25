// Coding contract: "Find Largest Prime Factor".
fn largest_prime_factor(mut n: u64) -> u64 {
    let mut factor = 2;
    let mut largest = 1;
    while factor * factor <= n {
        while n % factor == 0 {
            largest = factor;
            n /= factor;
        }
        factor += 1;
    }
    if n > 1 { n } else { largest }
}

fn main() {
    let inputs = [91, 600_851_475_143, 2, 1_048_576, 999_999_000_001, 7_919 * 7_907];
    for n in inputs {
        println!("{:>16} -> {}", n, largest_prime_factor(n));
    }
}
