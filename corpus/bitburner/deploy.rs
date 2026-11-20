//! Roots everything it can, then fills every rooted server with `early_hack.rs` aimed at
//! the best target for the current hacking level.
//!
//! run deploy.rs [--kill]

mod net;

use net::{Host, try_root};

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

const WORKER: &str = "early_hack.rs";

/// Ranks a target; higher is better.
type Rank = Box<dyn Fn(&NS, &str) -> f64>;

/// How to rank targets. Boxed closures so `--strategy` can pick one at run time.
fn strategies() -> HashMap<&'static str, Rank> {
    let mut m: HashMap<&'static str, Rank> = HashMap::new();
    m.insert("money", Box::new(|ns, h| ns.get_server_max_money(h)));
    m.insert(
        "money-per-sec",
        Box::new(|ns, h| ns.get_server_max_money(h) * ns.hack_analyze_chance(h) / ns.get_weaken_time(h)),
    );
    m.insert("growth", Box::new(|ns, h| ns.get_server_growth(h)));
    m
}

fn eligible(ns: &NS, host: &str) -> bool {
    ns.has_root_access(host)
        && ns.get_server_max_money(host) > 0.0
        && ns.get_server_required_hacking_level(host) <= (ns.get_hacking_level() / 2).max(1)
}

/// Starts as many worker threads as fit; returns how many.
fn fill(ns: &NS, host: &Host, target: &str, script_ram: f64) -> u32 {
    let reserve = if &**host == "home" { net::HOME_RESERVE_GB } else { 0.0 };
    let free = ns.get_server_max_ram(host) - ns.get_server_used_ram(host) - reserve;
    let threads = (free / script_ram).floor() as u32;
    if threads == 0 {
        return 0;
    }
    if &**host != "home" {
        ns.scp(WORKER, host, "home");
    }
    match ns.exec(WORKER, host, threads as i32, target) {
        0 => 0,
        _ => threads,
    }
}

pub async fn main(ns: NS) {
    let kill_first = ns.args.iter().any(|a| a.as_string().as_deref() == Some("--kill"));
    let strategy_name = ns
        .args
        .iter()
        .filter_map(|a| a.as_string())
        .find_map(|a| a.strip_prefix("--strategy=").map(str::to_owned))
        .unwrap_or_else(|| "money-per-sec".to_owned());

    let strategies = strategies();
    let Some(rank) = strategies.get(strategy_name.as_str()) else {
        let mut names: Vec<_> = strategies.keys().collect();
        names.sort();
        ns.tprint(&format!("unknown strategy {strategy_name}; try one of {names:?}"));
        return;
    };

    let hosts = net::all_hosts(&ns);
    let failures: Rc<RefCell<Vec<(Host, String)>>> = Rc::new(RefCell::new(Vec::new()));
    let rooted: Vec<&Host> = hosts
        .iter()
        .filter(|h| match try_root(&ns, h) {
            Ok(ok) => ok,
            Err(e) => {
                failures.borrow_mut().push(((*h).clone(), e.to_string()));
                false
            }
        })
        .collect();

    let target = hosts
        .iter()
        .filter(|h| eligible(&ns, h))
        .max_by(|a, b| rank(&ns, a).total_cmp(&rank(&ns, b)))
        .cloned()
        .unwrap_or_else(|| Host::from("n00dles"));

    let script_ram = ns.get_script_ram(WORKER);
    let mut started = 0;
    for host in &rooted {
        if kill_first {
            ns.script_kill(WORKER, host);
        }
        started += fill(&ns, host, &target, script_ram);
    }

    ns.tprint(&format!(
        "rooted {}/{} servers, {started} threads on {target} (strategy {strategy_name})",
        rooted.len(),
        hosts.len(),
    ));
    let failures = failures.borrow();
    if !failures.is_empty() {
        let summary = failures
            .iter()
            .take(5)
            .map(|(h, why)| format!("  {h}: {why}"))
            .collect::<Vec<_>>()
            .join("\n");
        ns.tprint(&format!("not rooted ({}):\n{summary}", failures.len()));
    }
}
