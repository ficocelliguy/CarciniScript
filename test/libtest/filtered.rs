// args: sum --include-ignored
// Name filters and `--include-ignored`, as `cargo test sum -- --include-ignored`.

fn sum(v: &[i32]) -> i32 {
    v.iter().fold(0, |a, &b| a + b)
}

fn product(v: &[i32]) -> i32 {
    v.iter().product()
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_empty() {
        assert_eq!(sum(&[]), 0);
    }

    #[test]
    #[ignore = "slow"]
    fn sum_big() {
        let v: Vec<i32> = (1..=1000).collect();
        assert_eq!(sum(&v), 500500);
    }

    #[test]
    fn product_small() {
        assert_eq!(product(&[2, 3]), 6);
    }

    #[test]
    fn sum_overflow() {
        eprint!("partial line");
        let big = [i32::MAX, 1];
        println!("{}", sum(&big));
    }
}
