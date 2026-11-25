// Coding contract: "Total Ways to Sum II"
// How many ways can `target` be written as a sum of numbers from the given set (unlimited use)?
use std::collections::BTreeMap;

fn parse_contract(line: &str) -> Option<(u64, Vec<u64>)> {
    // e.g. "64: 1,3,4,6,7,9,10"
    let (target, rest) = line.split_once(':')?;
    let target = target.trim().parse().ok()?;
    let coins = rest
        .split(',')
        .map(|s| s.trim().parse().ok())
        .collect::<Option<Vec<u64>>>()?;
    Some((target, coins))
}

fn count_ways(target: u64, coins: Vec<u64>) -> u64 {
    let mut ways = vec![0u64; target as usize + 1];
    ways[0] = 1;
    for coin in coins {
        let coin = coin as usize;
        for amount in coin..=target as usize {
            ways[amount] += ways[amount - coin];
        }
    }
    println!("  ({} coins, table size {})", coins.len(), ways.len());
    ways[target as usize]
}

fn main() {
    let lines = [
        "64: 1,3,4,6,7,9,10",
        "10: 2,3,5",
        "100: 1,5,10,25,50",
        "37: 4,6,9",
        "oops: 1,2",
    ];

    let mut results = BTreeMap::new();
    for line in lines {
        match parse_contract(line) {
            Some((target, coins)) => {
                let n = count_ways(target, coins);
                results.insert(target, n);
            }
            None => println!("could not parse {line:?}"),
        }
    }

    for (target, ways) in &results {
        println!("{target:>4}: {ways}");
    }
}
