// Rank the servers we could hack right now by money per security, and print a table.
use std::cmp::Ordering;

#[derive(Debug, Clone)]
struct Target {
    host: String,
    max_money: f64,
    min_sec: f64,
    level: i32,
}

impl Target {
    fn score(&self) -> f64 {
        self.max_money / self.min_sec
    }
}

fn collect(ns: &NS, host: &str, parent: Option<&str>, out: &mut Vec<String>) {
    out.push(host.to_string());
    for next in ns.scan(host) {
        if Some(next.as_str()) != parent {
            collect(ns, &next, Some(host), out);
        }
    }
}

pub async fn main(ns: NS) {
    let mut hosts = Vec::new();
    collect(&ns, "home", None, &mut hosts);
    let level = ns.get_hacking_level();

    let mut targets: Vec<Target> = hosts
        .iter()
        .filter(|h| h.as_str() != "home")
        .map(|h| Target {
            host: h.clone(),
            max_money: ns.get_server_max_money(h),
            min_sec: ns.get_server_min_security_level(h),
            level: ns.get_server_required_hacking_level(h),
        })
        .filter(|t| t.max_money > 0.0 && t.level <= level)
        .collect();
    targets.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal));

    ns.tprint(format!("{:<18}{:>16}{:>8}{:>6}", "host", "max money", "sec", "lvl"));
    for t in &targets {
        ns.tprint(format!("{:<18}{:>16.0}{:>8.1}{:>6}", t.host, t.max_money, t.min_sec, t.level));
    }
    match targets.first() {
        Some(best) => ns.tprint(format!("best: {} (score {:.1})", best.host, best.score())),
        None => ns.tprint("nothing to hack yet"),
    }
    let skipped = hosts.len() - 1 - targets.len();
    println!("{} servers scanned, {} skipped", hosts.len(), skipped);
}
