fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    let mut d = 2;
    while d * d <= n {
        if n % d == 0 {
            return false;
        }
        d += 1;
    }
    true
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn fizzbuzz(n: u32) -> String {
    if n % 15 == 0 {
        String::from("FizzBuzz")
    } else if n % 3 == 0 {
        "Fizz".to_string()
    } else if n % 5 == 0 {
        "Buzz".to_string()
    } else {
        n.to_string()
    }
}

// xorshift32: exercises u32 shifts and xor wrapping.
fn xorshift(mut x: u32) -> u32 {
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x
}

fn digit_sum(mut n: u64) -> u64 {
    let mut s = 0;
    while n > 0 {
        s += n % 10;
        n /= 10;
    }
    s
}

fn main() {
    let mut count = 0;
    let mut last = 0;
    for n in 0..10_000 {
        if is_prime(n) {
            count += 1;
            last = n;
        }
    }
    println!("{count} primes below 10000, largest {last}");
    println!("gcd(1071, 462) = {}", gcd(1071, 462));

    let mut sum: u64 = 0;
    for n in 1..=1000u64 {
        sum += n * n;
    }
    println!("sum of squares = {sum}");

    for i in 1..=15 {
        print!("{} ", fizzbuzz(i));
    }
    println!();

    let mut r: u32 = 2463534242;
    for _ in 0..5 {
        r = xorshift(r);
        print!("{r} ");
    }
    println!();
    println!("{:>12}|{:<12}|{:^12}|", r % 1000, digit_sum(9_007_199_254_740_991), r >> 20);

    let mut h: i32 = 0;
    for c in 0..200 {
        h = h * 31 + c;
    }
    println!("hash = {h}");
    println!("{:.4} {:.4} {:.4}", 2f64.sqrt(), std::f64::consts::PI.sin(), (1.0f64).exp());
}
