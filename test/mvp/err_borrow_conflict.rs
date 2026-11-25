// Grow a list of hosts while walking it: the classic BFS-without-a-queue mistake.

fn neighbors(host: &str) -> Vec<String> {
    match host {
        "home" => vec!["n00dles".to_string(), "foodnstuff".to_string()],
        "n00dles" => vec!["CSEC".to_string()],
        _ => vec![],
    }
}

fn main() {
    let mut hosts = vec!["home".to_string()];
    for host in &hosts {
        for next in neighbors(host) {
            if !hosts.contains(&next) {
                hosts.push(next);
            }
        }
    }
    println!("{:?}", hosts);
}
