// A contract solver with its tests, all passing: helpers and types in the test module, a
// nested test module, a test outside any module, and code that exists only in test builds.

/// "Total Ways to Sum": the partitions of `n` into at least two positive parts.
fn ways_to_sum(n: usize) -> u64 {
    let mut ways = vec![0u64; n + 1];
    ways[0] = 1;
    for part in 1..n {
        for total in part..=n {
            ways[total] += ways[total - part];
        }
    }
    ways[n]
}

#[cfg(test)]
const KNOWN: [(usize, u64); 4] = [(2, 1), (5, 6), (10, 41), (20, 626)];

#[cfg(not(test))]
const KNOWN: [(usize, u64); 0] = [];

fn main() {
    println!("{} known", KNOWN.len());
    println!("{}", ways_to_sum(50));
}

#[test]
fn top_level() {
    assert!(ways_to_sum(3) == 2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Case {
        n: usize,
        want: u64,
    }

    fn cases() -> Vec<Case> {
        KNOWN.iter().map(|&(n, want)| Case { n, want }).collect()
    }

    #[test]
    fn known_values() {
        for c in cases() {
            assert_eq!(ways_to_sum(c.n), c.want, "n = {}", c.n);
        }
    }

    #[test]
    fn grows() {
        let v: Vec<u64> = (2..12).map(ways_to_sum).collect();
        assert!(v.windows(2).all(|w| w[0] < w[1]), "{v:?}");
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn indexes_past_the_end() {
        let v = vec![1, 2, 3];
        let i = v.len() + 2;
        if i >= v.len() {
            panic!("index {i} out of range");
        }
    }

    mod nested {
        use super::super::ways_to_sum;

        #[test]
        fn one() -> Result<(), String> {
            if ways_to_sum(1) == 0 { Ok(()) } else { Err("one has a split".into()) }
        }
    }
}
