//! Hacknet manager: always buys the upgrade with the best payback, within a budget
//! fraction of current money, then sells hashes for money (or spends them on a target).
//!
//! run hacknet.rs [budget-fraction=0.25] [hash-target]

use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Level,
    Ram,
    Cores,
    Cache,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Purchase {
    Node,
    Upgrade { node: usize, part: Part, cost: f64, gain: f64 },
}

impl Purchase {
    fn cost(&self, ns: &NS) -> f64 {
        match *self {
            Purchase::Node => ns.hacknet.get_purchase_node_cost(),
            Purchase::Upgrade { cost, .. } => cost,
        }
    }

    /// Seconds until the purchase pays for itself; lower is better.
    fn payback(&self, ns: &NS, avg_node_production: f64) -> f64 {
        let gain = match *self {
            Purchase::Node => avg_node_production,
            Purchase::Upgrade { gain, .. } => gain,
        };
        if gain <= 0.0 { f64::INFINITY } else { self.cost(ns) / gain }
    }

    fn buy(&self, ns: &NS) -> bool {
        match *self {
            Purchase::Node => ns.hacknet.purchase_node() >= 0.0,
            Purchase::Upgrade { node, part, .. } => {
                let i = node as f64;
                match part {
                    Part::Level => ns.hacknet.upgrade_level(i, 1),
                    Part::Ram => ns.hacknet.upgrade_ram(i, 1),
                    Part::Cores => ns.hacknet.upgrade_core(i, 1),
                    Part::Cache => ns.hacknet.upgrade_cache(i, 1),
                }
            }
        }
    }
}

impl fmt::Display for Purchase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Purchase::Node => f.write_str("new node"),
            Purchase::Upgrade { node, part, cost, .. } => {
                write!(f, "node {node} {part:?} (${cost:.0})")
            }
        }
    }
}

/// Production estimate for a node with the given stats, matching the game's formula
/// closely enough to rank upgrades.
fn production(level: f64, ram: f64, cores: f64, mult: f64) -> f64 {
    const PER_LEVEL: f64 = 1.5;
    level * PER_LEVEL * 1.035f64.powf(ram - 1.0) * ((cores + 5.0) / 6.0) * mult
}

fn candidates(ns: &NS, mult: f64) -> Vec<Purchase> {
    let nodes = ns.hacknet.num_nodes() as usize;
    let mut out = Vec::with_capacity(nodes * 4 + 1);
    for node in 0..nodes {
        let i = node as f64;
        let s = ns.hacknet.get_node_stats(i);
        let now = production(s.level, s.ram, s.cores, mult);
        let options = [
            (Part::Level, ns.hacknet.get_level_upgrade_cost(i, 1), production(s.level + 1.0, s.ram, s.cores, mult)),
            (Part::Ram, ns.hacknet.get_ram_upgrade_cost(i, 1), production(s.level, s.ram * 2.0, s.cores, mult)),
            (Part::Cores, ns.hacknet.get_core_upgrade_cost(i, 1), production(s.level, s.ram, s.cores + 1.0, mult)),
        ];
        out.extend(
            options
                .into_iter()
                .filter(|(_, cost, _)| cost.is_finite())
                .map(|(part, cost, after)| Purchase::Upgrade { node, part, cost, gain: after - now }),
        );
    }
    if (nodes as f64) < ns.hacknet.max_num_nodes() {
        out.push(Purchase::Node);
    }
    out
}

fn sell_hashes(ns: &NS, target: Option<&str>) -> u32 {
    const SELL: &str = "Sell for Money";
    const SECURITY: &str = "Reduce Minimum Security";
    let mut spent = 0;
    loop {
        let ok = match target {
            Some(host) if ns.hacknet.hash_cost(SECURITY, 1) <= ns.hacknet.num_hashes() => {
                ns.hacknet.spend_hashes(SECURITY, host, 1)
            }
            _ if ns.hacknet.hash_cost(SELL, 1) <= ns.hacknet.num_hashes() => ns.hacknet.spend_hashes(SELL),
            _ => false,
        };
        if !ok {
            break spent;
        }
        spent += 1;
    }
}

pub async fn main(ns: NS) {
    ns.disable_log("sleep");
    ns.disable_log("getServerMoneyAvailable");
    let budget: f64 = ns
        .args
        .first()
        .and_then(|a| a.as_f64())
        .unwrap_or(0.25)
        .clamp(0.0, 1.0);
    let hash_target = ns.args.get(1).and_then(JsValue::as_string);
    let mult = ns.get_player().mults.hacknet_node_money;

    let mut spent_total = 0.0;
    loop {
        let nodes = ns.hacknet.num_nodes() as usize;
        let avg = if nodes == 0 {
            production(1.0, 1.0, 1.0, mult)
        } else {
            (0..nodes)
                .map(|i| ns.hacknet.get_node_stats(i as f64).production)
                .sum::<f64>()
                / nodes as f64
        };

        let best = candidates(&ns, mult)
            .into_iter()
            .map(|p| (p.payback(&ns, avg), p))
            .min_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(Ordering::Equal));

        let money = ns.get_server_money_available("home");
        match best {
            Some((payback, p)) if p.cost(&ns) <= money * budget && payback < 60.0 * 60.0 * 4.0 => {
                let cost = p.cost(&ns);
                if p.buy(&ns) {
                    spent_total += cost;
                    ns.print(&format!("bought {p}, payback {}", ns.format.time(payback * 1000.0)));
                }
                ns.sleep(20).await;
            }
            Some((payback, p)) => {
                ns.print(&format!(
                    "waiting: next is {p}, payback {:.0}s; spent ${} so far",
                    payback,
                    ns.format.number(spent_total)
                ));
                ns.sleep(5_000).await;
            }
            None => {
                ns.print("hacknet maxed out");
                ns.sleep(30_000).await;
            }
        }

        let sold = sell_hashes(&ns, hash_target.as_deref());
        if sold > 0 {
            ns.print(&format!("spent hashes ×{sold}"));
        }
    }
}
