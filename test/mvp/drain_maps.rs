// A batch queue: jobs are collected per host, then drained and handed out in a stable order.
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
struct Job {
    script: &'static str,
    threads: u32,
}

fn main() {
    let mut pending: HashMap<String, Vec<Job>> = HashMap::new();
    let mut touched: HashSet<String> = HashSet::new();
    for (host, script, threads) in [
        ("n00dles", "hack.rs", 4),
        ("foodnstuff", "grow.rs", 8),
        ("n00dles", "weaken.rs", 2),
        ("zer0", "hack.rs", 16),
    ] {
        pending.entry(host.to_string()).or_default().push(Job { script, threads });
        touched.insert(host.to_string());
    }

    let mut batch: Vec<(String, Vec<Job>)> = pending.drain().collect();
    batch.sort_by(|a, b| a.0.cmp(&b.0));
    for (host, jobs) in &batch {
        let total: u32 = jobs.iter().map(|j| j.threads).sum();
        println!("{host}: {} job(s), {total} threads, first {:?}", jobs.len(), jobs[0]);
    }
    println!("pending after drain: {} (empty: {})", pending.len(), pending.is_empty());

    // The map is still usable after a drain.
    pending.insert("home".into(), vec![Job { script: "share.rs", threads: 1 }]);
    let mut left = 0;
    for (host, jobs) in pending.drain() {
        left += jobs.len();
        println!("late: {host} {}", jobs[0].script);
    }
    println!("{left} {}", pending.len());

    let mut hosts: Vec<String> = touched.drain().collect();
    hosts.sort();
    println!("{:?} {}", hosts, touched.len());

    let mut counts: HashMap<char, usize> = HashMap::new();
    for c in "hack grow weaken".chars().filter(|c| c.is_alphabetic()) {
        *counts.entry(c).or_insert(0) += 1;
    }
    let mut top: Vec<(char, usize)> = counts.drain().filter(|&(_, n)| n > 1).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    println!("{:?}", top);
    let total: usize = touched.drain().count() + counts.drain().map(|(_, n)| n).sum::<usize>();
    println!("{total}");
}
