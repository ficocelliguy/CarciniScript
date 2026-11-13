// The starter script: crawl the network and nuke what needs no ports.
use std::collections::HashSet;

pub async fn main(ns: NS) {
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue = vec![String::from("home")];
    while let Some(host) = queue.pop() {
        if !seen.insert(host.clone()) {
            continue;
        }
        for next in ns.scan(&host) {
            queue.push(next);
        }
        // NS numbers are f64; ports are whole numbers, so match on an integer.
        let ports = ns.get_server_num_ports_required(&host) as u32;
        match (ports, ns.has_root_access(&host)) {
            (0, false) => {
                ns.nuke(&host);
            }
            (0, true) => {}
            (n, _) => ns.tprint(&format!("{host} needs {n} port(s)")),
        }
    }
    let mut names = Vec::new();
    for name in &seen {
        names.push(name.clone());
    }
    names.sort();
    ns.tprint(&format!("visited {} servers: {:?}", seen.len(), names));
}
