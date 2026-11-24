fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    println!("{}", add(1, 2));
}

#[cfg(not(test))]
fn only_normal() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds() {
        println!("hidden");
        assert_eq!(add(2, 2), 4);
    }

    #[test]
    fn bad_add() {
        println!("shown on failure");
        assert_eq!(add(2, 2), 5);
    }

    #[test]
    #[should_panic]
    fn panics() {
        panic!("boom");
    }

    #[test]
    #[should_panic(expected = "bang")]
    fn wrong_panic() {
        panic!("boom");
    }

    #[test]
    #[should_panic]
    fn no_panic() {}

    #[test]
    #[ignore]
    fn slow() {}

    #[test]
    #[ignore = "too slow"]
    fn slower() {}

    #[test]
    fn result_err() -> Result<(), String> {
        Err("nope".to_string())
    }

    #[test]
    fn result_ok() -> Result<(), String> {
        Ok(())
    }
}
