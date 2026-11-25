// Coding contracts: "Array Jumping Game" and "Array Jumping Game II"
// I: can you reach the last index? II: fewest jumps to get there (0 if you can't).
use std::collections::HashMap;

fn can_reach_end(jumps: &[usize]) -> bool {
    let mut furthest = 0;
    for (i, &j) in jumps.iter().enumerate() {
        if i > furthest {
            return false;
        }
        furthest = furthest.max(i + j);
    }
    true
}

fn min_jumps(jumps: &[usize]) -> usize {
    let n = jumps.len();
    let mut count = 0;
    let mut end = 0;
    let mut furthest = 0;

    for i in 0..jumps.len() - 1 {
        furthest = furthest.max(i + jumps[i]);
        if i == end {
            if furthest == end {
                return 0;
            }
            count += 1;
            end = furthest;
        }
    }
    count
}

fn parse(s: &str) -> Vec<usize> {
    s.split(',').map(|x| x.trim().parse().unwrap()).collect()
}

fn main() {
    let inputs = [
        "3,0,2,0,1,0,4,2,1",
        "2,3,1,1,4",
        "0,5,1",
        "1,1,1,1,1",
        "4,0,0,0,0,2,0,1",
        "7",
    ];

    for input in inputs {
        let jumps = parse(input);
        println!(
            "[{:<20}] I: {:<5} II: {}",
            input,
            can_reach_end(&jumps) as u8,
            min_jumps(&jumps)
        );
    }
}
