// "Array Jumping Game" and "Encryption II: Vigenère Cipher", with tests next to them.

fn can_reach_end(jumps: &[usize]) -> bool {
    let mut reach = 0;
    for (i, &j) in jumps.iter().enumerate() {
        if i > reach {
            return false;
        }
        reach = reach.max(i + j);
    }
    true
}

fn vigenere(plain: &str, key: &str) -> String {
    plain
        .bytes()
        .zip(key.bytes().cycle())
        .map(|(p, k)| ((p - b'A' + k - b'A') % 26 + b'A') as char)
        .collect()
}

fn main() {
    println!("{}", can_reach_end(&[2, 3, 1, 1, 4]));
    println!("{}", vigenere("DASHBOARD", "LINUX"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_end() {
        assert!(can_reach_end(&[2, 3, 1, 1, 4]));
        assert!(can_reach_end(&[0]));
    }

    #[test]
    fn stuck_on_zero() {
        assert!(!can_reach_end(&[3, 2, 1, 0, 4]));
    }

    #[test]
    fn vigenere_matches_game() {
        assert_eq!(vigenere("DASHBOARD", "LINUX"), "OIFBYZIEX");
    }

    #[test]
    #[should_panic]
    fn vigenere_rejects_lowercase() {
        vigenere("abc", "KEY");
    }

    #[test]
    fn wrong_expectation() {
        assert_eq!(vigenere("A", "B"), "C", "shift by one");
    }
}
