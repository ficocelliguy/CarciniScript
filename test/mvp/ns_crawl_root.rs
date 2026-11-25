// Walk the whole network from home, open what ports we can and nuke everything we're able to.
use std::collections::{HashSet, VecDeque};

fn open_ports(ns: &NS, host: &str) -> i32 {
    let mut opened = 0;
    if ns.file_exists("BruteSSH.exe", "home") {
        ns.brutessh(host);
        opened += 1;
    }
    if ns.file_exists("FTPCrack.exe", "home") {
        ns.ftpcrack(host);
        opened += 1;
    }
    if ns.file_exists("relaySMTP.exe", "home") {
        opened += 1;
    }
    opened
}

pub async fn main(ns: NS) {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    let mut order = Vec::new();
    queue.push_back("home".to_string());
    seen.insert("home".to_string());

    while let Some(host) = queue.pop_front() {
        for next in ns.scan(&host) {
            if seen.insert(next.clone()) {
                queue.push_back(next);
            }
        }
        order.push(host);
    }

    let mut rooted = 0;
    for host in &order {
        if ns.has_root_access(host) {
            rooted += 1;
            continue;
        }
        let needed = ns.get_server_num_ports_required(host);
        let opened = open_ports(&ns, host);
        if opened < needed {
            ns.tprint(format!("skip {host}: needs {needed} ports, have {opened}"));
            continue;
        }
        if ns.nuke(host) {
            ns.tprint(format!("rooted {host}"));
            rooted += 1;
        }
    }
    ns.tprint(format!("{}/{} servers rooted", rooted, order.len()));
}
