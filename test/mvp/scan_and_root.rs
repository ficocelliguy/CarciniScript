// Walk the whole network from here, open ports with whatever crackers we own, nuke what we can,
// then print a table of every server.
use std::collections::{HashSet, VecDeque};

struct ServerInfo {
    host: String,
    depth: usize,
    rooted: bool,
    req_level: i32,
    ports: i32,
    max_money: f64,
    ram: f64,
}

fn try_root(ns: &NS, host: &str) -> bool {
    if ns.has_root_access(host) {
        return true;
    }
    let mut opened = 0;
    if ns.file_exists("BruteSSH.exe", "home") {
        ns.brutessh(host);
        opened += 1;
    }
    if ns.file_exists("FTPCrack.exe", "home") {
        ns.ftpcrack(host);
        opened += 1;
    }
    if opened < ns.get_server_num_ports_required(host) {
        return false;
    }
    ns.nuke(host)
}

fn format_money(m: f64) -> String {
    if m >= 1e9 {
        format!("${:.2}b", m / 1e9)
    } else if m >= 1e6 {
        format!("${:.2}m", m / 1e6)
    } else if m >= 1e3 {
        format!("${:.2}k", m / 1e3)
    } else {
        format!("${:.0}", m)
    }
}

pub async fn main(ns: NS) {
    let start = ns.get_hostname();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue = VecDeque::new();
    let mut servers = Vec::new();

    seen.insert(start.clone());
    queue.push_back((start.clone(), 0));

    while let Some((host, depth)) = queue.pop_front() {
        for next in ns.scan(&host) {
            if seen.insert(next.clone()) {
                queue.push_back((next, depth + 1));
            }
        }
        if host == start {
            continue;
        }
        let rooted = try_root(&ns, &host);
        servers.push(ServerInfo {
            req_level: ns.get_server_required_hacking_level(&host),
            ports: ns.get_server_num_ports_required(&host),
            max_money: ns.get_server_max_money(&host),
            ram: ns.get_server_max_ram(&host),
            host,
            depth,
            rooted,
        });
    }

    servers.sort_by(|a, b| a.req_level.cmp(&b.req_level).then(a.host.cmp(&b.host)));

    let level = ns.get_hacking_level();
    ns.tprint(format!(
        "{:<18} {:>5} {:>5} {:>5} {:>6} {:>12} {:>5}",
        "server", "depth", "lvl", "ports", "root", "max money", "ram"
    ));
    for s in &servers {
        let marker = if s.req_level <= level { "" } else { " (too high)" };
        ns.tprint(format!(
            "{:<18} {:>5} {:>5} {:>5} {:>6} {:>12} {:>5}{}",
            s.host,
            s.depth,
            s.req_level,
            s.ports,
            if s.rooted { "yes" } else { "no" },
            format_money(s.max_money),
            s.ram,
            marker
        ));
    }

    let rooted = servers.iter().filter(|s| s.rooted).count();
    let total_ram: f64 = servers.iter().filter(|s| s.rooted).map(|s| s.ram).sum();
    ns.tprint(format!(
        "rooted {}/{} servers, {} GB of RAM available",
        rooted,
        servers.len(),
        total_ram
    ));
}
